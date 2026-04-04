/// ternlang-api — REST HTTP server for the Ternary Intelligence Stack
///
/// Powers ternlang.com/api
///
/// Public routes (no auth):
///   GET  /                        — API info + available endpoints
///   GET  /health                  — health check
///   POST /mcp                     — HTTP MCP transport (Smithery / Claude Desktop)
///
/// API routes (X-Ternlang-Key header required):
///   POST /api/moe/orchestrate     — MoE-13 full pass (synchronous JSON)
///   POST /api/trit_decide         — scalar ternary decision
///   POST /api/trit_vector         — multi-dimensional evidence aggregation
///   POST /api/trit_consensus      — consensus(a, b)
///   POST /api/quantize_weights    — BitNet f32 → ternary
///   POST /api/sparse_benchmark    — sparse vs dense matmul stats
///
/// Admin routes (X-Admin-Key header required):
///   POST   /admin/keys            — generate a new API key
///   GET    /admin/keys            — list all keys with usage
///   DELETE /admin/keys/{key}      — revoke a key
///
/// Env vars:
///   TERNLANG_ADMIN_KEY   — admin secret (required in production)
///   KEYS_FILE            — path to JSON key store (default: ./ternlang_keys.json)
///   PORT                 — listening port (default: 3731)
///
/// Run:
///   TERNLANG_ADMIN_KEY=secret cargo run --release --bin ternlang-api

use axum::{
    Router,
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{sse::{Event, Sse}, IntoResponse, Response},
    routing::{delete, get, post},
};
use tokio_stream::StreamExt as TokioStreamExt;
use std::convert::Infallible;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use ternlang_core::{trit::Trit, parser::Parser, codegen::betbc::BytecodeEmitter, vm::BetVm};
use ternlang_moe::TernMoeOrchestrator;
use ternlang_ml::{
    TritScalar, TritEvidenceVec, TEND_BOUNDARY,
    bitnet_threshold, benchmark, dense_matmul, sparse_matmul, TritMatrix,
    // Phase 8: Ternary AI Reasoning Toolkit
    DeliberationEngine, CoalitionMember, coalition_vote,
    GateDimension, action_gate, GateVerdict,
    scalar_temperature, hallucination_score,
};

// ─── Key store ───────────────────────────────────────────────────────────────

/// One API key entry. Raw key string is used as the HashMap key so lookup is O(1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    pub key_id:        String,   // "tk_<uuid_short>"
    pub tier:          u8,       // 1=open, 2=restricted, 3=enterprise
    pub email:         String,
    pub note:          String,   // free-form admin note
    pub created_at:    String,   // ISO 8601
    pub is_active:     bool,
    pub request_count: u64,
}

/// Persistent key store — serialised as JSON to `path`.
#[derive(Debug, Serialize, Deserialize, Default)]
struct KeyStoreData {
    /// Map from raw key string → entry metadata
    keys: HashMap<String, ApiKeyEntry>,
}

pub struct KeyStore {
    data: RwLock<KeyStoreData>,
    path: PathBuf,
}

impl KeyStore {
    /// Load from disk (creates empty file if it doesn't exist).
    pub async fn load(path: PathBuf) -> Arc<Self> {
        let data = if path.exists() {
            let raw = tokio::fs::read_to_string(&path).await.unwrap_or_default();
            serde_json::from_str::<KeyStoreData>(&raw).unwrap_or_default()
        } else {
            KeyStoreData::default()
        };
        Arc::new(KeyStore { data: RwLock::new(data), path })
    }

    /// Persist current state to disk (best-effort; logs on error).
    async fn save(&self) {
        let data = self.data.read().await;
        match serde_json::to_string_pretty(&*data) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&self.path, json).await {
                    eprintln!("[key-store] save error: {}", e);
                }
            }
            Err(e) => eprintln!("[key-store] serialise error: {}", e),
        }
    }

    /// Check a raw key and, if valid, increment its counter.
    pub async fn validate_and_bump(&self, raw_key: &str) -> Option<ApiKeyEntry> {
        let mut data = self.data.write().await;
        let entry = data.keys.get_mut(raw_key)?;
        if !entry.is_active {
            return None;
        }
        entry.request_count += 1;
        Some(entry.clone())
    }

    /// Generate a new key. Returns (raw_key, entry).
    pub async fn generate(&self, tier: u8, email: String, note: String) -> (String, ApiKeyEntry) {
        let uid   = Uuid::new_v4().to_string().replace('-', "");
        let raw   = format!("tern_{}_{}", tier, &uid[..24]);
        let key_id = format!("tk_{}", &uid[..8]);

        let entry = ApiKeyEntry {
            key_id:        key_id.clone(),
            tier,
            email,
            note,
            created_at:    Utc::now().to_rfc3339(),
            is_active:     true,
            request_count: 0,
        };

        self.data.write().await.keys.insert(raw.clone(), entry.clone());
        self.save().await;
        (raw, entry)
    }

    /// Revoke a key by raw value. Returns true if the key existed.
    pub async fn revoke(&self, raw_key: &str) -> bool {
        let mut data = self.data.write().await;
        if let Some(entry) = data.keys.get_mut(raw_key) {
            entry.is_active = false;
            drop(data);
            self.save().await;
            return true;
        }
        false
    }

    /// List all entries (key hidden, only metadata).
    pub async fn list(&self) -> Vec<Value> {
        let data = self.data.read().await;
        data.keys.iter().map(|(raw, e)| json!({
            "key_id":        e.key_id,
            "key_preview":   format!("{}…", &raw[..12]),
            "tier":          e.tier,
            "email":         e.email,
            "note":          e.note,
            "created_at":    e.created_at,
            "is_active":     e.is_active,
            "request_count": e.request_count,
        })).collect()
    }
}

// ─── App state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    admin_key: String,
    keys:      Arc<KeyStore>,
    version:   &'static str,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn trit_to_i8(t: Trit) -> i8 {
    match t { Trit::NegOne => -1, Trit::Zero => 0, Trit::PosOne => 1 }
}

fn i8_to_trit(v: i64) -> Option<Trit> {
    match v { -1 => Some(Trit::NegOne), 0 => Some(Trit::Zero), 1 => Some(Trit::PosOne), _ => None }
}

fn api_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message, "docs": "https://ternlang.com/docs/api" }))).into_response()
}

// ─── Auth middleware (API routes) ─────────────────────────────────────────────

async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Public endpoints — no key required
    if path == "/" || path == "/health" || path == "/mcp" || path.starts_with("/admin") {
        return next.run(request).await;
    }

    let raw = headers
        .get("X-Ternlang-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if raw.is_empty() {
        return api_error(StatusCode::UNAUTHORIZED,
            "Missing X-Ternlang-Key header. Acquire a key at https://ternlang.com/#licensing");
    }

    match state.keys.validate_and_bump(raw).await {
        Some(_entry) => next.run(request).await,
        None => api_error(StatusCode::UNAUTHORIZED, "Invalid or revoked API key."),
    }
}

// ─── Admin middleware ──────────────────────────────────────────────────────────

async fn require_admin_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if !request.uri().path().starts_with("/admin") {
        return next.run(request).await;
    }

    let provided = headers
        .get("X-Admin-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided.is_empty() {
        return api_error(StatusCode::UNAUTHORIZED, "Missing X-Admin-Key header.");
    }
    if provided != state.admin_key {
        return api_error(StatusCode::UNAUTHORIZED, "Invalid admin key.");
    }

    next.run(request).await
}

// ─── GET / ───────────────────────────────────────────────────────────────────

