/// ternlang-mcp — Model Context Protocol server
///
/// Turns any binary AI agent into a ternary decision engine.
/// Transport: JSON-RPC 2.0 over stdio (MCP standard).
///
/// Tools exposed:
///   trit_decide       — scalar ternary decision: evidence[] → reject/tend/affirm + confidence
///   trit_vector       — multi-dimensional evidence aggregation with named dimensions + weights
///   trit_consensus    — consensus(a, b) → ternary result
///   trit_eval         — evaluate a trit expression string
///   ternlang_run      — compile + run a .tern snippet
///   quantize_weights  — f32 array → ternary weights
///   sparse_benchmark  — run sparse vs dense matmul, report skip rate

use std::io::{self, BufRead, Write};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ternlang_core::{trit::Trit, parser::Parser, codegen::betbc::BytecodeEmitter, vm::BetVm};
use ternlang_ml::{TritMatrix, TritScalar, TritEvidenceVec, TEND_BOUNDARY,
                   bitnet_threshold, benchmark, dense_matmul, sparse_matmul};

// ─── JSON-RPC types ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id:      Option<Value>,
    method:  String,
    params:  Option<Value>,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: String,
    id:      Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result:  Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error:   Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code:    i32,
    message: String,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }
    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None,
               error: Some(RpcError { code, message: message.into() }) }
    }
}

// ─── Trit helpers ────────────────────────────────────────────────────────────

fn trit_to_i8(t: Trit) -> i8 {
    match t { Trit::NegOne => -1, Trit::Zero => 0, Trit::PosOne => 1 }
}

fn trit_label(t: Trit) -> &'static str {
    match t { Trit::NegOne => "reject", Trit::Zero => "tend", Trit::PosOne => "affirm" }
}

fn i8_to_trit(v: i64) -> Option<Trit> {
    match v { -1 => Some(Trit::NegOne), 0 => Some(Trit::Zero), 1 => Some(Trit::PosOne), _ => None }
}

// ─── Tool: trit_decide ───────────────────────────────────────────────────────
//
// THE flagship tool. Any agent passes in floating-point evidence signals.
// We quantize to ternary, run consensus across all signals, return a decision.
//
// This is what "turns a binary agent into a ternary decision engine" means:
// the agent no longer has to collapse ambiguous evidence into YES/NO.
// It gets HOLD — a computational instruction to gather more before acting.

fn tool_trit_decide(params: &Value) -> Result<Value, String> {
    let evidence: Vec<f32> = params["evidence"]
        .as_array()
        .ok_or("evidence must be an array of numbers")?
        .iter()
        .map(|v| v.as_f64().ok_or("evidence values must be numbers").map(|f| f as f32))
        .collect::<Result<_, _>>()?;

    if evidence.is_empty() {
        return Err("evidence array cannot be empty".into());
    }

    let min_confidence = params["min_confidence"].as_f64().unwrap_or(0.0) as f32;

    // Mean of clamped evidence values → aggregate TritScalar
    let mean: f32 = evidence.iter().sum::<f32>() / evidence.len() as f32;
    let scalar = TritScalar::new(mean);

    // Per-signal breakdown
    let per_signal: Vec<Value> = evidence.iter().enumerate().map(|(i, &v)| {
        let s = TritScalar::new(v);
        json!({
            "index": i,
            "raw": (v * 1000.0).round() / 1000.0,
            "label": s.label(),
            "confidence": (s.confidence() * 1000.0).round() / 1000.0,
            "trit": trit_to_i8(s.trit()),
        })
    }).collect();

    let zeros = per_signal.iter().filter(|s| s["trit"] == 0).count();
    let sparsity = zeros as f64 / evidence.len() as f64;
    let actionable = scalar.is_actionable(min_confidence);

    let recommendation = match scalar.trit() {
        Trit::PosOne => format!(
            "Affirm — confidence {:.0}%{}. Proceed with action.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence threshold — gather more evidence)" }
        ),
        Trit::NegOne => format!(
            "Reject — confidence {:.0}%{}. Do not proceed.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence threshold — gather more evidence)" }
        ),
        Trit::Zero => format!(
            "Tend — scalar {:.3} is within the deliberation zone [{:.3}, +{:.3}]. \
             Gather more evidence before acting.",
            scalar.raw(), -TEND_BOUNDARY, TEND_BOUNDARY
        ),
    };

    Ok(json!({
        "scalar":          (scalar.raw() * 1000.0).round() / 1000.0,
        "trit":            trit_to_i8(scalar.trit()),
        "label":           scalar.label(),
        "confidence":      (scalar.confidence() * 1000.0).round() / 1000.0,
        "is_actionable":   actionable,
        "tend_boundary":   TEND_BOUNDARY,
        "signal_sparsity": sparsity,
        "recommendation":  recommendation,
        "per_signal":      per_signal,
    }))
}

