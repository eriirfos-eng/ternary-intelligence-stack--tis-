/// ternlang-mcp — Model Context Protocol server
///
/// Turns any binary AI agent into a ternary decision engine.
/// Transport: JSON-RPC 2.0 over stdio (MCP standard).
///
/// Tools exposed:
///   trit_decide       — the flagship: evidence → -1/0/+1 ternary decision
///   trit_consensus    — consensus(a, b) → ternary result
///   trit_eval         — evaluate a trit expression string
///   ternlang_run      — compile + run a .tern snippet
///   quantize_weights  — f32 array → ternary weights
///   sparse_benchmark  — run sparse vs dense matmul, report skip rate

use std::io::{self, BufRead, Write};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ternlang_core::{trit::Trit, parser::Parser, codegen::betbc::BytecodeEmitter, vm::BetVm};
use ternlang_ml::{TritMatrix, bitnet_threshold, benchmark, dense_matmul, sparse_matmul};

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

fn trit_semantic(t: Trit) -> &'static str {
    match t { Trit::NegOne => "conflict", Trit::Zero => "hold", Trit::PosOne => "truth" }
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

    let threshold = params["threshold"].as_f64().unwrap_or_else(|| {
        // Auto-compute BitNet threshold if not provided
        let mean_abs = evidence.iter().map(|w| w.abs()).sum::<f32>() / evidence.len() as f32;
        (0.5 * mean_abs) as f64
    }) as f32;

    // Quantize each signal to a trit
    let trits: Vec<Trit> = evidence.iter().map(|&e| {
        if e > threshold { Trit::PosOne }
        else if e < -threshold { Trit::NegOne }
        else { Trit::Zero }
    }).collect();

    // Consensus across all signals: sum, clamp to [-1, 0, +1]
    let raw_sum: i32 = trits.iter().map(|&t| trit_to_i8(t) as i32).sum();
    let decision = if raw_sum > 0 { Trit::PosOne }
                   else if raw_sum < 0 { Trit::NegOne }
                   else { Trit::Zero };

    // Confidence: how far from zero is the raw sum, relative to max possible
    let max_sum = evidence.len() as f32;
    let confidence_raw = raw_sum.abs() as f32 / max_sum;
    let confidence = if confidence_raw > 0.66 { "high" }
                     else if confidence_raw > 0.33 { "medium" }
                     else { "low" };

    let zeros = trits.iter().filter(|&&t| t == Trit::Zero).count();
    let sparsity = zeros as f64 / trits.len() as f64;

    let trit_repr: Vec<i8> = trits.iter().map(|&t| trit_to_i8(t)).collect();

    Ok(json!({
        "decision": trit_to_i8(decision),
        "semantic": trit_semantic(decision),
        "confidence": confidence,
        "raw_consensus": raw_sum,
        "threshold_used": threshold,
        "quantized_trits": trit_repr,
        "signal_sparsity": sparsity,
        "interpretation": match decision {
            Trit::PosOne  => "Evidence supports action. Proceed.",
            Trit::NegOne  => "Evidence contradicts action. Do not proceed.",
            Trit::Zero    => "Evidence is ambiguous. Hold — gather more information before deciding.",
        }
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
        "semantic": trit_semantic(sum),
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
            "description": "THE flagship tool. Turns any binary agent into a ternary decision engine.\n\nPass in floating-point evidence signals (your confidence scores, probabilities, sentiment values, anything numeric). The engine quantizes them to balanced ternary (-1/0/+1), runs consensus across all signals, and returns a ternary decision:\n\n  +1 (truth)    → evidence supports action. Proceed.\n   0 (hold)     → evidence is ambiguous. Gather more before deciding.\n  -1 (conflict) → evidence contradicts action. Do not proceed.\n\nThe HOLD state is the key innovation: instead of forcing a binary YES/NO from ambiguous evidence, the agent gets a computational instruction to stay in uncertainty until it has enough signal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "evidence": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Array of numeric evidence signals (confidence scores, probabilities, sentiment values, etc). Positive = supporting, negative = contradicting, near-zero = uncertain."
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Quantization threshold τ. Values with |e| > τ become ±1, others become 0 (hold). Omit to auto-compute using BitNet formula: 0.5 × mean(|evidence|)."
                    }
                },
                "required": ["evidence"]
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

    eprintln!("[ternlang-mcp] server ready — ternary intelligence stack v0.1");
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