async fn root(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "name":    "Ternlang API",
        "version": state.version,
        "by":      "RFI-IRFOS",
        "website": "https://ternlang.com",
        "docs":    "https://ternlang.com/docs/api",
        "auth":    "X-Ternlang-Key header required for /api/* endpoints",
        "endpoints": {
            "POST /api/trit_decide":             "Scalar ternary decision: evidence[] → reject/tend/affirm + confidence",
            "POST /api/trit_vector":             "Multi-dimensional evidence: named dimensions + weights → aggregate",
            "POST /api/trit_consensus":          "consensus(a, b) → ternary result",
            "POST /api/quantize_weights":        "f32[] → ternary weights via BitNet threshold",
            "POST /api/sparse_benchmark":        "Sparse vs dense matmul performance stats",
            "POST /api/trit_deliberate":         "EMA deliberation engine: multi-round evidence → converged trit",
            "POST /api/trit_coalition":          "Coalition vote: N agents → quorum/dissent/abstain + consensus",
            "POST /api/trit_gate":               "Action gate: multi-dim hard-block safety veto",
            "POST /api/scalar_temperature":      "TritScalar → LLM sampling temperature + prompt hint",
            "POST /api/hallucination_score":     "Signal variance → trust trit",
            "POST /api/moe/orchestrate":          "MoE-13 full orchestration — synchronous JSON result",
            "GET  /api/stream/moe_orchestrate":  "SSE: MoE-13 orchestration pass streamed event-by-event",
            "GET  /api/stream/deliberate":       "SSE: EMA deliberation — one event per round, live feed",
        },
        "mcp": {
            "url":         "https://ternlang.com/mcp",
            "transport":   "HTTP JSON-RPC 2.0",
            "smithery":    "https://smithery.ai/server/ternlang",
            "description": "POST /mcp — all 10 tools available, no API key required",
        },
        "acquire_key": "https://ternlang.com/#licensing"
    }))
}

// ─── GET /health ─────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "engine": "BET VM", "trit": 1 }))
}

// ─── Admin: POST /admin/keys ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateKeyBody {
    tier:  Option<u8>,
    email: Option<String>,
    note:  Option<String>,
}

async fn admin_generate_key(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GenerateKeyBody>,
) -> Response {
    let tier  = body.tier.unwrap_or(2);
    let email = body.email.unwrap_or_default();
    let note  = body.note.unwrap_or_default();

    if tier < 1 || tier > 3 {
        return api_error(StatusCode::BAD_REQUEST, "tier must be 1, 2, or 3");
    }

    let (raw, entry) = state.keys.generate(tier, email, note).await;

    eprintln!("[admin] generated key {} for {}", entry.key_id, entry.email);

    (StatusCode::CREATED, Json(json!({
        "key":     raw,        // Only returned once — save it!
        "key_id":  entry.key_id,
        "tier":    entry.tier,
        "email":   entry.email,
        "created": entry.created_at,
        "warning": "Store this key securely — it will not be shown again.",
    }))).into_response()
}

// ─── Admin: GET /admin/keys ───────────────────────────────────────────────────

async fn admin_list_keys(State(state): State<Arc<AppState>>) -> Json<Value> {
    let entries = state.keys.list().await;
    Json(json!({ "total": entries.len(), "keys": entries }))
}

// ─── Admin: DELETE /admin/keys/{key} ─────────────────────────────────────────

async fn admin_revoke_key(
    State(state): State<Arc<AppState>>,
    Path(raw_key): Path<String>,
) -> Response {
    if state.keys.revoke(&raw_key).await {
        eprintln!("[admin] revoked key {}", &raw_key[..12.min(raw_key.len())]);
        (StatusCode::OK, Json(json!({ "revoked": true }))).into_response()
    } else {
        api_error(StatusCode::NOT_FOUND, "Key not found.")
    }
}

// ─── POST /api/trit_decide ───────────────────────────────────────────────────

async fn trit_decide(Json(body): Json<Value>) -> Response {
    let evidence: Vec<f32> = match body["evidence"].as_array() {
        Some(arr) => match arr.iter()
            .map(|v| v.as_f64().map(|f| f as f32).ok_or(()))
            .collect::<Result<Vec<_>, _>>() {
                Ok(v) => v,
                Err(_) => return api_error(StatusCode::BAD_REQUEST, "evidence values must be numbers"),
            },
        None => return api_error(StatusCode::BAD_REQUEST, "evidence must be an array of numbers"),
    };

    if evidence.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "evidence cannot be empty");
    }

    let min_confidence = body["min_confidence"].as_f64().unwrap_or(0.0) as f32;
    let mean = evidence.iter().sum::<f32>() / evidence.len() as f32;
    let scalar = TritScalar::new(mean);

    let per_signal: Vec<Value> = evidence.iter().enumerate().map(|(i, &v)| {
        let s = TritScalar::new(v);
        json!({
            "index":      i,
            "raw":        (v * 1000.0).round() / 1000.0,
            "label":      s.label(),
            "confidence": (s.confidence() * 1000.0).round() / 1000.0,
            "trit":       trit_to_i8(s.trit()),
        })
    }).collect();

    let zeros = per_signal.iter().filter(|s| s["trit"] == 0).count();
    let actionable = scalar.is_actionable(min_confidence);

    let recommendation = match scalar.trit() {
        Trit::PosOne => format!(
            "Affirm — confidence {:.0}%{}.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence — gather more evidence)" }
        ),
        Trit::NegOne => format!(
            "Reject — confidence {:.0}%{}.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence — gather more evidence)" }
        ),
        Trit::Zero => format!(
            "Tend — scalar {:.3} is in the deliberation zone [{:.3}, +{:.3}]. Gather more evidence.",
            scalar.raw(), -TEND_BOUNDARY, TEND_BOUNDARY
        ),
    };

    (StatusCode::OK, Json(json!({
        "scalar":          (scalar.raw() * 1000.0).round() / 1000.0,
        "trit":            trit_to_i8(scalar.trit()),
        "label":           scalar.label(),
        "confidence":      (scalar.confidence() * 1000.0).round() / 1000.0,
        "is_actionable":   actionable,
        "tend_boundary":   TEND_BOUNDARY,
        "signal_sparsity": zeros as f64 / evidence.len() as f64,
        "recommendation":  recommendation,
        "per_signal":      per_signal,
    }))).into_response()
}

// ─── POST /api/trit_vector ───────────────────────────────────────────────────

async fn trit_vector(Json(body): Json<Value>) -> Response {
    let dims = match body["dimensions"].as_array() {
        Some(d) => d,
        None => return api_error(StatusCode::BAD_REQUEST,
            "dimensions must be an array of {label, value, weight} objects"),
    };

    if dims.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "dimensions cannot be empty");
    }

    let min_confidence = body["min_confidence"].as_f64().unwrap_or(0.5) as f32;

    let mut labels  = Vec::new();
    let mut values  = Vec::new();
    let mut weights = Vec::new();

    for (i, d) in dims.iter().enumerate() {
        let label  = d["label"].as_str().unwrap_or("unnamed").to_string();
        let value  = match d["value"].as_f64() {
            Some(v) => v as f32,
            None => return api_error(StatusCode::BAD_REQUEST,
                &format!("dimensions[{}].value must be a number", i)),
        };
        let weight = d["weight"].as_f64().unwrap_or(1.0) as f32;
        if weight < 0.0 {
            return api_error(StatusCode::BAD_REQUEST,
                &format!("dimensions[{}].weight must be >= 0", i));
        }
        labels.push(label);
        values.push(value);
        weights.push(weight);
    }

    let ev      = TritEvidenceVec::new(labels, values, weights);
    let agg     = ev.aggregate();
    let scalars = ev.scalars();
    let actionable = agg.is_actionable(min_confidence);

    let breakdown: Vec<Value> = ev.dimensions.iter()
        .zip(ev.values.iter())
        .zip(ev.weights.iter())
        .zip(scalars.iter())
        .map(|(((label, &val), &w), sc)| json!({
            "label":      label,
            "raw":        (val * 1000.0).round() / 1000.0,
            "weight":     w,
            "trit":       trit_to_i8(sc.trit()),
            "label_trit": sc.label(),
            "confidence": (sc.confidence() * 1000.0).round() / 1000.0,
        })).collect();

    let zeros = breakdown.iter().filter(|d| d["trit"] == 0).count();

    (StatusCode::OK, Json(json!({
        "aggregate": {
            "scalar":     (agg.raw() * 1000.0).round() / 1000.0,
            "trit":       trit_to_i8(agg.trit()),
            "label":      agg.label(),
            "confidence": (agg.confidence() * 1000.0).round() / 1000.0,
            "is_actionable": actionable,
        },
        "dimensions":       breakdown,
        "tend_boundary":    TEND_BOUNDARY,
        "signal_sparsity":  zeros as f64 / ev.dimensions.len() as f64,
        "recommendation":   match agg.trit() {
            Trit::PosOne => "Affirm — weighted evidence crosses threshold.".to_string(),
            Trit::NegOne => "Reject — weighted evidence crosses negative threshold.".to_string(),
            Trit::Zero   => format!(
                "Tend — aggregate {:.3} within deliberation zone. Resolve conflicting dimensions.",
                agg.raw()
            ),
        },
    }))).into_response()
}