// ─── Tool: trit_vector ───────────────────────────────────────────────────────
//
// Multi-dimensional evidence aggregation with named dimensions and weights.
// The flagship agent-reasoning tool: give it your evidence sources by name,
// get back an aggregate scalar decision + per-dimension breakdown.

fn tool_trit_vector(params: &Value) -> Result<Value, String> {
    let dims = params["dimensions"]
        .as_array()
        .ok_or("dimensions must be an array of {label, value, weight} objects")?;

    if dims.is_empty() {
        return Err("dimensions cannot be empty".into());
    }

    let min_confidence = params["min_confidence"].as_f64().unwrap_or(0.5) as f32;

    let mut labels  = Vec::new();
    let mut values  = Vec::new();
    let mut weights = Vec::new();

    for (i, d) in dims.iter().enumerate() {
        let label  = d["label"].as_str()
            .unwrap_or_else(|| "unnamed")
            .to_string();
        let value  = d["value"].as_f64()
            .ok_or(format!("dimension[{}].value must be a number", i))? as f32;
        let weight = d["weight"].as_f64().unwrap_or(1.0) as f32;
        if weight < 0.0 { return Err(format!("dimension[{}].weight must be >= 0", i)); }
        labels.push(label);
        values.push(value);
        weights.push(weight);
    }

    let ev  = TritEvidenceVec::new(labels, values, weights);
    let agg = ev.aggregate();
    let scalars = ev.scalars();

    let breakdown: Vec<Value> = ev.dimensions.iter()
        .zip(ev.values.iter())
        .zip(ev.weights.iter())
        .zip(scalars.iter())
        .map(|(((label, &raw), &weight), s)| json!({
            "label":      label,
            "raw":        (raw * 1000.0).round() / 1000.0,
            "weight":     weight,
            "trit":       trit_to_i8(s.trit()),
            "zone":       s.label(),
            "confidence": (s.confidence() * 1000.0).round() / 1000.0,
        }))
        .collect();

    let dominant = ev.dominant().map(|(label, s)| json!({
        "label":      label,
        "zone":       s.label(),
        "confidence": (s.confidence() * 1000.0).round() / 1000.0,
    }));

    let actionable = agg.is_actionable(min_confidence);

    let recommendation = match agg.trit() {
        Trit::PosOne => format!(
            "Affirm — aggregate scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Act." } else { ". Confidence below threshold — continue gathering evidence." }
        ),
        Trit::NegOne => format!(
            "Reject — aggregate scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Do not act." } else { ". Confidence below threshold — continue gathering evidence." }
        ),
        Trit::Zero => format!(
            "Tend — aggregate scalar {:.3} is in the deliberation zone [{:.3}, +{:.3}]. \
             Do not act yet. Strongest signal: {}.",
            agg.raw(), -TEND_BOUNDARY, TEND_BOUNDARY,
            ev.dominant().map(|(l, _)| l).unwrap_or("none")
        ),
    };

    Ok(json!({
        "aggregate": {
            "scalar":       (agg.raw() * 1000.0).round() / 1000.0,
            "trit":         trit_to_i8(agg.trit()),
            "label":        agg.label(),
            "confidence":   (agg.confidence() * 1000.0).round() / 1000.0,
            "is_actionable": actionable,
        },
        "breakdown":      breakdown,
        "dominant":       dominant,
        "tend_boundary":  TEND_BOUNDARY,
        "recommendation": recommendation,
    }))
}

// ─── Tool: trit_consensus ────────────────────────────────────────────────────

fn tool_trit_consensus(params: &Value) -> Result<Value, String> {
    let a_val = params["a"].as_i64().ok_or("a must be -1, 0, or 1")?;
    let b_val = params["b"].as_i64().ok_or("b must be -1, 0, or 1")?;
    let a = i8_to_trit(a_val).ok_or(format!("a={} is not a valid trit (-1, 0, 1)", a_val))?;
    let b = i8_to_trit(b_val).ok_or(format!("b={} is not a valid trit (-1, 0, 1)", b_val))?;
    let (sum, carry) = a + b;
    Ok(json!({
        "result": trit_to_i8(sum),
        "label": trit_label(sum),
        "carry": trit_to_i8(carry),
        "expression": format!("consensus({}, {}) = {}", a_val, b_val, trit_to_i8(sum))
    }))
}

