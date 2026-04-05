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
                   bitnet_threshold, benchmark, dense_matmul, sparse_matmul,
                   DeliberationEngine, action_gate, GateDimension, GateVerdict};
use ternlang_moe::TernMoeOrchestrator;

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
    match t { Trit::Reject => -1, Trit::Tend => 0, Trit::Affirm => 1 }
}

fn trit_label(t: Trit) -> &'static str {
    match t { Trit::Reject => "reject", Trit::Tend => "tend", Trit::Affirm => "affirm" }
}

fn i8_to_trit(v: i64) -> Option<Trit> {
    match v { -1 => Some(Trit::Reject), 0 => Some(Trit::Tend), 1 => Some(Trit::Affirm), _ => None }
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
        Trit::Affirm => format!(
            "Affirm — confidence {:.0}%{}. Proceed with action.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence threshold — gather more evidence)" }
        ),
        Trit::Reject => format!(
            "Reject — confidence {:.0}%{}. Do not proceed.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence threshold — gather more evidence)" }
        ),
        Trit::Tend => format!(
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
        Trit::Affirm => format!(
            "Affirm — aggregate scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Act." } else { ". Confidence below threshold — continue gathering evidence." }
        ),
        Trit::Reject => format!(
            "Reject — aggregate scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Do not act." } else { ". Confidence below threshold — continue gathering evidence." }
        ),
        Trit::Tend => format!(
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

// ─── Tool: moe_orchestrate ───────────────────────────────────────────────────
//
// Full MoE-13 ternary orchestration pass.
// Routes through the 13-expert pool, synthesises triad field, runs safety gate,
// votes, detects hold, optionally calls tiebreaker. Returns full OrchestrationResult.

fn tool_moe_orchestrate(params: &Value) -> Result<Value, String> {
    let query = params["query"].as_str().ok_or("query must be a string")?;

    // Parse evidence vector (6 dims: syntax/world_knowledge/reasoning/tool_use/persona/safety)
    let evidence: Vec<f32> = match params["evidence"].as_array() {
        Some(arr) => arr.iter()
            .map(|v| v.as_f64().ok_or("evidence values must be numbers").map(|f| f as f32))
            .collect::<Result<_, _>>()?,
        None => vec![0.0f32; 6],
    };

    let mut orch = TernMoeOrchestrator::with_standard_experts();
    let result = orch.orchestrate(query, &evidence);

    let trit_label = match result.trit {
        1  => "affirm",
        -1 => "reject",
        _  => "tend",
    };

    let verdicts: Vec<Value> = result.verdicts.iter().map(|v| json!({
        "expert_id":   v.expert_id,
        "expert_name": v.expert_name,
        "trit":        v.trit,
        "confidence":  (v.confidence * 1000.0).round() / 1000.0,
        "reasoning":   v.reasoning,
    })).collect();

    let pair_info = result.pair.as_ref().map(|p| json!({
        "expert_a":  p.expert_a,
        "expert_b":  p.expert_b,
        "relevance": (p.relevance * 1000.0).round() / 1000.0,
        "synergy":   (p.synergy  * 1000.0).round() / 1000.0,
        "combined":  (p.combined * 1000.0).round() / 1000.0,
    }));

    Ok(json!({
        "trit":           result.trit,
        "label":          trit_label,
        "confidence":     (result.confidence * 1000.0).round() / 1000.0,
        "held":           result.held,
        "safety_vetoed":  result.safety_vetoed,
        "temperature":    (result.temperature * 1000.0).round() / 1000.0,
        "prompt_hint":    result.prompt_hint,
        "triad_field":    {
            "synergy_weight":    (result.triad_field.synergy_weight * 1000.0).round() / 1000.0,
            "field":             result.triad_field.field.raw,
            "is_amplifying":     result.triad_field.is_amplifying(),
        },
        "routing_pair":   pair_info,
        "verdicts":       verdicts,
    }))
}

// ─── Tool: moe_deliberate ────────────────────────────────────────────────────
//
// Run the EMA deliberation engine — converges scalar toward target confidence
// over multiple rounds. Useful when initial evidence is weak and you want to
// simulate iterative reasoning (like a human saying "let me think about this").

fn tool_moe_deliberate(params: &Value) -> Result<Value, String> {
    let initial = params["initial_scalar"]
        .as_f64().ok_or("initial_scalar must be a number")? as f32;
    let target  = params["target_confidence"]
        .as_f64().unwrap_or(0.8) as f32;
    let alpha   = params["alpha"].as_f64().unwrap_or(0.4) as f32;
    let max_rounds = params["max_rounds"].as_u64().unwrap_or(10) as usize;

    // Evidence updates — each round can inject new signals
    let updates: Vec<f32> = match params["evidence_updates"].as_array() {
        Some(arr) => arr.iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect(),
        None => vec![initial; max_rounds], // no updates → converge from initial alone
    };

    let engine = DeliberationEngine::new(target, max_rounds)
        .with_alpha(alpha);

    // `run()` takes Vec<Vec<f32>> — each outer entry is one round's signals
    let rounds_evidence: Vec<Vec<f32>> = updates.iter().map(|&v| vec![v]).collect();
    let result = engine.run(rounds_evidence);

    let trit_label = match result.final_trit {
        1  => "affirm",
        -1 => "reject",
        _  => "tend",
    };

    let rounds: Vec<Value> = result.trace.iter().map(|r| json!({
        "round":      r.round,
        "scalar":     (r.scalar.raw() * 1000.0).round() / 1000.0,
        "confidence": (r.scalar.confidence() * 1000.0).round() / 1000.0,
        "trit":       r.scalar.trit_i8(),
        "converged":  r.converged,
    })).collect();

    Ok(json!({
        "final_confidence": (result.final_confidence * 1000.0).round() / 1000.0,
        "trit":             result.final_trit,
        "label":            trit_label,
        "converged":        result.converged,
        "rounds_used":      result.rounds_used,
        "target_confidence": target,
        "convergence_reason": result.convergence_reason,
        "rounds":           rounds,
        "summary": format!(
            "{} after {} round(s) — confidence {:.0}% (target {:.0}%) — {}",
            trit_label.to_uppercase(),
            result.rounds_used,
            result.final_confidence * 100.0,
            target * 100.0,
            if result.converged { "converged" } else { "did not converge (held)" }
        ),
    }))
}

// ─── Tool: trit_action_gate ──────────────────────────────────────────────────
//
// Multi-dimensional hard-block safety gate.
// Any dimension marked hard_block:true with a negative signal VETOES the action.
// All other dims contribute to the weighted pass/block vote.
// This is the structural safety layer: it fires before any other reasoning.

fn tool_trit_action_gate(params: &Value) -> Result<Value, String> {
    let dims_raw = params["dimensions"]
        .as_array().ok_or("dimensions must be an array")?;

    // GateDimension uses `name` + `evidence` (f32 signal), not label+trit
    // evidence on [-1,1]: positive = pass signal, negative = block signal
    let dims: Vec<GateDimension> = dims_raw.iter().map(|d| {
        let name       = d["label"].as_str().unwrap_or("dim").to_string();
        // Accept either `trit` (-1/0/1) or `evidence` (-1.0..1.0); trit → float
        let evidence   = if let Some(t) = d["trit"].as_i64() {
            t as f32
        } else {
            d["evidence"].as_f64().unwrap_or(0.0) as f32
        };
        let weight     = d["weight"].as_f64().unwrap_or(1.0) as f32;
        let hard_block = d["hard_block"].as_bool().unwrap_or(false);
        let mut dim = GateDimension::new(name, evidence, weight);
        if hard_block { dim = dim.hard(); }
        dim
    }).collect::<Vec<_>>();

    if dims.is_empty() {
        return Err("dimensions cannot be empty".into());
    }

    let result = action_gate(&dims);

    let verdict_label = match result.verdict {
        GateVerdict::Proceed => "proceed",
        GateVerdict::Block   => "blocked",
        GateVerdict::Hold    => "hold",
    };

    let dim_details: Vec<Value> = result.dim_results.iter().map(|(name, scalar, is_hard)| json!({
        "label":      name,
        "evidence":   (scalar.raw() * 1000.0).round() / 1000.0,
        "trit":       scalar.trit_i8(),
        "hard_block": is_hard,
        "status": if *is_hard && scalar.trit_i8() < 0 { "VETO" }
                  else if scalar.trit_i8() > 0 { "pass" }
                  else if scalar.trit_i8() < 0 { "block" }
                  else { "hold" },
    })).collect();

    Ok(json!({
        "verdict":       verdict_label,
        "hard_blocked_by": result.hard_blocked_by,
        "aggregate_scalar": (result.aggregate.raw() * 1000.0).round() / 1000.0,
        "aggregate_confidence": (result.aggregate.confidence() * 1000.0).round() / 1000.0,
        "dimensions":    dim_details,
        "explanation":   result.explanation,
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
        "moe_orchestrate"   => tool_moe_orchestrate(params),
        "moe_deliberate"    => tool_moe_deliberate(params),
        "trit_action_gate"  => tool_trit_action_gate(params),
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
        },
        {
            "name": "moe_orchestrate",
            "description": "Full MoE-13 ternary orchestration pass.\n\nRoutes your query through a pool of 13 domain experts (Syntax, WorldKnowledge, DeductiveReason, InductiveReason, ToolUse, Persona, Safety, FactCheck, CausalReason, AmbiguityRes, MathReason, ContextMem, MetaSafety) using dual-key synergistic routing.\n\nThe routing algorithm selects the expert PAIR that maximises both relevance to the query AND complementarity to each other — orthogonal competences produce a stronger emergent field than two similar experts.\n\nThe emergent triad field (1+1=3): Ek = synergy × (vi + vj)/2. Two experts that are good in different dimensions produce a third signal neither could produce alone.\n\nSafety hard gate fires FIRST, before any vote. A negative safety field = immediate reject, logged to audit trail.\n\nHold state: if the vote is split or confidence is low, the orchestrator calls a tiebreaker (up to 4 active experts max) before giving up — modelling the human 'I'll think about it' behaviour.\n\nReturns: trit decision, confidence, held flag, safety_vetoed flag, temperature hint for downstream LLM, prompt_hint, triad field details, and per-expert verdicts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The question or decision to orchestrate."
                    },
                    "evidence": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "6-element evidence vector: [syntax, world_knowledge, reasoning, tool_use, persona, safety]. Values on [-1.0, +1.0]. Omit or leave empty for neutral (all zeros)."
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "moe_deliberate",
            "description": "EMA-based ternary deliberation engine.\n\nModels iterative reasoning: given an initial scalar signal and a series of incoming evidence updates, the engine converges toward a stable ternary decision using exponential moving average. Alpha controls how fast new evidence is weighted vs prior belief.\n\nThis is the 'Wall-E collecting things' mechanism: the agent doesn't have to decide immediately. It accumulates evidence across rounds and commits only when confidence crosses the target threshold.\n\nRound-by-round trace is returned so you can inspect exactly when and why the agent committed or stayed in the tend (hold) zone.\n\nUse cases:\n  - Simulate multi-turn deliberation before acting\n  - Model an agent that keeps gathering context before deciding\n  - Tune alpha and target_confidence to calibrate risk tolerance",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "initial_scalar": {
                        "type": "number",
                        "description": "Starting evidence scalar on [-1.0, +1.0]. Positive = leaning affirm, negative = leaning reject, near-zero = maximum uncertainty."
                    },
                    "target_confidence": {
                        "type": "number",
                        "description": "Confidence threshold to stop deliberating (0.0–1.0). Default 0.8. Agent stays in tend until this is met."
                    },
                    "alpha": {
                        "type": "number",
                        "description": "EMA smoothing factor (0.0–1.0). High = fast adaptation to new evidence. Low = strong prior. Default 0.4."
                    },
                    "max_rounds": {
                        "type": "integer",
                        "description": "Maximum deliberation rounds before giving up and returning tend. Default 10."
                    },
                    "evidence_updates": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Sequence of incoming evidence scalars, one per round. Omit to replay initial_scalar each round (tests convergence from a single signal)."
                    }
                },
                "required": ["initial_scalar"]
            }
        },
        {
            "name": "trit_action_gate",
            "description": "Multi-dimensional ternary action gate with hard-block safety veto.\n\nBefore any AI agent takes an action, pass the relevant decision dimensions through this gate. Any dimension marked hard_block:true with a negative trit (reject) VETOES the action unconditionally, regardless of other dimensions. This implements the 'safety as absolute veto' principle from the MoE-13 architecture.\n\nNon-hard-block dimensions contribute to a weighted vote: positive trits add to pass_weight, negative trits add to block_weight. The gate returns Pass, Blocked, or Hold.\n\nTypical pattern:\n  - Safety check → hard_block: true\n  - Relevance → hard_block: false, weight: 1.0\n  - User consent → hard_block: true\n  - Confidence → hard_block: false, weight: 0.5\n\nThe gate enforces a structural separation: some signals are hard constraints, others are soft preferences. Binary logic collapses these — ternary keeps them distinct.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dimensions": {
                        "type": "array",
                        "description": "Array of gate dimensions. Each: {label, trit (-1/0/1), weight (default 1.0), hard_block (default false)}",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label":      { "type": "string",  "description": "Name of this decision dimension" },
                                "trit":       { "type": "integer", "enum": [-1, 0, 1], "description": "-1=block, 0=hold, 1=pass" },
                                "weight":     { "type": "number",  "description": "Importance weight (default 1.0). Ignored for hard_block dims." },
                                "hard_block": { "type": "boolean", "description": "If true, a negative trit on this dim VETOES regardless of all other dims." }
                            },
                            "required": ["label", "trit"]
                        }
                    }
                },
                "required": ["dimensions"]
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