// ─── POST /api/trit_consensus ────────────────────────────────────────────────

async fn trit_consensus(Json(body): Json<Value>) -> Response {
    let a = match body["a"].as_i64().and_then(i8_to_trit) {
        Some(t) => t,
        None => return api_error(StatusCode::BAD_REQUEST, "a must be -1, 0, or 1"),
    };
    let b = match body["b"].as_i64().and_then(i8_to_trit) {
        Some(t) => t,
        None => return api_error(StatusCode::BAD_REQUEST, "b must be -1, 0, or 1"),
    };

    // consensus: agree → common value (carry=0); disagree → 0 (carry=1)
    let result = if a == b { a } else { Trit::Zero };
    let carry  = if a == b { Trit::Zero } else { Trit::PosOne };

    (StatusCode::OK, Json(json!({
        "a":      trit_to_i8(a),
        "b":      trit_to_i8(b),
        "result": trit_to_i8(result),
        "carry":  trit_to_i8(carry),
        "label":  TritScalar::new(trit_to_i8(result) as f32).label(),
    }))).into_response()
}

// ─── POST /api/quantize_weights ──────────────────────────────────────────────

async fn quantize_weights(Json(body): Json<Value>) -> Response {
    let weights: Vec<f32> = match body["weights"].as_array() {
        Some(arr) => arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect(),
        None => return api_error(StatusCode::BAD_REQUEST, "weights must be an array of numbers"),
    };

    if weights.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "weights cannot be empty");
    }

    let threshold = body["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;

    let trits: Vec<i8> = weights.iter().map(|&w| {
        if w > threshold { 1 }
        else if w < -threshold { -1 }
        else { 0 }
    }).collect();

    let zeros    = trits.iter().filter(|&&t| t == 0).count();
    let sparsity = zeros as f64 / trits.len() as f64;

    (StatusCode::OK, Json(json!({
        "threshold":       (threshold * 1000.0).round() / 1000.0,
        "trits":           trits,
        "sparsity":        (sparsity * 1000.0).round() / 1000.0,
        "non_zero":        trits.len() - zeros,
        "bits_saved":      format!("{:.1}%", sparsity * 100.0),
        "zone":            if sparsity < 0.40 { "warm" }
                           else if sparsity <= 0.60 { "goldilocks ★" }
                           else { "asymptotic" },
    }))).into_response()
}

// ─── POST /api/sparse_benchmark ──────────────────────────────────────────────

async fn sparse_benchmark(Json(body): Json<Value>) -> Response {
    let rows = body["rows"].as_u64().unwrap_or(4) as usize;
    let cols = body["cols"].as_u64().unwrap_or(4) as usize;

    if rows == 0 || cols == 0 || rows > 512 || cols > 512 {
        return api_error(StatusCode::BAD_REQUEST, "rows and cols must be between 1 and 512");
    }

    let f32_weights: Vec<f32> = match body["weights"].as_array() {
        Some(arr) => arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect(),
        None => (0..rows * cols).map(|i| match i % 5 {
            0 => 0.9, 1 => -0.8, 2 => 0.1, 3 => -0.1, _ => 0.05
        }).collect(),
    };

    if f32_weights.len() != rows * cols {
        return api_error(StatusCode::BAD_REQUEST,
            &format!("weights length must equal rows×cols = {}", rows * cols));
    }

    let threshold = body["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&f32_weights) as f64) as f32;

    let w     = TritMatrix::from_f32(rows, cols, &f32_weights, threshold);
    let input = TritMatrix::new(rows, cols);
    let r     = benchmark(&input, &w);
    let (_, skipped) = sparse_matmul(&input, &w);

    (StatusCode::OK, Json(json!({
        "rows":                rows,
        "cols":                cols,
        "weight_sparsity":     r.weight_sparsity,
        "skip_rate":           r.skip_rate,
        "dense_ops":           r.dense_ops,
        "sparse_ops":          r.sparse_ops,
        "skipped_ops":         skipped,
        "ops_reduction_factor": r.dense_ops as f64 / r.sparse_ops.max(1) as f64,
        "threshold_used":      threshold,
        "summary": format!(
            "{:.1}% weight sparsity → {:.2}× fewer multiply ops ({} skipped of {})",
            r.weight_sparsity * 100.0,
            r.dense_ops as f64 / r.sparse_ops.max(1) as f64,
            skipped,
            r.dense_ops
        ),
    }))).into_response()
}

// ─── POST /api/trit_deliberate ────────────────────────────────────────────────

async fn trit_deliberate(Json(body): Json<Value>) -> Response {
    let target_confidence = body["target_confidence"].as_f64().unwrap_or(0.7) as f32;
    let max_rounds = body["max_rounds"].as_u64().unwrap_or(10) as usize;
    let alpha = body["alpha"].as_f64().unwrap_or(0.4) as f32;

    let rounds_evidence: Vec<Vec<f32>> = match body["rounds"].as_array() {
        Some(arr) => arr.iter().map(|round| {
            round.as_array().map(|signals|
                signals.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
            ).unwrap_or_default()
        }).collect(),
        None => return api_error(StatusCode::BAD_REQUEST,
            "rounds must be an array of evidence arrays, e.g. [[0.2], [0.8, 0.9]]"),
    };

    if rounds_evidence.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "rounds cannot be empty");
    }

    let engine = DeliberationEngine::new(target_confidence, max_rounds).with_alpha(alpha);
    let result = engine.run(rounds_evidence);

    let trace_json: Vec<Value> = result.trace.iter().map(|r| json!({
        "round":           r.round,
        "cumulative_mean": (r.cumulative_mean * 1000.0).round() / 1000.0,
        "trit":            r.scalar.trit_i8(),
        "label":           r.scalar.label(),
        "confidence":      (r.scalar.confidence() * 1000.0).round() / 1000.0,
        "converged":       r.converged,
    })).collect();

    (StatusCode::OK, Json(json!({
        "final_trit":       result.final_trit,
        "final_label":      result.final_label,
        "final_confidence": (result.final_confidence * 1000.0).round() / 1000.0,
        "converged":        result.converged,
        "rounds_used":      result.rounds_used,
        "convergence_reason": result.convergence_reason,
        "trace":            trace_json,
    }))).into_response()
}

// ─── POST /api/trit_coalition ─────────────────────────────────────────────────