// ─── Tool: trit_eval ─────────────────────────────────────────────────────────

fn tool_trit_eval(params: &Value) -> Result<Value, String> {
    let code = params["expression"].as_str().ok_or("expression must be a string")?;
    // Wrap bare expression in a return statement if needed
    let full_code = if code.trim_end().ends_with(';') {
        code.to_string()
    } else {
        format!("return {};", code)
    };

    let mut parser = Parser::new(&full_code);
    let mut emitter = BytecodeEmitter::new();
    loop {
        match parser.parse_stmt() {
            Ok(stmt) => emitter.emit_stmt(&stmt),
            Err(e) => {
                if format!("{:?}", e).contains("EOF") { break; }
                return Err(format!("parse error: {:?}", e));
            }
        }
    }
    let code_bytes = emitter.finalize();
    let mut vm = BetVm::new(code_bytes);
    vm.run().map_err(|e| format!("vm error: {}", e))?;

    let reg0 = vm.get_register(0);
    Ok(json!({
        "expression": params["expression"],
        "result_register_0": format!("{:?}", reg0)
    }))
}

// ─── Tool: ternlang_run ──────────────────────────────────────────────────────

fn tool_ternlang_run(params: &Value) -> Result<Value, String> {
    let code = params["code"].as_str().ok_or("code must be a string")?;

    let mut parser = Parser::new(code);
    let mut emitter = BytecodeEmitter::new();

    match parser.parse_program() {
        Ok(prog) => emitter.emit_program(&prog),
        Err(_) => {
            let mut p2 = Parser::new(code);
            loop {
                match p2.parse_stmt() {
                    Ok(stmt) => emitter.emit_stmt(&stmt),
                    Err(e) => {
                        if format!("{:?}", e).contains("EOF") { break; }
                        return Err(format!("parse error: {:?}", e));
                    }
                }
            }
        }
    }

    let bytecode = emitter.finalize();
    let bytecode_len = bytecode.len();
    let mut vm = BetVm::new(bytecode);
    vm.run().map_err(|e| format!("vm error: {}", e))?;

    let registers: Vec<Value> = (0..10).map(|i| {
        format!("{:?}", vm.get_register(i)).into()
    }).collect();

    Ok(json!({
        "status": "ok",
        "bytecode_bytes": bytecode_len,
        "registers": registers
    }))
}

// ─── Tool: quantize_weights ──────────────────────────────────────────────────