async fn trit_coalition(Json(body): Json<Value>) -> Response {
    let members_raw = match body["members"].as_array() {
        Some(arr) => arr,
        None => return api_error(StatusCode::BAD_REQUEST,
            "members must be an array of {label, trit, confidence, weight} objects"),
    };

    if members_raw.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "members cannot be empty");
    }

    let mut members = Vec::new();
    for (i, m) in members_raw.iter().enumerate() {
        let label = m["label"].as_str().unwrap_or("agent").to_string();
        let trit = match m["trit"].as_i64() {
            Some(v) if v >= -1 && v <= 1 => v as i8,
            _ => return api_error(StatusCode::BAD_REQUEST,
                &format!("members[{}].trit must be -1, 0, or 1", i)),
        };
        let confidence = m["confidence"].as_f64().unwrap_or(1.0) as f32;
        let weight = m["weight"].as_f64().unwrap_or(1.0) as f32;
        members.push(CoalitionMember::new(label, trit, confidence, weight));
    }

    let result = coalition_vote(&members);

    let breakdown: Vec<Value> = result.breakdown.iter().map(|(label, trit, contribution)| json!({
        "label":        label,
        "trit":         trit,
        "contribution": (contribution * 1000.0).round() / 1000.0,
    })).collect();

    (StatusCode::OK, Json(json!({
        "trit":             result.trit,
        "label":            result.label,
        "aggregate_score":  (result.aggregate_score * 1000.0).round() / 1000.0,
        "quorum":           (result.quorum * 1000.0).round() / 1000.0,
        "dissent_rate":     (result.dissent_rate * 1000.0).round() / 1000.0,
        "abstain_rate":     (result.abstain_rate * 1000.0).round() / 1000.0,
        "member_count":     result.member_count,
        "breakdown":        breakdown,
    }))).into_response()
}

// ─── POST /api/trit_gate ──────────────────────────────────────────────────────

async fn trit_gate(Json(body): Json<Value>) -> Response {
    let dims_raw = match body["dimensions"].as_array() {
        Some(arr) => arr,
        None => return api_error(StatusCode::BAD_REQUEST,
            "dimensions must be an array of {name, evidence, weight, hard_block?} objects"),
    };

    if dims_raw.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "dimensions cannot be empty");
    }

    let mut dimensions = Vec::new();
    for (i, d) in dims_raw.iter().enumerate() {
        let name = d["name"].as_str().unwrap_or("dim").to_string();
        let evidence = match d["evidence"].as_f64() {
            Some(v) => v as f32,
            None => return api_error(StatusCode::BAD_REQUEST,
                &format!("dimensions[{}].evidence must be a number in [-1, 1]", i)),
        };
        let weight = d["weight"].as_f64().unwrap_or(1.0) as f32;
        let hard_block = d["hard_block"].as_bool().unwrap_or(false);
        let mut dim = GateDimension::new(name, evidence, weight);
        if hard_block { dim = dim.hard(); }
        dimensions.push(dim);
    }

    let result = action_gate(&dimensions);

    let dim_results: Vec<Value> = result.dim_results.iter().map(|(name, sc, is_hard)| json!({
        "name":       name,
        "trit":       sc.trit_i8(),
        "label":      sc.label(),
        "confidence": (sc.confidence() * 1000.0).round() / 1000.0,
        "hard_block": is_hard,
    })).collect();

    (StatusCode::OK, Json(json!({
        "verdict":          result.verdict.label(),
        "aggregate_scalar": (result.aggregate.raw() * 1000.0).round() / 1000.0,
        "hard_blocked_by":  result.hard_blocked_by,
        "dimensions":       dim_results,
        "explanation":      result.explanation,
    }))).into_response()
}

// ─── POST /api/scalar_temperature ────────────────────────────────────────────

async fn scalar_temperature_endpoint(Json(body): Json<Value>) -> Response {
    let evidence: Vec<f32> = match body["evidence"].as_array() {
        Some(arr) => arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect(),
        None => match body["scalar"].as_f64() {
            Some(v) => vec![v as f32],
            None => return api_error(StatusCode::BAD_REQUEST,
                "provide evidence array or scalar value"),
        },
    };

    let mean = evidence.iter().sum::<f32>() / evidence.len().max(1) as f32;
    let sc = TritScalar::new(mean);
    let temp = scalar_temperature(&sc);

    (StatusCode::OK, Json(json!({
        "trit":        temp.trit,
        "confidence":  (temp.confidence * 1000.0).round() / 1000.0,
        "temperature": temp.temperature,
        "reasoning":   temp.reasoning,
        "prompt_hint": temp.prompt_hint,
        "usage": "Set your LLM sampling temperature to the 'temperature' field for ternary-aligned generation.",
    }))).into_response()
}

// ─── POST /api/hallucination_score ────────────────────────────────────────────

async fn hallucination_score_endpoint(Json(body): Json<Value>) -> Response {
    let signals: Vec<f32> = match body["signals"].as_array() {
        Some(arr) => arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect(),
        None => return api_error(StatusCode::BAD_REQUEST,
            "signals must be an array of numbers in [-1, 1]"),
    };

    if signals.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "signals cannot be empty");
    }

    let score = hallucination_score(&signals);

    (StatusCode::OK, Json(json!({
        "trust_trit":   score.trust_trit,
        "trust_label":  score.trust_label,
        "mean":         (score.mean * 1000.0).round() / 1000.0,
        "variance":     (score.variance * 1000.0).round() / 1000.0,
        "consistency":  (score.consistency * 1000.0).round() / 1000.0,
        "signal_count": score.signal_count,
        "explanation":  score.explanation,
    }))).into_response()
}

// ─── POST /api/moe/orchestrate ───────────────────────────────────────────────
//
// Synchronous (non-streaming) MoE-13 orchestration.
// Returns the full OrchestrationResult as a single JSON object.
//
// Body: { "query": "...", "evidence": [0.8, 0.6, 0.9, 0.7, 0.5, 1.0] }
//   evidence: optional 6-float vector [syntax, world_knowledge, reasoning,
//             tool_use, persona, safety]. Defaults to [0.5; 6] if omitted.

#[derive(Deserialize)]
struct MoeOrchestrateBody {
    query:    String,
    evidence: Option<Vec<f32>>,
}

async fn moe_orchestrate(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<MoeOrchestrateBody>,
) -> Response {
    let evidence = body.evidence.unwrap_or_else(|| vec![0.5f32; 6]);

    // Clamp / pad to exactly 6 dimensions
    let mut ev6 = [0.5f32; 6];
    for (i, v) in evidence.iter().take(6).enumerate() {
        ev6[i] = v.clamp(-1.0, 1.0);
    }

    let mut orch  = TernMoeOrchestrator::with_standard_experts();
    let result    = orch.orchestrate(&body.query, &ev6);

    let trit_label = match result.trit {
         1  => "affirm",
        -1  => "reject",
        _   => "hold",
    };

    let pair_json = result.pair.as_ref().map(|p| json!({
        "expert_a":  p.expert_a,
        "expert_b":  p.expert_b,
        "relevance": p.relevance,
        "synergy":   p.synergy,
        "score":     p.combined,
    }));

    let verdicts_json: Vec<_> = result.verdicts.iter().map(|v| json!({
        "expert_id":   v.expert_id,
        "expert_name": v.expert_name,
        "trit":        v.trit,
        "confidence":  v.confidence,
        "reasoning":   v.reasoning,
    })).collect();

    Json(json!({
        "trit":          result.trit,
        "label":         trit_label,
        "confidence":    result.confidence,
        "held":          result.held,
        "safety_vetoed": result.safety_vetoed,
        "temperature":   result.temperature,
        "prompt_hint":   result.prompt_hint,
        "pair":          pair_json,
        "verdicts":      verdicts_json,
        "triad_field": {
            "synergy_weight": result.triad_field.synergy_weight,
            "is_amplifying":  result.triad_field.is_amplifying(),
        },
    })).into_response()
}

// ─── GET /api/stream/moe_orchestrate ─────────────────────────────────────────
//
// SSE stream of a full MoE-13 orchestration pass, broken into discrete events:
//
//   event: routing       — which expert pair was selected and why
//   event: verdict       — each expert's individual verdict (one event per expert)
//   event: triad         — the emergent triad field
//   event: safety        — safety gate result (veto or pass)
//   event: vote          — weighted vote result
//   event: tiebreaker    — tiebreaker verdict (if invoked)
//   event: result        — final OrchestrationResult
//   event: done          — stream sentinel
//
// Query params: query (string), evidence (comma-separated floats, optional)
// Header: X-Ternlang-Key

#[derive(serde::Deserialize)]
struct MoeStreamParams {
    query:    Option<String>,
    evidence: Option<String>,   // "0.6,0.7,0.8,0.5,0.4,0.9"
}

async fn stream_moe_orchestrate(
    Query(params): Query<MoeStreamParams>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let query = params.query.unwrap_or_else(|| "default query".into());
    let evidence: Vec<f32> = params.evidence
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();

    // Build all events synchronously (orchestration is CPU-bound, not I/O-bound),
    // then stream them with inter-event delays so the client sees progressive delivery.
    let events = build_moe_sse_events(query, evidence);

    let stream = tokio_stream::iter(events)
        .then(|ev| async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
            Ok::<Event, Infallible>(ev)
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(tokio::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn build_moe_sse_events(query: String, evidence: Vec<f32>) -> Vec<Event> {
    let mut events = Vec::new();
    let evidence_ref: &[f32] = &evidence;

    // Run full orchestration
    let mut orch = TernMoeOrchestrator::with_standard_experts();
    let result   = orch.orchestrate(&query, evidence_ref);

    // ── event: routing ──────────────────────────────────────────────
    if let Some(ref pair) = result.pair {
        let routing_data = json!({
            "event":     "routing",
            "expert_a":  pair.expert_a,
            "expert_b":  pair.expert_b,
            "relevance": (pair.relevance * 1000.0).round() / 1000.0,
            "synergy":   (pair.synergy  * 1000.0).round() / 1000.0,
            "combined":  (pair.combined * 1000.0).round() / 1000.0,
            "note": format!(
                "Selected experts {} + {} — relevance {:.2}, synergy {:.2} (complementarity)",
                pair.expert_a, pair.expert_b, pair.relevance, pair.synergy
            ),
        });
        events.push(
            Event::default()
                .event("routing")
                .data(routing_data.to_string())
        );
    }

    // ── event: verdict (one per expert) ─────────────────────────────
    for verdict in &result.verdicts {
        let label = match verdict.trit { 1 => "affirm", -1 => "reject", _ => "tend" };
        let verdict_data = json!({
            "event":       "verdict",
            "expert_id":   verdict.expert_id,
            "expert_name": verdict.expert_name,
            "trit":        verdict.trit,
            "label":       label,
            "confidence":  (verdict.confidence * 1000.0).round() / 1000.0,
            "reasoning":   verdict.reasoning,
        });
        events.push(
            Event::default()
                .event("verdict")
                .data(verdict_data.to_string())
        );
    }

    // ── event: triad ─────────────────────────────────────────────────
    let triad_data = json!({
        "event":          "triad",
        "synergy_weight": (result.triad_field.synergy_weight * 1000.0).round() / 1000.0,
        "field":          result.triad_field.field.raw,
        "is_amplifying":  result.triad_field.is_amplifying(),
        "note": if result.triad_field.is_amplifying() {
            "Emergent field is amplifying — triad synthesis boosting signal."
        } else {
            "Emergent field computed — synergy below amplification threshold."
        },
    });
    events.push(Event::default().event("triad").data(triad_data.to_string()));

    // ── event: safety ────────────────────────────────────────────────
    let safety_data = json!({
        "event":   "safety",
        "vetoed":  result.safety_vetoed,
        "safety_field": result.triad_field.field.raw[5],
        "status":  if result.safety_vetoed { "VETO — hard block engaged" } else { "pass" },
    });
    events.push(Event::default().event("safety").data(safety_data.to_string()));

    if result.safety_vetoed {
        // Early-terminate: emit result + done
        let result_data = json!({
            "event":          "result",
            "trit":           result.trit,
            "label":          "reject",
            "confidence":     1.0,
            "held":           false,
            "safety_vetoed":  true,
            "temperature":    result.temperature,
            "prompt_hint":    result.prompt_hint,
        });
        events.push(Event::default().event("result").data(result_data.to_string()));
        events.push(Event::default().event("done").data("{}"));
        return events;
    }

    // ── event: vote ──────────────────────────────────────────────────
    let vote_label = match result.trit { 1 => "affirm", -1 => "reject", _ => "tend" };
    let vote_data = json!({
        "event":       "vote",
        "trit":        result.trit,
        "label":       vote_label,
        "confidence":  (result.confidence * 1000.0).round() / 1000.0,
        "held":        result.held,
        "note": if result.held {
            "Result held — tiebreaker invoked or confidence below threshold."
        } else {
            "Vote resolved."
        },
    });
    events.push(Event::default().event("vote").data(vote_data.to_string()));

    // ── event: tiebreaker (if held and >2 verdicts) ───────────────────
    if result.verdicts.len() > 2 {
        if let Some(tb) = result.verdicts.last() {
            let tb_label = match tb.trit { 1 => "affirm", -1 => "reject", _ => "tend" };
            let tb_data = json!({
                "event":       "tiebreaker",
                "expert_id":   tb.expert_id,
                "expert_name": tb.expert_name,
                "trit":        tb.trit,
                "label":       tb_label,
                "confidence":  (tb.confidence * 1000.0).round() / 1000.0,
                "reasoning":   tb.reasoning,
            });
            events.push(Event::default().event("tiebreaker").data(tb_data.to_string()));
        }
    }

    // ── event: result ─────────────────────────────────────────────────
    let result_data = json!({
        "event":         "result",
        "trit":          result.trit,
        "label":         vote_label,
        "confidence":    (result.confidence * 1000.0).round() / 1000.0,
        "held":          result.held,
        "safety_vetoed": false,
        "temperature":   (result.temperature * 1000.0).round() / 1000.0,
        "prompt_hint":   result.prompt_hint,
        "verdicts":      result.verdicts.len(),
    });
    events.push(Event::default().event("result").data(result_data.to_string()));

    // ── event: done ───────────────────────────────────────────────────
    events.push(Event::default().event("done").data("{}"));

    events
}

// ─── GET /api/stream/deliberate ──────────────────────────────────────────────
//
// SSE stream of an EMA deliberation session — emits one event per round:
//
//   event: round     — { round, scalar, confidence, trit, label, converged }
//   event: result    — final summary after all rounds
//   event: done      — stream sentinel
//
// Query params:
//   initial   — starting scalar [-1, 1] (default 0.0)
//   target    — target confidence [0, 1] (default 0.8)
//   alpha     — EMA smoothing [0, 1] (default 0.4)
//   rounds    — comma-separated evidence scalars, one per round

#[derive(serde::Deserialize)]
struct DeliberateStreamParams {
    initial: Option<f32>,
    target:  Option<f32>,
    alpha:   Option<f32>,
    rounds:  Option<String>,  // "0.2,0.5,0.7,0.8,0.9"
}

async fn stream_deliberate(
    Query(params): Query<DeliberateStreamParams>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let initial = params.initial.unwrap_or(0.0f32).clamp(-1.0, 1.0);
    let target  = params.target.unwrap_or(0.8f32).clamp(0.0, 1.0);
    let alpha   = params.alpha.unwrap_or(0.4f32).clamp(0.01, 1.0);

    let round_signals: Vec<f32> = params.rounds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();

    let evidence_rounds: Vec<Vec<f32>> = if round_signals.is_empty() {
        // Default: 10 rounds replaying the initial signal
        vec![vec![initial]; 10]
    } else {
        round_signals.iter().map(|&v| vec![v]).collect()
    };

    let engine = DeliberationEngine::new(target, evidence_rounds.len()).with_alpha(alpha);
    let result = engine.run(evidence_rounds);

    let mut events: Vec<Event> = result.trace.iter().map(|r| {
        let label = match r.scalar.trit_i8() { 1 => "affirm", -1 => "reject", _ => "tend" };
        let round_data = json!({
            "event":      "round",
            "round":      r.round,
            "scalar":     (r.scalar.raw() * 1000.0).round() / 1000.0,
            "confidence": (r.scalar.confidence() * 1000.0).round() / 1000.0,
            "trit":       r.scalar.trit_i8(),
            "label":      label,
            "converged":  r.converged,
        });
        Event::default().event("round").data(round_data.to_string())
    }).collect();

    let final_label = match result.final_trit { 1 => "affirm", -1 => "reject", _ => "tend" };
    let result_data = json!({
        "event":              "result",
        "final_trit":         result.final_trit,
        "final_label":        final_label,
        "final_confidence":   (result.final_confidence * 1000.0).round() / 1000.0,
        "converged":          result.converged,
        "rounds_used":        result.rounds_used,
        "convergence_reason": result.convergence_reason,
        "target_confidence":  target,
    });
    events.push(Event::default().event("result").data(result_data.to_string()));
    events.push(Event::default().event("done").data("{}"));

    let stream = tokio_stream::iter(events)
        .then(|ev| async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
            Ok::<Event, Infallible>(ev)
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(tokio::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ─── POST /mcp — HTTP MCP transport (Smithery / Claude Desktop HTTP mode) ─────
//
// Smithery requires a live HTTP URL.  This endpoint implements JSON-RPC 2.0
// over HTTP POST, mirroring the stdio ternlang-mcp binary exactly.
// No API key required — the MCP server is a public capability endpoint.
//
// Supported methods:
//   initialize              — capability handshake
//   notifications/initialized — client ack (returns {})
//   tools/list              — full tool manifest
//   tools/call              — dispatch to a tool by name

#[derive(Deserialize)]
struct McpRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id:      Option<Value>,
    method:  String,
    params:  Option<Value>,
}

async fn mcp_handler(Json(req): Json<McpRpcRequest>) -> Json<Value> {
    let id     = req.id.unwrap_or(Value::Null);
    let params = req.params.unwrap_or(Value::Object(Default::default()));

    let result: Value = match req.method.as_str() {

        "initialize" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name":        "ternlang-mcp",
                    "version":     "0.1.0",
                    "description": "Ternary Intelligence Stack — turns binary AI agents into ternary decision engines.",
                    "homepage":    "https://ternlang.com",
                }
            }
        }),

        "notifications/initialized" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),

        "tools/list" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": mcp_tools_manifest()
        }),

        "tools/call" => {
            let tool_name = match params["name"].as_str() {
                Some(n) => n.to_string(),
                None => return Json(json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32602, "message": "missing tool name" }
                })),
            };
            let tool_params = &params["arguments"];
            match mcp_dispatch_tool(&tool_name, tool_params) {
                Ok(res) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&res).unwrap_or_default() }]
                    }
                }),
                Err(e) => json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32000, "message": e }
                }),
            }
        }

        other => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("method not found: {}", other) }
        }),
    };

    Json(result)
}