fn tool_quantize_weights(params: &Value) -> Result<Value, String> {
    let weights: Vec<f32> = params["weights"]
        .as_array().ok_or("weights must be an array")?
        .iter()
        .map(|v| v.as_f64().ok_or("weight values must be numbers").map(|f| f as f32))
        .collect::<Result<_, _>>()?;

    let threshold = params["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;

    let trits: Vec<i8> = weights.iter().map(|&w| {
        if w > threshold { 1 }
        else if w < -threshold { -1 }
        else { 0 }
    }).collect();

    let zeros = trits.iter().filter(|&&t| t == 0).count();
    let sparsity = zeros as f64 / trits.len() as f64;

    Ok(json!({
        "trits": trits,
        "threshold_used": threshold,
        "sparsity": sparsity,
        "nnz": trits.len() - zeros,
        "total": trits.len()
    }))
}

// ─── Tool: sparse_benchmark ──────────────────────────────────────────────────

fn tool_sparse_benchmark(params: &Value) -> Result<Value, String> {
    let rows = params["rows"].as_u64().unwrap_or(4) as usize;
    let cols = params["cols"].as_u64().unwrap_or(4) as usize;

    let weights: Vec<f32> = match params["weights"].as_array() {
        Some(arr) => arr.iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect(),
        None => {
            // Generate a representative ternary weight distribution if none provided
            (0..rows*cols).map(|i| {
                match i % 5 { 0 => 0.9, 1 => -0.8, 2 => 0.1, 3 => -0.1, _ => 0.05 }
            }).collect()
        }
    };

    if weights.len() != rows * cols {
        return Err(format!("weights length {} must equal rows×cols = {}×{}={}", weights.len(), rows, cols, rows*cols));
    }

    let threshold = params["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;

    let w = TritMatrix::from_f32(rows, cols, &weights, threshold);
    let input = TritMatrix::new(rows, cols); // zero input — measures weight sparsity purely

    let r = benchmark(&input, &w);
    let (_dense_result, _) = { let i2 = TritMatrix::new(rows, cols); (dense_matmul(&i2, &w), 0) };
    let (_, skipped) = sparse_matmul(&input, &w);

    Ok(json!({
        "rows": rows,
        "cols": cols,
        "weight_sparsity": r.weight_sparsity,
        "skip_rate": r.skip_rate,
        "dense_ops": r.dense_ops,
        "sparse_ops": r.sparse_ops,
        "skipped_ops": skipped,
        "ops_reduction_factor": r.dense_ops as f64 / r.sparse_ops.max(1) as f64,
        "threshold_used": threshold,
        "summary": format!(
            "{:.1}% weight sparsity → {:.1}x fewer multiply ops ({} skipped of {})",
            r.weight_sparsity * 100.0,
            r.dense_ops as f64 / r.sparse_ops.max(1) as f64,
            skipped,
            r.dense_ops
        )
    }))
}

// ─── Tool dispatch ───────────────────────────────────────────────────────────

fn dispatch_tool(name: &str, params: &Value) -> Result<Value, String> {
    match name {
        "trit_decide"       => tool_trit_decide(params),
        "trit_vector"       => tool_trit_vector(params),
        "trit_consensus"    => tool_trit_consensus(params),
        "trit_eval"         => tool_trit_eval(params),
        "ternlang_run"      => tool_ternlang_run(params),
        "quantize_weights"  => tool_quantize_weights(params),
        "sparse_benchmark"  => tool_sparse_benchmark(params),
        _ => Err(format!("unknown tool: {}", name)),
    }
}

// ─── Tool manifest ───────────────────────────────────────────────────────────

fn tools_list() -> Value {
    json!({ "tools": [
        {
            "name": "trit_decide",
            "description": "Scalar ternary decision engine. Pass in floating-point evidence signals on [-1.0, +1.0]. Returns a continuous scalar temperature, a discrete ternary decision (reject/tend/affirm), and a confidence score.\n\nThe three zones:\n  affirm  (+0.33, +1.0] — signal is affirmative. Act if confidence is high enough.\n  tend    [-0.33, +0.33] — deliberation zone. Do NOT act. Gather more evidence.\n  reject  [-1.0, -0.33) — signal is negative. Do not proceed.\n\nThe 'tend' zone is the key innovation: it is not null or undecided — it is an active computational instruction to remain in uncertainty until the scalar clears a boundary. Confidence tells you HOW decisive the signal is within its zone.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "evidence": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Array of numeric evidence signals on [-1.0, +1.0]. Positive = supporting, negative = contradicting, near-zero = uncertain. Mean is computed as the aggregate scalar."
                    },
                    "min_confidence": {
                        "type": "number",
                        "description": "Minimum confidence threshold for is_actionable (0.0–1.0). Default 0.0 (any decisive signal is actionable)."
                    }
                },
                "required": ["evidence"]
            }
        },
        {
            "name": "trit_vector",
            "description": "Multi-dimensional ternary evidence aggregation. The full agent reasoning tool.\n\nProvide named evidence dimensions, each with a scalar value [-1.0, +1.0] and an importance weight. The engine computes a weighted-mean aggregate TritScalar and returns:\n  - aggregate: scalar, trit, label (reject/tend/affirm), confidence, is_actionable\n  - breakdown: per-dimension zone classification\n  - dominant: which evidence dimension is pulling hardest\n  - recommendation: plain-language decision guidance\n\nUse case: an AI agent collects evidence from multiple sources (visual, textual, contextual, historical) before deciding whether to act. The tend zone means 'keep deliberating'. Only act when is_actionable is true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dimensions": {
                        "type": "array",
                        "description": "Array of evidence dimensions. Each: {label: string, value: number [-1,1], weight: number >= 0 (default 1.0)}",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label":  { "type": "string",  "description": "Name of this evidence source" },
                                "value":  { "type": "number",  "description": "Evidence scalar, clamped to [-1.0, +1.0]" },
                                "weight": { "type": "number",  "description": "Importance weight (default 1.0)" }
                            },
                            "required": ["label", "value"]
                        }
                    },
                    "min_confidence": {
                        "type": "number",
                        "description": "Minimum confidence for is_actionable (0.0–1.0). Default 0.5."
                    }
                },
                "required": ["dimensions"]
            }
        },
        {
            "name": "trit_consensus",
            "description": "Compute balanced ternary consensus between two trit signals.\n\nConsensus is the core ternary addition operator: truth+conflict=hold, truth+truth=truth, conflict+conflict=conflict.\n\nUse this to merge two independent ternary judgements into a single signal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "a": { "type": "integer", "enum": [-1, 0, 1], "description": "First trit: -1=conflict, 0=hold, 1=truth" },
                    "b": { "type": "integer", "enum": [-1, 0, 1], "description": "Second trit: -1=conflict, 0=hold, 1=truth" }
                },
                "required": ["a", "b"]
            }
        },
        {
            "name": "trit_eval",
            "description": "Evaluate a ternary expression using the BET (Balanced Ternary Execution) VM.\n\nSupports: consensus(a,b), invert(x), truth(), hold(), conflict(), arithmetic (+, -, *), let bindings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "expression": { "type": "string", "description": "A ternlang expression or statement. E.g. 'consensus(1, -1)' or 'let x: trit = 1; return -x;'" }
                },
                "required": ["expression"]
            }
        },
        {
            "name": "ternlang_run",
            "description": "Compile and execute a full ternlang (.tern) program on the BET VM. Returns register state after execution.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Full ternlang source code to compile and run." }
                },
                "required": ["code"]
            }
        },
        {
            "name": "quantize_weights",
            "description": "Quantize an array of float weights to balanced ternary (-1, 0, +1) using BitNet-style thresholding. Returns the trit vector and sparsity statistics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "weights": { "type": "array", "items": { "type": "number" }, "description": "Float weight values to quantize." },
                    "threshold": { "type": "number", "description": "Quantization threshold. Omit to auto-compute via BitNet formula." }
                },
                "required": ["weights"]
            }
        },
        {
            "name": "sparse_benchmark",
            "description": "Run sparse vs dense ternary matrix multiply benchmark. Shows how many multiply-accumulate operations are skipped due to zero-state (hold) weights. Demonstrates the computational efficiency of ternary AI inference.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "rows":      { "type": "integer", "description": "Matrix rows (default 4)" },
                    "cols":      { "type": "integer", "description": "Matrix cols (default 4)" },
                    "weights":   { "type": "array", "items": { "type": "number" }, "description": "Flat row-major float weights. Length must equal rows×cols. Omit for a demo distribution." },
                    "threshold": { "type": "number", "description": "Quantization threshold. Omit to auto-compute." }
                }
            }
        }
    ]})
}