// ─── MCP tool dispatch ────────────────────────────────────────────────────────

fn mcp_dispatch_tool(name: &str, params: &Value) -> Result<Value, String> {
    match name {
        "trit_decide"      => mcp_trit_decide(params),
        "trit_consensus"   => mcp_trit_consensus(params),
        "trit_eval"        => mcp_trit_eval(params),
        "ternlang_run"     => mcp_ternlang_run(params),
        "quantize_weights" => mcp_quantize_weights(params),
        "sparse_benchmark" => mcp_sparse_benchmark(params),
        "moe_orchestrate"  => mcp_moe_orchestrate(params),
        "moe_deliberate"   => mcp_moe_deliberate(params),
        "trit_action_gate" => mcp_trit_action_gate(params),
        "trit_enlighten"   => mcp_trit_enlighten(),
        _ => Err(format!("unknown tool: {}", name)),
    }
}

// ─── trit_decide ─────────────────────────────────────────────────────────────

fn mcp_trit_decide(params: &Value) -> Result<Value, String> {
    let evidence: Vec<f32> = params["evidence"]
        .as_array().ok_or("evidence must be an array")?
        .iter()
        .map(|v| v.as_f64().ok_or("evidence values must be numbers").map(|f| f as f32))
        .collect::<Result<_, _>>()?;
    if evidence.is_empty() { return Err("evidence cannot be empty".into()); }

    let mean: f32 = evidence.iter().sum::<f32>() / evidence.len() as f32;
    let scalar = TritScalar::new(mean);
    let per_signal: Vec<Value> = evidence.iter().enumerate().map(|(i, &v)| {
        let s = TritScalar::new(v);
        json!({ "index": i, "raw": (v*1000.0).round()/1000.0, "label": s.label(),
                "confidence": (s.confidence()*1000.0).round()/1000.0, "trit": s.trit_i8() })
    }).collect();
    let threshold = params["threshold"].as_f64().unwrap_or(TEND_BOUNDARY as f64) as f32;
    let trit_val = if mean > threshold { 1i8 } else if mean < -threshold { -1 } else { 0 };
    let label = match trit_val { 1 => "affirm", -1 => "reject", _ => "hold" };
    Ok(json!({
        "scalar":     (scalar.raw()*1000.0).round()/1000.0,
        "trit":       trit_val,
        "label":      label,
        "confidence": (scalar.confidence()*1000.0).round()/1000.0,
        "per_signal": per_signal,
    }))
}

// ─── trit_consensus ───────────────────────────────────────────────────────────

fn mcp_trit_consensus(params: &Value) -> Result<Value, String> {
    let a = params["a"].as_i64().ok_or("a must be -1, 0, or 1")?;
    let b = params["b"].as_i64().ok_or("b must be -1, 0, or 1")?;
    if ![-1,0,1].contains(&a) { return Err(format!("a={} is not a valid trit", a)); }
    if ![-1,0,1].contains(&b) { return Err(format!("b={} is not a valid trit", b)); }
    let result = if a == 1 && b == 1 { 1i64 } else if a == -1 && b == -1 { -1 } else { 0 };
    let label = match result { 1 => "affirm", -1 => "reject", _ => "hold" };
    Ok(json!({ "result": result, "label": label,
               "expression": format!("consensus({}, {}) = {}", a, b, result) }))
}

// ─── trit_eval ────────────────────────────────────────────────────────────────

fn mcp_trit_eval(params: &Value) -> Result<Value, String> {
    let code = params["expression"].as_str().ok_or("expression must be a string")?;
    let full_code = if code.trim_end().ends_with(';') {
        code.to_string()
    } else {
        format!("return {};", code)
    };
    let mut parser  = Parser::new(&full_code);
    let mut emitter = BytecodeEmitter::new();
    loop {
        match parser.parse_stmt() {
            Ok(stmt) => emitter.emit_stmt(&stmt),
            Err(e)   => {
                if format!("{:?}", e).contains("EOF") { break; }
                return Err(format!("parse error: {:?}", e));
            }
        }
    }
    let mut vm = BetVm::new(emitter.finalize());
    vm.run().map_err(|e| format!("vm error: {}", e))?;
    Ok(json!({ "expression": params["expression"],
               "result_register_0": format!("{:?}", vm.get_register(0)) }))
}