// ─── MCP initialize response ─────────────────────────────────────────────────

fn initialize_response() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name":    "ternlang-mcp",
            "version": "0.1.0",
            "description": "Ternary Intelligence Stack — turns binary AI agents into ternary decision engines. Built by RFI-IRFOS."
        }
    })
}

// ─── Main loop ───────────────────────────────────────────────────────────────

fn handle_request(req: RpcRequest) -> RpcResponse {
    let id = req.id.unwrap_or(Value::Null);
    let params = req.params.unwrap_or(Value::Object(Default::default()));

    match req.method.as_str() {
        "initialize" => RpcResponse::ok(id, initialize_response()),

        "notifications/initialized" => {
            // Client confirmation — no response needed, but send ok
            RpcResponse::ok(id, json!({}))
        }

        "tools/list" => RpcResponse::ok(id, tools_list()),

        "tools/call" => {
            let tool_name = match params["name"].as_str() {
                Some(n) => n.to_string(),
                None => return RpcResponse::err(id, -32602, "missing tool name"),
            };
            let tool_params = &params["arguments"];
            match dispatch_tool(&tool_name, tool_params) {
                Ok(result) => RpcResponse::ok(id, json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                    }]
                })),
                Err(e) => RpcResponse::err(id, -32000, e),
            }
        }

        other => RpcResponse::err(id, -32601, format!("method not found: {}", other)),
    }
}

fn main() {
    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    eprintln!("[ternlang-mcp] server ready — ternary intelligence stack v0.2");
    eprintln!("[ternlang-mcp] waiting for MCP client on stdin...");

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(req) => handle_request(req),
            Err(e)  => RpcResponse::err(
                Value::Null, -32700,
                format!("parse error: {}", e)
            ),
        };

        let json = serde_json::to_string(&response).unwrap_or_default();
        writeln!(out, "{}", json).ok();
        out.flush().ok();
    }
}