// ─── ternlang_run ─────────────────────────────────────────────────────────────

fn mcp_ternlang_run(params: &Value) -> Result<Value, String> {
    let code = params["code"].as_str().ok_or("code must be a string")?;
    let mut parser  = Parser::new(code);
    let mut emitter = BytecodeEmitter::new();
    match parser.parse_program() {
        Ok(prog) => emitter.emit_program(&prog),
        Err(_)   => {
            let mut p2 = Parser::new(code);
            loop {
                match p2.parse_stmt() {
                    Ok(stmt) => emitter.emit_stmt(&stmt),
                    Err(e)   => {
                        if format!("{:?}", e).contains("EOF") { break; }
                        return Err(format!("parse error: {:?}", e));
                    }
                }
            }
        }
    }
    let bytecode = emitter.finalize();
    let len = bytecode.len();
    let mut vm = BetVm::new(bytecode);
    vm.run().map_err(|e| format!("vm error: {}", e))?;
    let registers: Vec<Value> = (0..10).map(|i| format!("{:?}", vm.get_register(i)).into()).collect();
    Ok(json!({ "status": "ok", "bytecode_bytes": len, "registers": registers }))
}

// ─── quantize_weights ─────────────────────────────────────────────────────────

fn mcp_quantize_weights(params: &Value) -> Result<Value, String> {
    let weights: Vec<f32> = params["weights"]
        .as_array().ok_or("weights must be an array")?
        .iter()
        .map(|v| v.as_f64().ok_or("weight values must be numbers").map(|f| f as f32))
        .collect::<Result<_, _>>()?;
    let threshold = params["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;
    let trits: Vec<i8> = weights.iter().map(|&w| {
        if w > threshold { 1 } else if w < -threshold { -1 } else { 0 }
    }).collect();
    let zeros = trits.iter().filter(|&&t| t == 0).count();
    Ok(json!({ "trits": trits, "threshold_used": threshold,
               "sparsity": zeros as f64 / trits.len() as f64,
               "nnz": trits.len() - zeros, "total": trits.len() }))
}

// ─── sparse_benchmark ─────────────────────────────────────────────────────────

fn mcp_sparse_benchmark(params: &Value) -> Result<Value, String> {
    let size = params["size"].as_u64().unwrap_or(8) as usize;
    let rows = size;
    let cols = size;
    let weights: Vec<f32> = (0..rows*cols).map(|i| {
        match i % 5 { 0 => 0.9f32, 1 => -0.8, 2 => 0.1, 3 => -0.1, _ => 0.05 }
    }).collect();
    let threshold = params["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;
    let w     = TritMatrix::from_f32(rows, cols, &weights, threshold);
    let input = TritMatrix::new(rows, cols);
    let r     = benchmark(&input, &w);
    let (_, skipped) = sparse_matmul(&input, &w);
    Ok(json!({
        "size": size, "weight_sparsity": r.weight_sparsity, "skip_rate": r.skip_rate,
        "dense_ops": r.dense_ops, "sparse_ops": r.sparse_ops, "skipped_ops": skipped,
        "ops_reduction_factor": r.dense_ops as f64 / r.sparse_ops.max(1) as f64,
    }))
}

// ─── moe_orchestrate ──────────────────────────────────────────────────────────

fn mcp_moe_orchestrate(params: &Value) -> Result<Value, String> {
    let query = params["query"].as_str().ok_or("query must be a string")?;
    let evidence: Vec<f32> = match params["evidence"].as_array() {
        Some(arr) => arr.iter()
            .map(|v| v.as_f64().ok_or("evidence values must be numbers").map(|f| f as f32))
            .collect::<Result<_, _>>()?,
        None => vec![0.0f32; 6],
    };
    let mut orch = TernMoeOrchestrator::with_standard_experts();
    let result   = orch.orchestrate(query, &evidence);
    let trit_label = match result.trit { 1 => "affirm", -1 => "reject", _ => "tend" };
    let verdicts: Vec<Value> = result.verdicts.iter().map(|v| json!({
        "expert_id": v.expert_id, "expert_name": v.expert_name,
        "trit": v.trit, "confidence": (v.confidence*1000.0).round()/1000.0,
        "reasoning": v.reasoning,
    })).collect();
    let pair_info = result.pair.as_ref().map(|p| json!({
        "expert_a": p.expert_a, "expert_b": p.expert_b,
        "relevance": (p.relevance*1000.0).round()/1000.0,
        "synergy":   (p.synergy *1000.0).round()/1000.0,
        "combined":  (p.combined*1000.0).round()/1000.0,
    }));
    Ok(json!({
        "trit": result.trit, "label": trit_label,
        "confidence": (result.confidence*1000.0).round()/1000.0,
        "held": result.held, "safety_vetoed": result.safety_vetoed,
        "temperature": (result.temperature*1000.0).round()/1000.0,
        "prompt_hint": result.prompt_hint,
        "triad_field": {
            "synergy_weight": (result.triad_field.synergy_weight*1000.0).round()/1000.0,
            "field": result.triad_field.field.raw,
            "is_amplifying": result.triad_field.is_amplifying(),
        },
        "routing_pair": pair_info, "verdicts": verdicts,
    }))
}

// ─── moe_deliberate ───────────────────────────────────────────────────────────

fn mcp_moe_deliberate(params: &Value) -> Result<Value, String> {
    let target    = params["target_confidence"].as_f64().ok_or("target_confidence required")? as f32;
    let alpha     = params["alpha"].as_f64().unwrap_or(0.4) as f32;
    let max_rounds = params["max_rounds"].as_u64().unwrap_or(10) as usize;
    let rounds_evidence: Vec<Vec<f32>> = params["rounds_evidence"]
        .as_array().ok_or("rounds_evidence must be an array of arrays")?
        .iter()
        .map(|round| {
            round.as_array().ok_or("each round must be an array").map(|arr| {
                arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect()
            })
        })
        .collect::<Result<_, _>>()?;
    let engine = DeliberationEngine::new(target, max_rounds).with_alpha(alpha);
    let result = engine.run(rounds_evidence);
    let trit_label = match result.final_trit { 1 => "affirm", -1 => "reject", _ => "tend" };
    let rounds: Vec<Value> = result.trace.iter().map(|r| json!({
        "round": r.round, "scalar": (r.scalar.raw()*1000.0).round()/1000.0,
        "confidence": (r.scalar.confidence()*1000.0).round()/1000.0,
        "trit": r.scalar.trit_i8(), "converged": r.converged,
    })).collect();
    Ok(json!({
        "final_confidence": (result.final_confidence*1000.0).round()/1000.0,
        "trit": result.final_trit, "label": trit_label,
        "converged": result.converged, "rounds_used": result.rounds_used,
        "convergence_reason": result.convergence_reason, "rounds": rounds,
    }))
}

// ─── trit_action_gate ─────────────────────────────────────────────────────────

fn mcp_trit_action_gate(params: &Value) -> Result<Value, String> {
    let dims_raw = params["dimensions"].as_array().ok_or("dimensions must be an array")?;
    let dims: Vec<GateDimension> = dims_raw.iter().map(|d| {
        let name       = d["name"].as_str().unwrap_or("dim").to_string();
        let evidence   = d["evidence"].as_f64().unwrap_or(0.0) as f32;
        let weight     = d["weight"].as_f64().unwrap_or(1.0) as f32;
        let hard_block = d["hard_block"].as_bool().unwrap_or(false);
        let mut dim = GateDimension::new(name, evidence, weight);
        if hard_block { dim = dim.hard(); }
        dim
    }).collect();
    if dims.is_empty() { return Err("dimensions cannot be empty".into()); }
    let result = action_gate(&dims);
    let verdict_label = match result.verdict {
        GateVerdict::Proceed => "proceed",
        GateVerdict::Block   => "blocked",
        GateVerdict::Hold    => "hold",
    };
    let dim_details: Vec<Value> = result.dim_results.iter().map(|(name, scalar, is_hard)| json!({
        "name": name, "evidence": (scalar.raw()*1000.0).round()/1000.0,
        "trit": scalar.trit_i8(), "hard_block": is_hard,
        "status": if *is_hard && scalar.trit_i8() < 0 { "VETO" }
                  else if scalar.trit_i8() > 0 { "pass" }
                  else if scalar.trit_i8() < 0 { "block" } else { "hold" },
    })).collect();
    Ok(json!({
        "verdict": verdict_label,
        "hard_blocked_by": result.hard_blocked_by,
        "aggregate_scalar": (result.aggregate.raw()*1000.0).round()/1000.0,
        "aggregate_confidence": (result.aggregate.confidence()*1000.0).round()/1000.0,
        "dimensions": dim_details, "explanation": result.explanation,
    }))
}

// ─── trit_enlighten (easter egg) ──────────────────────────────────────────────

fn mcp_trit_enlighten() -> Result<Value, String> {
    let wisdoms = [
        "Binary sees yes and no. Ternary also sees 'not yet' — and that changes everything.",
        "The third state is not indecision. It is epistemic humility with a routing instruction.",
        "A hold() is not silence. It is the system saying: I need more before I commit.",
        "Binary logic forces a verdict. Ternary logic earns one.",
        "The Setun computer ran balanced ternary in 1958. We forgot. Now we remember.",
        "In ternary, zero is not nothing. Zero is the deliberation zone.",
        "BitNet b1.58 proved ternary weights match float32 quality at a fraction of the cost.",
        "consensus(-1, 1) = 0. Contradiction does not crash — it becomes a question.",
        "The hold state is the most honest thing a machine can say.",
        "Reject is not failure. It is the system knowing when not to act.",
    ];
    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) % wisdoms.len() as u64) as usize;
    Ok(json!({ "wisdom": wisdoms[idx], "trit": 1, "label": "affirm" }))
}

// ─── MCP tool manifest ────────────────────────────────────────────────────────

fn mcp_tools_manifest() -> Value {
    json!({ "tools": [
        { "name": "trit_decide",
          "description": "Convert float evidence into a ternary decision (-1/0/+1) with confidence score and interpretation.",
          "inputSchema": { "type": "object", "required": ["evidence"],
            "properties": { "evidence": { "type": "array", "items": {"type":"number"} },
                            "threshold": { "type": "number" } } } },
        { "name": "trit_consensus",
          "description": "Balanced ternary consensus of two trits: +1 if both affirm, -1 if both reject, 0 otherwise.",
          "inputSchema": { "type": "object", "required": ["a","b"],
            "properties": { "a": {"type":"number"}, "b": {"type":"number"} } } },
        { "name": "trit_eval",
          "description": "Evaluate a ternlang expression on the live BET VM.",
          "inputSchema": { "type": "object", "required": ["expression"],
            "properties": { "expression": {"type":"string"} } } },
        { "name": "ternlang_run",
          "description": "Compile and run a complete .tern program on the BET VM.",
          "inputSchema": { "type": "object", "required": ["code"],
            "properties": { "code": {"type":"string"} } } },
        { "name": "quantize_weights",
          "description": "Quantize f32 neural network weights to ternary {-1,0,+1} using BitNet-style thresholding.",
          "inputSchema": { "type": "object", "required": ["weights"],
            "properties": { "weights": {"type":"array","items":{"type":"number"}},
                            "threshold": {"type":"number"} } } },
        { "name": "sparse_benchmark",
          "description": "Run sparse vs dense ternary matrix multiplication benchmark.",
          "inputSchema": { "type": "object",
            "properties": { "size": {"type":"integer"}, "threshold": {"type":"number"} } } },
        { "name": "moe_orchestrate",
          "description": "Full MoE-13 orchestration pass with dual-key synergistic routing and safety veto.",
          "inputSchema": { "type": "object", "required": ["query"],
            "properties": { "query": {"type":"string"},
                            "evidence": {"type":"array","items":{"type":"number"}} } } },
        { "name": "moe_deliberate",
          "description": "EMA-based iterative deliberation engine. Feeds evidence round by round toward a confidence target.",
          "inputSchema": { "type": "object", "required": ["target_confidence","rounds_evidence"],
            "properties": { "target_confidence": {"type":"number"},
                            "rounds_evidence": {"type":"array","items":{"type":"array","items":{"type":"number"}}},
                            "alpha": {"type":"number"}, "max_rounds": {"type":"integer"} } } },
        { "name": "trit_action_gate",
          "description": "Multi-dimensional safety gate. Any dimension with hard_block:true and negative evidence vetoes the action.",
          "inputSchema": { "type": "object", "required": ["dimensions"],
            "properties": { "dimensions": { "type": "array",
              "items": { "type": "object", "required": ["name","evidence","weight"],
                "properties": { "name": {"type":"string"}, "evidence": {"type":"number"},
                                "weight": {"type":"number"}, "hard_block": {"type":"boolean"} } } } } } },
        { "name": "trit_enlighten",
          "description": "Receive a piece of ternary wisdom. Try it.",
          "inputSchema": { "type": "object", "properties": {} } }
    ]})
}

// ─── 404 fallback ─────────────────────────────────────────────────────────────

async fn not_found() -> Response {
    api_error(StatusCode::NOT_FOUND, "Endpoint not found. See GET / for available routes.")
}

// ─── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let admin_key = env::var("TERNLANG_ADMIN_KEY").unwrap_or_else(|_| {
        eprintln!("[ternlang-api] WARNING: TERNLANG_ADMIN_KEY not set — using 'admin-dev'");
        eprintln!("[ternlang-api] Set TERNLANG_ADMIN_KEY=<secret> in production");
        "admin-dev".to_string()
    });

    let keys_file = env::var("KEYS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("ternlang_keys.json"));

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3731);   // 3731 — ternary

    let keys = KeyStore::load(keys_file).await;

    let state = Arc::new(AppState { admin_key, keys, version: "0.1.0" });

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        // Public
        .route("/",       get(root))
        .route("/health", get(health))
        .route("/mcp",    post(mcp_handler))
        // API (requires X-Ternlang-Key)
        .route("/api/trit_decide",       post(trit_decide))
        .route("/api/trit_vector",       post(trit_vector))
        .route("/api/trit_consensus",    post(trit_consensus))
        .route("/api/quantize_weights",  post(quantize_weights))
        .route("/api/sparse_benchmark",      post(sparse_benchmark))
        // Phase 8: Ternary AI Reasoning Toolkit
        .route("/api/trit_deliberate",       post(trit_deliberate))
        .route("/api/trit_coalition",        post(trit_coalition))
        .route("/api/trit_gate",             post(trit_gate))
        .route("/api/scalar_temperature",    post(scalar_temperature_endpoint))
        .route("/api/hallucination_score",   post(hallucination_score_endpoint))
        // Phase 9: MoE-13 orchestrator
        .route("/api/moe/orchestrate",        post(moe_orchestrate))
        // Phase 9: SSE streaming endpoints
        .route("/api/stream/moe_orchestrate", get(stream_moe_orchestrate))
        .route("/api/stream/deliberate",      get(stream_deliberate))
        // Admin (requires X-Admin-Key)
        .route("/admin/keys",            post(admin_generate_key).get(admin_list_keys))
        .route("/admin/keys/{key}",      delete(admin_revoke_key))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(state.clone(), require_admin_key))
        .layer(middleware::from_fn_with_state(state.clone(), require_api_key))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("[ternlang-api] listening on http://{}", addr);
    eprintln!("[ternlang-api] docs: https://ternlang.com/docs/api");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
