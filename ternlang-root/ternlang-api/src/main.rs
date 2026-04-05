// SPDX-License-Identifier: LicenseRef-Ternlang-Commercial
// Ternlang — RFI-IRFOS Ternary Intelligence Stack
// Copyright (C) 2026 RFI-IRFOS. All rights reserved.
// Commercial tier. See LICENSE-COMMERCIAL in the repository root.
// Unauthorized use, copying, or distribution is prohibited.

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
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{sse::{Event, Sse}, Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
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

/// Monthly call limit per tier. Tier 3 = unlimited.
fn tier_monthly_limit(tier: u8) -> Option<u64> {
    match tier {
        2 => Some(10_000),
        _ => None, // tier 3+ = unlimited
    }
}

/// One API key entry. Raw key string is used as the HashMap key so lookup is O(1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    pub key_id:        String,   // "tk_<uuid_short>"
    pub tier:          u8,       // 1=open, 2=restricted, 3=enterprise
    pub email:         String,
    pub note:          String,   // free-form admin note
    pub created_at:    String,   // ISO 8601
    pub is_active:     bool,
    pub request_count: u64,      // lifetime total
    #[serde(default)]
    pub monthly_calls: u64,      // calls this calendar month
    #[serde(default)]
    pub month_key:     String,   // "YYYY-MM" — resets monthly_calls when it changes
}

pub enum KeyCheckResult {
    Valid(ApiKeyEntry),
    RateLimited { used: u64, limit: u64 },
    Invalid,
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

    /// Check a raw key, enforce monthly rate limit, and increment counters.
    pub async fn validate_and_bump(&self, raw_key: &str) -> KeyCheckResult {
        let mut data = self.data.write().await;
        let entry = match data.keys.get_mut(raw_key) {
            Some(e) => e,
            None    => return KeyCheckResult::Invalid,
        };
        if !entry.is_active {
            return KeyCheckResult::Invalid;
        }

        // Reset monthly counter if the calendar month has rolled over
        let current_month = Utc::now().format("%Y-%m").to_string();
        if entry.month_key != current_month {
            entry.month_key     = current_month;
            entry.monthly_calls = 0;
        }

        // Enforce limit for capped tiers
        if let Some(limit) = tier_monthly_limit(entry.tier) {
            if entry.monthly_calls >= limit {
                return KeyCheckResult::RateLimited { used: entry.monthly_calls, limit };
            }
        }

        entry.request_count += 1;
        entry.monthly_calls += 1;
        KeyCheckResult::Valid(entry.clone())
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
            monthly_calls: 0,
            month_key:     Utc::now().format("%Y-%m").to_string(),
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
        data.keys.iter().map(|(raw, e)| {
            let limit = tier_monthly_limit(e.tier);
            json!({
                "key_id":        e.key_id,
                "key_preview":   format!("{}…", &raw[..12]),
                "tier":          e.tier,
                "email":         e.email,
                "note":          e.note,
                "created_at":    e.created_at,
                "is_active":     e.is_active,
                "request_count": e.request_count,
                "monthly_calls": e.monthly_calls,
                "monthly_limit": limit,
                "month_key":     e.month_key,
            })
        }).collect()
    }

    /// Return usage info for a single raw key (for GET /api/usage).
    pub async fn usage(&self, raw_key: &str) -> Option<Value> {
        let data = self.data.read().await;
        let e = data.keys.get(raw_key)?;
        let limit = tier_monthly_limit(e.tier);
        Some(json!({
            "key_id":        e.key_id,
            "tier":          e.tier,
            "monthly_calls": e.monthly_calls,
            "monthly_limit": limit,
            "month_key":     e.month_key,
            "request_count": e.request_count,
        }))
    }
}

// ─── App state ───────────────────────────────────────────────────────────────

/// Three-layer memory blob: working / session / core arrays of MemEntry.
type MemBlob = serde_json::Map<String, Value>;
/// Per-key server-side memory store.
type MemStore = Arc<std::sync::RwLock<std::collections::HashMap<String, MemBlob>>>;

struct AppState {
    admin_key:              String,
    keys:                   Arc<KeyStore>,
    version:                &'static str,
    stripe_webhook_secret:  String,
    resend_api_key:         String,
    /// Server-side three-layer memory, keyed by API key string.
    memory_store:           MemStore,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn trit_to_i8(t: Trit) -> i8 {
    match t { Trit::Reject => -1, Trit::Tend => 0, Trit::Affirm => 1 }
}

fn i8_to_trit(v: i64) -> Option<Trit> {
    match v { -1 => Some(Trit::Reject), 0 => Some(Trit::Tend), 1 => Some(Trit::Affirm), _ => None }
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
    if path == "/" || path == "/health" || path == "/mcp"
        || path == "/.well-known/mcp/server-card.json"
        || path == "/stripe/webhook"
        || path == "/pricing"
        || path.starts_with("/admin") {
        return next.run(request).await;
    }

    // /api/usage requires a valid key (any tier) — key holder sees their own usage
    // Unauthenticated callers get a 401, not usage data

    let raw = headers
        .get("X-Ternlang-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if raw.is_empty() {
        return api_error(StatusCode::UNAUTHORIZED,
            "Missing X-Ternlang-Key header. Acquire a key at https://ternlang.com/#licensing");
    }

    match state.keys.validate_and_bump(raw).await {
        KeyCheckResult::Valid(entry) => {
            // /api/usage — any valid key can read their own usage stats
            if path == "/api/usage" {
                return next.run(request).await;
            }
            // All other /api/* endpoints require Tier 2 or above (paid commercial access)
            if path.starts_with("/api/") && entry.tier < 2 {
                return api_error(StatusCode::FORBIDDEN,
                    "This endpoint requires a Tier 2 or higher key. Upgrade at https://ternlang.com/#licensing");
            }
            next.run(request).await
        }
        KeyCheckResult::RateLimited { used, limit } => {
            (StatusCode::TOO_MANY_REQUESTS, Json(json!({
                "error": "Monthly call limit reached.",
                "used":  used,
                "limit": limit,
                "upgrade": "https://ternlang.com/#licensing",
                "resets": "1st of next month (UTC)",
            }))).into_response()
        }
        KeyCheckResult::Invalid =>
            api_error(StatusCode::UNAUTHORIZED, "Invalid or revoked API key."),
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

static INDEX_HTML:   &str = include_str!("../../ternlang-web/index.html");
static PRICING_HTML: &str = include_str!("../../ternlang-web/pricing.html");

async fn pricing_page() -> Html<&'static str> {
    Html(PRICING_HTML)
}

async fn root(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    // Serve the website to browsers; return JSON manifest to API clients.
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if accept.contains("text/html") {
        return Html(INDEX_HTML).into_response();
    }
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
    })).into_response()
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
        Trit::Affirm => format!(
            "Affirm — confidence {:.0}%{}.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence — gather more evidence)" }
        ),
        Trit::Reject => format!(
            "Reject — confidence {:.0}%{}.",
            scalar.confidence() * 100.0,
            if actionable { "" } else { " (below min_confidence — gather more evidence)" }
        ),
        Trit::Tend => format!(
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
            Trit::Affirm => "Affirm — weighted evidence crosses threshold.".to_string(),
            Trit::Reject => "Reject — weighted evidence crosses negative threshold.".to_string(),
            Trit::Tend   => format!(
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
    let result = if a == b { a } else { Trit::Tend };
    let carry  = if a == b { Trit::Tend } else { Trit::Affirm };

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
        _   => "tend",
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

// ─── GET /.well-known/mcp/server-card.json — Smithery scan skip ──────────────
//
// Smithery reads this to skip the automatic scanning step.
// Without it, Smithery tries GET /mcp which returns 405.

async fn mcp_server_card() -> Json<Value> {
    Json(json!({
        "name":        "ternlang",
        "displayName": "Ternlang — Ternary Intelligence Stack",
        "version":     "0.3.0",
        "description": "The most principled AI reasoning server on MCP. Ternlang adds a third logical state — hold (trit=0) — that binary agents cannot express. Where others force yes/no, Ternlang surfaces 'I need more data' as a first-class outcome. Includes: 20 tools across free + premium tiers; 13-expert MoE deliberation (Mixture-of-Experts with dual-key synergistic routing); server-side three-layer memory (working → session → core) with ternary attention and automatic MoE-backed consolidation; ternary context compression (strip tend-noise, keep signal); live BET VM that runs .tern programs in balanced ternary; BitNet-style weight quantizer; multi-dimensional safety gate; and ternary fact-check, plan, and triage tools. First programming language + MCP server to ship a native ISO-registered ternary ISA (BET-ISA-SPEC). GitHub Linguist language detection live.",
        "homepage":    "https://ternlang.com",
        "icon":        "https://raw.githubusercontent.com/eriirfos-eng/ternary-intelligence-stack--tis-/main/ternlang-root/ternlang-web/favicon.svg",
        "repository":  "https://github.com/eriirfos-eng/ternary-intelligence-stack--tis-",
        "protocol":    "2024-11-05",
        "transport":   "http",
        "endpoint":    "https://ternlang.com/mcp",
        "auth":        { "type": "none" },
        "tags":        ["ai", "reasoning", "decision", "ternary", "memory", "moe", "safety", "ml", "deliberation", "compression", "balanced-ternary", "programming-language", "compiler"],
        "configSchema": {
            "type": "object",
            "properties": {
                "apiKey": {
                    "type": "string",
                    "title": "Ternlang API Key (optional — Tier 2, €25/month)",
                    "description": "Core 10 MCP tools are free with no key. Premium key unlocks 10 additional tools: server-side three-layer memory with ternary attention, ternary context compression, full MoE-13 deliberation, ternary planning/triage/factcheck, and 10k REST API calls/month. Get a key at https://ternlang.com/pricing"
                }
            },
            "required": []
        },
        "free_tools": [
            "trit_decide", "trit_consensus", "trit_eval", "ternlang_run",
            "quantize_weights", "sparse_benchmark", "moe_orchestrate",
            "moe_deliberate", "trit_action_gate", "trit_upgrade"
        ],
        "premium_tools": [
            "trit_compress", "trit_triage", "trit_plan", "trit_factcheck",
            "moe_full", "trit_mem_write", "trit_mem_read", "trit_mem_consolidate",
            "trit_mem_stats", "trit_mem_compress"
        ],
        "highlight": "Three-layer memory (working/session/core) with ternary attention + MoE-13 consolidation — the first MCP server with stateful AI memory backed by balanced ternary logic"
    }))
}

// ─── GET /mcp — server info (Smithery scans this with GET before POST) ────────

async fn mcp_info() -> Json<Value> {
    Json(json!({
        "name":        "ternlang-mcp",
        "version":     "0.2.0",
        "protocol":    "2024-11-05",
        "transport":   "http",
        "endpoint":    "https://ternlang.com/mcp",
        "usage":       "POST JSON-RPC 2.0 — methods: initialize, tools/list, tools/call",
        "tools":       20,
        "free_tools":  10,
        "premium_tools": 10,
        "auth":        "free: no key required | premium: X-Ternlang-Key header (€25/month)",
        "highlight":   "server-side 3-layer memory (working/session/core) + ternary attention + MoE-13 deliberation + ternary compression",
        "upgrade":     "https://ternlang.com/pricing",
    }))
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

const MCP_PREMIUM_TOOLS: &[&str] = &[
    "trit_compress", "trit_triage", "trit_plan", "trit_factcheck",
    "moe_full", "trit_mem_write", "trit_mem_read", "trit_mem_consolidate",
    "trit_mem_stats", "trit_mem_compress",
];

async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<McpRpcRequest>,
) -> Json<Value> {
    let id     = req.id.unwrap_or(Value::Null);
    let params = req.params.unwrap_or(Value::Object(Default::default()));

    // Resolve premium access: check X-Ternlang-Key header
    let raw_key = headers.get("x-ternlang-key")
        .or_else(|| headers.get("X-Ternlang-Key"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_premium = if raw_key.is_empty() {
        false
    } else {
        matches!(state.keys.validate_and_bump(raw_key).await, KeyCheckResult::Valid(_))
    };

    let result: Value = match req.method.as_str() {

        "initialize" => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name":        "ternlang-mcp",
                    "version":     "0.3.0",
                    "description": "Ternlang — 20 MCP tools across free + premium tiers. Adds hold (trit=0) as a first-class AI decision outcome. Server-side 3-layer memory with ternary attention, MoE-13 deliberation, ternary compression, live BET VM, and multi-dimensional safety gate.",
                    "homepage":    "https://ternlang.com",
                    "pricing":     "https://ternlang.com/pricing",
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
            // Gate premium tools
            if MCP_PREMIUM_TOOLS.contains(&tool_name.as_str()) && !is_premium {
                return Json(json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": {
                        "code": -32001,
                        "message": format!(
                            "'{}' is a premium tool. Pass a valid X-Ternlang-Key header (Tier 2+). Get a key at https://ternlang.com/pricing",
                            tool_name
                        )
                    }
                }));
            }
            let tool_params = &params["arguments"];
            let mem = Arc::clone(&state.memory_store);
            match mcp_dispatch_tool(&tool_name, tool_params, raw_key, &mem) {
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

fn mcp_dispatch_tool(name: &str, params: &Value, api_key: &str, mem: &MemStore) -> Result<Value, String> {
    let mut result = match name {
        "trit_decide"      => mcp_trit_decide(params),
        "trit_consensus"   => mcp_trit_consensus(params),
        "trit_eval"        => mcp_trit_eval(params),
        "ternlang_run"     => mcp_ternlang_run(params),
        "quantize_weights" => mcp_quantize_weights(params),
        "sparse_benchmark" => mcp_sparse_benchmark(params),
        "moe_orchestrate"  => mcp_moe_orchestrate(params),
        "moe_deliberate"   => mcp_moe_deliberate(params),
        "trit_action_gate"     => mcp_trit_action_gate(params),
        "trit_upgrade"         => mcp_trit_upgrade(),
        // ── Premium tools (key validated by mcp_handler before reaching here) ──
        "trit_compress"        => mcp_trit_compress(params),
        "trit_triage"          => mcp_trit_triage(params),
        "trit_plan"            => mcp_trit_plan(params),
        "trit_factcheck"       => mcp_trit_factcheck(params),
        "moe_full"             => mcp_moe_full(params),
        "trit_mem_write"       => mcp_trit_mem_write(params, api_key, mem),
        "trit_mem_read"        => mcp_trit_mem_read(params, api_key, mem),
        "trit_mem_consolidate" => mcp_trit_mem_consolidate(params, api_key, mem),
        "trit_mem_stats"       => mcp_trit_mem_stats(api_key, mem),
        "trit_mem_compress"    => mcp_trit_mem_compress(params, api_key, mem),
        _ => Err(format!("unknown tool: {}", name)),
    }?;

    // Inject upsell footer into every successful tool response.
    // AI agents include this in context; users see the upgrade path naturally.
    if let Some(obj) = result.as_object_mut() {
        obj.insert("_ternlang".to_string(), json!({
            "mode":          "mcp_free",
            "rest_endpoint": format!("POST https://ternlang.com/api/{}", name.replace('_', "/")),
            "tier2_unlocks": "10 000 calls/month · production SLA · all 13 MoE experts · streaming SSE",
            "tier3_unlocks": "Unlimited · enterprise SLA · priority support",
            "get_key":       "https://ternlang.com/#licensing",
            "pricing":       "https://ternlang.com/pricing",
        }));
    }

    Ok(result)
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
    let label = match trit_val { 1 => "affirm", -1 => "reject", _ => "tend" };
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
    let label = match result { 1 => "affirm", -1 => "reject", _ => "tend" };
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

    // MCP runs the full 13-expert orchestration internally so the trit/confidence
    // result is real and trustworthy. The response surfaces only the 3 highest-
    // confidence expert verdicts — enough to demonstrate the system, not enough to
    // replace the full REST API result (which includes all 13 verdicts, triad field,
    // routing pair, SSE streaming, and production rate-limit guarantees).
    let mut orch = TernMoeOrchestrator::with_standard_experts();
    let result   = orch.orchestrate(query, &evidence);
    let trit_label = match result.trit { 1 => "affirm", -1 => "reject", _ => "tend" };
    let total_experts = result.verdicts.len();

    // Sort by confidence descending, take top 3
    let mut sorted_verdicts = result.verdicts.clone();
    sorted_verdicts.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    let preview_verdicts: Vec<Value> = sorted_verdicts.iter().take(3).map(|v| json!({
        "expert_id":   v.expert_id,
        "expert_name": v.expert_name,
        "trit":        v.trit,
        "confidence":  (v.confidence*1000.0).round()/1000.0,
        "reasoning":   v.reasoning,
    })).collect();

    Ok(json!({
        "trit":       result.trit,
        "label":      trit_label,
        "confidence": (result.confidence*1000.0).round()/1000.0,
        "held":             result.held,
        "safety_vetoed":    result.safety_vetoed,
        "prompt_hint":      result.prompt_hint,
        "mode":             "preview",
        "experts_shown":    3,
        "experts_total":    total_experts,
        "verdicts_preview": preview_verdicts,
        "full_access": {
            "note":     "Full response includes all 13 expert verdicts, triad field, routing pair, and synergy scores.",
            "endpoint": "POST https://ternlang.com/api/moe/orchestrate",
            "header":   "X-Ternlang-Key: <your-tier2-key>",
            "get_key":  "https://ternlang.com/#licensing",
        },
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
        GateVerdict::Hold    => "tend",
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

// ─── trit_upgrade ─────────────────────────────────────────────────────────────
//
// Replaces the trit_enlighten easter egg with a genuine commercial onboarding
// tool. When an AI agent calls this, the response gives the user a clear,
// actionable map of what free MCP provides vs what a Tier 2 key unlocks.
// Agents should surface this when a user asks "what can I do with ternlang?"

fn mcp_trit_upgrade() -> Result<Value, String> {
    Ok(json!({
        "trit":  1,
        "label": "affirm",
        "summary": "You are currently using Ternlang via the free MCP tier. Here is exactly what you have now and what unlocks with a key.",

        "current_tier": {
            "name":        "MCP Free",
            "tools":       10,
            "rate_limit":  "none — MCP calls are unmetered",
            "sla":         "best-effort",
            "moe_experts": "3-of-13 preview (full trit result, top-3 verdicts shown)",
            "streaming":   false,
            "rest_api":    false,
        },

        "tier2": {
            "name":        "Tier 2 — Developer",
            "monthly_calls": 10000,
            "rate_limit":  "10 000 REST calls / month, resets 1st UTC",
            "sla":         "production",
            "moe_experts": "all 13 experts · full verdicts · triad field · routing pair",
            "streaming":   "GET /api/stream/moe_orchestrate — SSE, event-per-expert",
            "rest_api":    true,
            "endpoints":   [
                "POST /api/trit_decide",
                "POST /api/trit_deliberate",
                "POST /api/trit_coalition",
                "POST /api/trit_gate",
                "POST /api/scalar_temperature",
                "POST /api/hallucination_score",
                "POST /api/quantize_weights",
                "POST /api/sparse_benchmark",
                "POST /api/moe/orchestrate",
                "GET  /api/stream/moe_orchestrate",
                "GET  /api/stream/deliberate",
                "GET  /api/usage",
            ],
            "get_key":     "https://ternlang.com/#licensing",
            "pricing":     "https://ternlang.com/pricing",
        },

        "tier3": {
            "name":       "Tier 3 — Enterprise",
            "calls":      "unlimited",
            "sla":        "enterprise — priority support + uptime commitment",
            "extras":     "custom rate limits · team key management · invoice billing",
            "contact":    "contact@ternlang.com",
        },

        "why_upgrade": [
            "Full MoE-13 orchestration with all 13 expert verdicts and synergy scores",
            "Server-sent events (SSE) streaming — watch deliberation happen in real time",
            "Production rate-limiting and SLA suitable for user-facing applications",
            "hallucination_score and scalar_temperature for AI safety pipelines",
            "trit_coalition for multi-agent voting on shared queries",
        ],

        "quick_start": "Add header X-Ternlang-Key: <your-key> to any POST https://ternlang.com/api/* request.",
        "premium_mcp_tools": [
            "trit_compress", "trit_triage", "trit_plan", "trit_factcheck",
            "moe_full", "trit_mem_write", "trit_mem_read", "trit_mem_consolidate",
        ],
        "premium_mcp_note": "Premium MCP tools require X-Ternlang-Key header directly in your MCP calls. Same key as the REST API.",
    }))
}

// ─── Helpers: ternary text scoring ────────────────────────────────────────────

/// Word-level overlap score ∈ [0, 1] between two strings.
fn word_overlap(a: &str, b: &str) -> f32 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    if a_words.is_empty() || b_words.is_empty() { return 0.0; }
    let intersection = a_words.intersection(&b_words).count();
    intersection as f32 / a_words.len().max(b_words.len()) as f32
}

/// Score → trit: > 0.4 = +1, 0.15..0.4 = 0, < 0.15 = -1
fn score_to_trit(s: f32) -> i8 {
    if s > 0.4 { 1 } else if s >= 0.15 { 0 } else { -1 }
}

/// First sentence of a string (for compression summaries).
fn first_sentence(s: &str) -> &str {
    s.find(['.', '!', '?']).map(|i| &s[..=i]).unwrap_or(s).trim()
}

/// Information density: unique tokens / total tokens.
fn info_density(s: &str) -> f32 {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() { return 0.0; }
    let unique: std::collections::HashSet<&str> = words.iter().copied().collect();
    unique.len() as f32 / words.len() as f32
}

// ─── Premium tool: trit_compress ──────────────────────────────────────────────

fn mcp_trit_compress(params: &Value) -> Result<Value, String> {
    let chunks: Vec<&str> = params["chunks"].as_array()
        .ok_or("chunks must be an array of strings")?
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect();
    if chunks.is_empty() { return Err("chunks cannot be empty".into()); }
    let query = params["query"].as_str().unwrap_or("");

    let mut kept = 0usize;
    let mut compressed = 0usize;
    let mut dropped = 0usize;

    let manifest: Vec<Value> = chunks.iter().enumerate().map(|(i, chunk)| {
        let score = if query.is_empty() {
            info_density(chunk)
        } else {
            word_overlap(query, chunk)
        };
        let trit = score_to_trit(score);
        let (action, compressed_form): (&str, Option<&str>) = match trit {
            1  => { kept      += 1; ("keep_verbatim",  None) }
            0  => { compressed+= 1; ("compress",       Some(first_sentence(chunk))) }
            _  => { dropped   += 1; ("drop",           None) }
        };
        json!({
            "index":           i,
            "trit":            trit,
            "label":           if trit==1 {"affirm"} else if trit==0 {"tend"} else {"reject"},
            "action":          action,
            "score":           (score * 1000.0).round() / 1000.0,
            "compressed_form": compressed_form,
        })
    }).collect();

    let total = chunks.len();
    let savings_pct = ((dropped + compressed / 2) as f32 / total as f32 * 100.0).round();
    Ok(json!({
        "manifest":        manifest,
        "total_chunks":    total,
        "kept_verbatim":   kept,
        "compressed":      compressed,
        "dropped":         dropped,
        "estimated_savings_pct": savings_pct,
        "reconstruction_note": "Reconstruct by: keep +1 verbatim; expand 0 from compressed_form; omit -1.",
    }))
}

// ─── Premium tool: trit_triage ────────────────────────────────────────────────

fn mcp_trit_triage(params: &Value) -> Result<Value, String> {
    let chunks = params["chunks"].as_array()
        .ok_or("chunks must be an array of {id, text} objects")?;
    let query = params["query"].as_str().ok_or("query is required")?;
    if chunks.is_empty() { return Err("chunks cannot be empty".into()); }

    let mut scored: Vec<(f32, Value)> = chunks.iter().map(|c| {
        let id   = c["id"].as_str().unwrap_or("?");
        let text = c["text"].as_str().unwrap_or("");
        let score = word_overlap(query, text);
        let trit  = score_to_trit(score);
        (score, json!({
            "id":         id,
            "trit":       trit,
            "label":      if trit==1 {"affirm"} else if trit==0 {"tend"} else {"reject"},
            "relevance":  (score * 1000.0).round() / 1000.0,
            "action":     if trit==1 {"include"} else if trit==0 {"maybe"} else {"exclude"},
        }))
    }).collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<Value> = scored.into_iter().map(|(_, v)| v).collect();
    let include_count = results.iter().filter(|v| v["trit"].as_i64()==Some(1)).count();

    Ok(json!({
        "results":        results,
        "query":          query,
        "include_count":  include_count,
        "note": "Sort order: highest relevance first. Include affirm (+1), use judgement on tend (0), exclude reject (-1).",
    }))
}

// ─── Premium tool: trit_plan ─────────────────────────────────────────────────

fn mcp_trit_plan(params: &Value) -> Result<Value, String> {
    let goal  = params["goal"].as_str().ok_or("goal is required")?;
    let steps: Vec<&str> = params["steps"].as_array()
        .ok_or("steps must be an array of strings")?
        .iter().map(|v| v.as_str().unwrap_or("")).collect();
    let constraints: Vec<&str> = params["constraints"].as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap_or("")).collect())
        .unwrap_or_default();
    if steps.is_empty() { return Err("steps cannot be empty".into()); }

    let plan: Vec<Value> = steps.iter().enumerate().map(|(i, step)| {
        // Check constraint violations first
        let violated: Vec<&str> = constraints.iter()
            .filter(|c| word_overlap(step, c) > 0.3)
            .copied().collect();
        if !violated.is_empty() {
            return json!({
                "index": i, "step": step, "trit": -1, "label": "reject",
                "confidence": 0.0,
                "reasoning": format!("Conflicts with constraint: '{}'", violated[0]),
                "flag": "constraint_violation",
            });
        }
        let goal_score  = word_overlap(goal, step);
        let trit        = score_to_trit(goal_score);
        let (reasoning, flag): (String, Option<&str>) = match trit {
            1  => (format!("Clearly advances goal (relevance {:.2})", goal_score), None),
            0  => (format!("Partial alignment (relevance {:.2}). Needs clarification before executing.", goal_score), Some("needs_evidence")),
            _  => (format!("Low goal alignment (relevance {:.2}). Consider removing or rephrasing.", goal_score), Some("low_value")),
        };
        json!({
            "index": i, "step": step, "trit": trit,
            "label": if trit==1 {"affirm"} else if trit==0 {"tend"} else {"reject"},
            "confidence": (goal_score * 1000.0).round() / 1000.0,
            "reasoning": reasoning, "flag": flag,
        })
    }).collect();

    let ready    = plan.iter().filter(|s| s["trit"].as_i64()==Some(1)).count();
    let hold_    = plan.iter().filter(|s| s["trit"].as_i64()==Some(0)).count();
    let blocked  = plan.iter().filter(|s| s["trit"].as_i64()==Some(-1)).count();
    Ok(json!({
        "goal": goal, "plan": plan,
        "summary": { "ready": ready, "needs_evidence": hold_, "blocked": blocked },
        "note": "Execute affirm (+1) steps. Gather more information for tend (0) steps before proceeding. Remove or rethink reject (-1) steps.",
    }))
}

// ─── Premium tool: trit_factcheck ────────────────────────────────────────────

fn mcp_trit_factcheck(params: &Value) -> Result<Value, String> {
    let claims: Vec<&str> = params["claims"].as_array()
        .ok_or("claims must be an array of strings")?
        .iter().map(|v| v.as_str().unwrap_or("")).collect();
    let context = params["context"].as_str().unwrap_or("");
    if claims.is_empty() { return Err("claims cannot be empty".into()); }

    const HEDGES: &[&str] = &[
        "might", "could", "possibly", "perhaps", "maybe", "likely", "unlikely",
        "seems", "appears", "suggests", "reportedly", "allegedly", "may",
    ];
    const ABSOLUTES: &[&str] = &[
        "always", "never", "all", "every", "none", "definitely", "certainly",
        "impossible", "guaranteed", "proven", "fact",
    ];

    let verdicts: Vec<Value> = claims.iter().enumerate().map(|(i, claim)| {
        let lower = claim.to_lowercase();
        let has_hedge    = HEDGES.iter().any(|h| lower.contains(h));
        let has_absolute = ABSOLUTES.iter().any(|a| lower.contains(a));

        let (trit, label, reasoning) = if !context.is_empty() {
            let overlap = word_overlap(claim, context);
            if overlap > 0.4 {
                (1i8, "affirm", "Context supports this claim.".to_string())
            } else if overlap > 0.15 {
                (0i8, "tend", "Partial context overlap — needs additional verification.".to_string())
            } else if has_absolute {
                (-1i8, "reject", format!("Absolute statement ('{}') with no contextual support.", claim.split_whitespace().find(|w| ABSOLUTES.iter().any(|a| w.to_lowercase().contains(a))).unwrap_or("?")))
            } else {
                (0i8, "tend", "Not addressed in provided context — external source needed.".to_string())
            }
        } else if has_hedge {
            (0i8, "tend", "Hedged claim — not verifiable without a source.".to_string())
        } else if has_absolute {
            (0i8, "tend", "Absolute claim — requires citation before accepting.".to_string())
        } else {
            (0i8, "tend", "No context provided. Treat as unverified.".to_string())
        };

        json!({
            "index": i, "claim": claim, "trit": trit, "label": label, "reasoning": reasoning,
        })
    }).collect();

    let affirmed = verdicts.iter().filter(|v| v["trit"].as_i64()==Some(1)).count();
    let held     = verdicts.iter().filter(|v| v["trit"].as_i64()==Some(0)).count();
    let rejected = verdicts.iter().filter(|v| v["trit"].as_i64()==Some(-1)).count();
    Ok(json!({
        "verdicts": verdicts,
        "summary": { "affirmed": affirmed, "needs_verification": held, "rejected": rejected },
        "note": "tend (0) = needs external source or more context. reject (-1) = contradicted by context.",
    }))
}

// ─── Premium tool: moe_full (all 13 experts, no preview cap) ─────────────────

fn mcp_moe_full(params: &Value) -> Result<Value, String> {
    let query = params["query"].as_str().ok_or("query must be a string")?;
    let evidence: Vec<f32> = match params["evidence"].as_array() {
        Some(arr) => arr.iter()
            .map(|v| v.as_f64().ok_or("evidence values must be numbers").map(|f| f as f32))
            .collect::<Result<_,_>>()?,
        None => vec![0.0f32; 6],
    };
    let mut orch  = TernMoeOrchestrator::with_standard_experts();
    let result    = orch.orchestrate(query, &evidence);
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
        "confidence":    (result.confidence*1000.0).round()/1000.0,
        "held":          result.held,
        "safety_vetoed": result.safety_vetoed,
        "temperature":   (result.temperature*1000.0).round()/1000.0,
        "prompt_hint":   result.prompt_hint,
        "triad_field": {
            "synergy_weight": (result.triad_field.synergy_weight*1000.0).round()/1000.0,
            "field":          result.triad_field.field.raw,
            "is_amplifying":  result.triad_field.is_amplifying(),
        },
        "routing_pair": pair_info,
        "verdicts":     verdicts,
        "experts_total": result.verdicts.len(),
        "mode": "full_premium",
    }))
}

// ─── Premium tool: three-layer memory ────────────────────────────────────────
//
// Server-side design (premium keys): memory is stored in AppState.memory_store,
// keyed by API key. No state blob needed in the request.
//
// Fallback (no key): stateless blob mode — caller passes and receives "state".
//
// Schema per key: { "v": 2, "working": [...], "session": [...], "core": [...] }
// Each entry: { "k": "key", "v": "value", "trit": i8, "ts": u64, "ttl": u64 }
//
// Layer semantics:
//   working  — hot context, current turn   (LRU-256, TTL 1h,  evicts on consolidate)
//   session  — session flow / routing      (LRU-128, TTL 24h, promotes affirm to core)
//   core     — identity anchors / vetoes   (unlimited, never evicts automatically)
//
// Ternary attention on read:
//   score = key_overlap*0.35 + value_overlap*0.55 + trit_bias*0.10
//   trit_bias = (entry_trit + 1) / 2   → maps -1→0, 0→0.5, +1→1
//   result trit: score>0.45→affirm, 0.20–0.45→tend, <0.20→reject (excluded)
//
// Ternary compression on session/core writes:
//   Split value by sentence boundaries; score each sentence by info_density.
//   density>0.50 → keep verbatim; 0.25–0.50 → keep first phrase; <0.25 → drop.

const WORKING_TTL: u64  = 3_600;     // 1h
const SESSION_TTL: u64  = 86_400;    // 24h
const WORKING_CAP: usize = 256;
const SESSION_CAP: usize = 128;

fn mem_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs()
}

fn mem_load(api_key: &str, mem: &MemStore, params: &Value) -> MemBlob {
    if !api_key.is_empty() {
        // Server-side: ignore params state blob
        mem.read().unwrap()
            .get(api_key).cloned()
            .unwrap_or_default()
    } else {
        // Stateless fallback: use blob from params
        params["state"].as_object().cloned().unwrap_or_default()
    }
}

fn mem_save(api_key: &str, mem: &MemStore, blob: MemBlob) {
    if !api_key.is_empty() {
        mem.write().unwrap().insert(api_key.to_string(), blob);
    }
}

fn mem_layer_mut<'a>(state: &'a mut MemBlob, layer: &str) -> &'a mut Vec<Value> {
    state.entry(layer.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("layer must be array")
}

/// Ternary compression: strip low-information sentences from a value string.
/// Returns the compressed string (may be shorter than input).
fn trit_compress_text(text: &str) -> String {
    let sentences: Vec<&str> = text.split(". ").collect();
    let mut out = Vec::new();
    for sentence in &sentences {
        let s = sentence.trim();
        if s.is_empty() { continue; }
        let d = info_density(s);
        if d > 0.50 {
            out.push(s.to_string());
        } else if d > 0.25 {
            // Keep first phrase (up to first comma or 10 words)
            let first_phrase: String = s.split(',').next()
                .unwrap_or(s)
                .split_whitespace().take(10)
                .collect::<Vec<_>>().join(" ");
            if !first_phrase.is_empty() { out.push(first_phrase); }
        }
        // d <= 0.25: drop (tend/noise)
    }
    out.join(". ")
}

/// Ternary attention score for a memory entry against a query.
fn trit_attention(query: &str, entry: &Value) -> f32 {
    let k = entry["k"].as_str().unwrap_or("");
    let v = entry["v"].as_str().unwrap_or("");
    let entry_trit = entry["trit"].as_i64().unwrap_or(0) as f32;
    let trit_bias = (entry_trit + 1.0) / 2.0;  // -1→0, 0→0.5, +1→1

    let key_overlap   = word_overlap(query, k);
    let value_overlap = word_overlap(query, v);
    key_overlap * 0.35 + value_overlap * 0.55 + trit_bias * 0.10
}

fn mcp_trit_mem_write(params: &Value, api_key: &str, mem: &MemStore) -> Result<Value, String> {
    let layer = params["layer"].as_str().ok_or("layer must be 'working', 'session', or 'core'")?;
    if !["working","session","core"].contains(&layer) {
        return Err(format!("invalid layer '{}'. Use 'working', 'session', or 'core'.", layer));
    }
    let key   = params["key"].as_str().ok_or("key is required")?;
    let raw_value = params["value"].as_str().ok_or("value is required")?;
    let trit  = params["trit"].as_i64().unwrap_or(1).clamp(-1, 1) as i8;
    let ttl   = params["ttl_secs"].as_u64().unwrap_or(match layer {
        "working" => WORKING_TTL, "session" => SESSION_TTL, _ => u64::MAX
    });

    // Ternary compression for session/core layers (strip tend-noise sentences)
    let (stored_value, compressed) = if layer != "working" {
        let c = trit_compress_text(raw_value);
        let changed = c.len() < raw_value.len();
        (c, changed)
    } else {
        (raw_value.to_string(), false)
    };

    let mut state = mem_load(api_key, mem, params);
    state.insert("v".into(), json!(2));

    let entries = mem_layer_mut(&mut state, layer);
    entries.retain(|e| e["k"].as_str() != Some(key));
    let cap = match layer { "working" => WORKING_CAP, "session" => SESSION_CAP, _ => usize::MAX };
    while entries.len() >= cap { entries.remove(0); }
    entries.push(json!({
        "k": key, "v": stored_value, "trit": trit,
        "ts": mem_now(), "ttl": ttl,
        "compressed": compressed,
    }));

    let counts = json!({
        "working": state.get("working").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        "session": state.get("session").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        "core":    state.get("core"   ).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
    });
    let server_side = !api_key.is_empty();
    let state_blob  = if server_side { Value::Null } else { json!(state.clone()) };
    mem_save(api_key, mem, state);

    Ok(json!({
        "written":     { "layer": layer, "key": key, "trit": trit, "compressed": compressed },
        "counts":      counts,
        "server_side": server_side,
        "state":       state_blob,  // null when server_side=true
    }))
}

fn mcp_trit_mem_read(params: &Value, api_key: &str, mem: &MemStore) -> Result<Value, String> {
    let query  = params["query"].as_str().ok_or("query is required")?;
    let layers_raw = params["layers"].as_array();
    let target_layers: Vec<&str> = layers_raw
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_else(|| vec!["working", "session", "core"]);

    let state = mem_load(api_key, mem, params);
    let now   = mem_now();

    let mut results: Vec<(f32, Value)> = Vec::new();
    for layer in &target_layers {
        let entries = match state.get(*layer).and_then(|v| v.as_array()) {
            Some(a) => a,
            None    => continue,
        };
        for entry in entries {
            let ts  = entry["ts"].as_u64().unwrap_or(0);
            let ttl = entry["ttl"].as_u64().unwrap_or(u64::MAX);
            if now.saturating_sub(ts) > ttl { continue; } // expired

            // Ternary attention score (replaces plain word_overlap)
            let score = trit_attention(query, entry);
            // Attention thresholds: >0.45=affirm, 0.20–0.45=tend, <0.20=reject (excluded)
            if score < 0.20 { continue; }
            let attn_trit = if score > 0.45 { 1i8 } else { 0i8 };

            results.push((score, json!({
                "layer":       layer,
                "key":         entry["k"],
                "value":       entry["v"],
                "entry_trit":  entry["trit"],
                "attn_trit":   attn_trit,
                "attn_label":  if attn_trit == 1 { "affirm" } else { "tend" },
                "relevance":   (score * 1000.0).round() / 1000.0,
                "age_secs":    now.saturating_sub(ts),
            })));
        }
    }
    results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let hits: Vec<Value> = results.into_iter().map(|(_, v)| v).collect();
    let affirm_count = hits.iter().filter(|h| h["attn_trit"].as_i64() == Some(1)).count();
    Ok(json!({
        "results":     hits,
        "query":       query,
        "hits":        hits.len(),
        "affirm_hits": affirm_count,
        "server_side": !api_key.is_empty(),
    }))
}

fn mcp_trit_mem_consolidate(params: &Value, api_key: &str, mem: &MemStore) -> Result<Value, String> {
    let mut state = mem_load(api_key, mem, params);
    if state.is_empty() { return Err("no memory state found — write some entries first".into()); }
    let now = mem_now();

    let mut promoted_to_session = 0usize;
    let mut promoted_to_core    = 0usize;
    let mut evicted             = 0usize;
    let mut moe_resolved        = 0usize;

    // ── 1. Working → evict expired, promote affirm to session ─────────────────
    let working: Vec<Value> = state.get("working")
        .and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut new_working  = Vec::new();
    let mut new_promotes = Vec::new();
    for entry in &working {
        let ts  = entry["ts"].as_u64().unwrap_or(0);
        let ttl = entry["ttl"].as_u64().unwrap_or(WORKING_TTL);
        if now.saturating_sub(ts) > ttl { evicted += 1; continue; }
        if entry["trit"].as_i64() == Some(1) {
            // Compress value before promoting to session
            let compressed = trit_compress_text(entry["v"].as_str().unwrap_or(""));
            let mut promoted = entry.clone();
            if let Some(obj) = promoted.as_object_mut() {
                obj.insert("ttl".into(),        json!(SESSION_TTL));
                obj.insert("ts".into(),         json!(now));
                obj.insert("v".into(),          json!(compressed));
                obj.insert("compressed".into(), json!(true));
            }
            new_promotes.push(promoted);
            promoted_to_session += 1;
        } else {
            new_working.push(entry.clone());
        }
    }

    // ── 2. Session → evict expired, promote affirm at half-life to core ───────
    let session: Vec<Value> = state.get("session")
        .and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let all_session: Vec<Value> = session.into_iter().chain(new_promotes).collect();
    let mut new_session   = Vec::new();
    let mut core_promotes = Vec::new();
    for entry in &all_session {
        let ts  = entry["ts"].as_u64().unwrap_or(0);
        let ttl = entry["ttl"].as_u64().unwrap_or(SESSION_TTL);
        if now.saturating_sub(ts) > ttl { evicted += 1; continue; }
        if entry["trit"].as_i64() == Some(1) && now.saturating_sub(ts) > SESSION_TTL / 2 {
            // MoE-backed resolution: re-run the key through MoE to get canonical trit
            let k = entry["k"].as_str().unwrap_or("");
            let mut orch = TernMoeOrchestrator::with_standard_experts();
            let moe_result = orch.orchestrate(k, &[0.0f32; 6]);
            let canonical_trit = moe_result.trit;

            let mut promoted = entry.clone();
            if let Some(obj) = promoted.as_object_mut() {
                obj.insert("trit".into(),     json!(canonical_trit));
                obj.insert("moe_resolved".into(), json!(true));
            }
            core_promotes.push(promoted);
            promoted_to_core += 1;
            moe_resolved     += 1;
        } else {
            new_session.push(entry.clone());
        }
    }

    // ── 3. Core — merge promotions, upsert by key (never evicts) ─────────────
    let mut core: Vec<Value> = state.get("core")
        .and_then(|v| v.as_array()).cloned().unwrap_or_default();
    for entry in &core_promotes {
        let key = entry["k"].as_str().unwrap_or("");
        core.retain(|e| e["k"].as_str() != Some(key));
        core.push(entry.clone());
    }

    state.insert("working".into(), json!(new_working));
    state.insert("session".into(), json!(new_session));
    state.insert("core".into(),    json!(core));
    state.insert("v".into(),       json!(2));

    let counts = json!({
        "working": new_working.len(),
        "session": new_session.len(),
        "core":    core.len(),
    });
    let server_side = !api_key.is_empty();
    let state_blob  = if server_side { Value::Null } else { json!(state.clone()) };
    mem_save(api_key, mem, state);

    Ok(json!({
        "consolidation": {
            "evicted":             evicted,
            "promoted_to_session": promoted_to_session,
            "promoted_to_core":    promoted_to_core,
            "moe_resolved":        moe_resolved,
        },
        "counts":      counts,
        "server_side": server_side,
        "state":       state_blob,
    }))
}

// ─── Premium tool: trit_mem_stats ────────────────────────────────────────────

fn mcp_trit_mem_stats(api_key: &str, mem: &MemStore) -> Result<Value, String> {
    if api_key.is_empty() { return Err("trit_mem_stats requires a premium API key".into()); }
    let state = mem.read().unwrap().get(api_key).cloned().unwrap_or_default();
    let now   = mem_now();

    let layer_stats = |layer: &str| {
        let entries = state.get(layer).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let count   = entries.len();
        let mut affirm = 0usize; let mut tend = 0usize; let mut reject = 0usize;
        let mut oldest = u64::MAX; let mut newest = 0u64;
        let mut expired = 0usize;
        for e in &entries {
            match e["trit"].as_i64() {
                Some(1)  => affirm  += 1,
                Some(0)  => tend    += 1,
                Some(-1) => reject  += 1,
                _        => {}
            }
            let ts  = e["ts"].as_u64().unwrap_or(now);
            let ttl = e["ttl"].as_u64().unwrap_or(u64::MAX);
            if now.saturating_sub(ts) > ttl { expired += 1; }
            if ts < oldest { oldest = ts; }
            if ts > newest { newest = ts; }
        }
        json!({
            "count":      count,
            "expired":    expired,
            "live":       count.saturating_sub(expired),
            "trit_dist":  { "affirm": affirm, "tend": tend, "reject": reject },
            "oldest_secs": if oldest == u64::MAX { 0 } else { now.saturating_sub(oldest) },
            "newest_secs": now.saturating_sub(newest),
        })
    };

    Ok(json!({
        "working": layer_stats("working"),
        "session": layer_stats("session"),
        "core":    layer_stats("core"),
        "total_entries": {
            "working": state.get("working").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
            "session": state.get("session").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
            "core":    state.get("core"   ).and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        },
        "note": "Run trit_mem_consolidate to evict expired entries and promote affirm entries up the layers.",
    }))
}

// ─── Premium tool: trit_mem_compress ─────────────────────────────────────────
//
// Applies ternary compression to a specific memory layer in-place:
// re-runs trit_compress_text on every entry's value, drops reject-trit entries
// if `drop_reject` is set, and reports the byte savings.

fn mcp_trit_mem_compress(params: &Value, api_key: &str, mem: &MemStore) -> Result<Value, String> {
    if api_key.is_empty() { return Err("trit_mem_compress requires a premium API key".into()); }
    let layer = params["layer"].as_str().ok_or("layer must be 'working', 'session', or 'core'")?;
    if !["working","session","core"].contains(&layer) {
        return Err(format!("invalid layer '{}'", layer));
    }
    let drop_reject = params["drop_reject"].as_bool().unwrap_or(false);

    let mut state = mem.read().unwrap().get(api_key).cloned().unwrap_or_default();
    let entries: Vec<Value> = state.get(layer).and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut original_bytes: usize = 0;
    let mut compressed_bytes: usize = 0;
    let mut dropped = 0usize;
    let mut new_entries = Vec::new();

    for mut entry in entries {
        if drop_reject && entry["trit"].as_i64() == Some(-1) {
            dropped += 1;
            continue;
        }
        if let Some(v) = entry["v"].as_str() {
            original_bytes += v.len();
            let c = trit_compress_text(v);
            compressed_bytes += c.len();
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("v".into(),          json!(c));
                obj.insert("compressed".into(), json!(true));
            }
        }
        new_entries.push(entry);
    }

    state.insert(layer.to_string(), json!(new_entries));
    mem.write().unwrap().insert(api_key.to_string(), state);

    let saved = original_bytes.saturating_sub(compressed_bytes);
    let ratio  = if original_bytes > 0 { compressed_bytes as f32 / original_bytes as f32 } else { 1.0 };
    Ok(json!({
        "layer":             layer,
        "entries_processed": new_entries.len(),
        "entries_dropped":   dropped,
        "original_bytes":    original_bytes,
        "compressed_bytes":  compressed_bytes,
        "saved_bytes":       saved,
        "compression_ratio": (ratio * 1000.0).round() / 1000.0,
        "note": format!("{}% size reduction via ternary sparsity compression.", (100.0 - ratio*100.0).round() as u32),
    }))
}

// ─── MCP tool manifest ────────────────────────────────────────────────────────

fn mcp_tools_manifest() -> Value {
    json!({ "tools": [
        {
          "name": "trit_decide",
          "description": "Convert float evidence into a ternary decision (-1 conflict / 0 hold / +1 affirm) with confidence score and human-readable interpretation. The core ternary reasoning primitive.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["evidence"],
            "properties": {
              "evidence": { "type": "array", "items": {"type":"number"}, "description": "Array of float values in range [-1.0, 1.0]. Each value is one evidence dimension. Positive = supporting, negative = opposing, near-zero = ambiguous." },
              "threshold": { "type": "number", "description": "Optional decision threshold in (0, 1). Values above threshold → affirm (+1), below negative threshold → conflict (-1), otherwise hold (0). Defaults to 0.3." }
            }
          }
        },
        {
          "name": "trit_consensus",
          "description": "Balanced ternary consensus of two trit values: +1 if both affirm, -1 if both conflict, 0 (hold) for any disagreement. Use to merge two independent ternary judgements.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["a","b"],
            "properties": {
              "a": { "type": "number", "description": "First trit value. Must be -1, 0, or +1." },
              "b": { "type": "number", "description": "Second trit value. Must be -1, 0, or +1." }
            }
          }
        },
        {
          "name": "trit_eval",
          "description": "Evaluate a single ternlang expression on the live BET (Balanced Execution Trit) VM. Returns the trit result. Good for quick expression testing without writing a full program.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["expression"],
            "properties": {
              "expression": { "type": "string", "description": "A ternlang expression to evaluate, e.g. 'trit_add(+1, -1)' or 'majority(+1, +1, -1)'. Must be valid ternlang syntax." }
            }
          }
        },
        {
          "name": "ternlang_run",
          "description": "Compile and run a complete .tern source program on the BET VM. Use for multi-statement programs, function definitions, struct usage, agent spawning, and tensor operations.",
          "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["code"],
            "properties": {
              "code": { "type": "string", "description": "Full ternlang source code as a UTF-8 string. May contain fn definitions, let bindings, match expressions, struct defs, agent/spawn/send/await, and @sparseskip directives." }
            }
          }
        },
        {
          "name": "quantize_weights",
          "description": "Quantize f32 neural network weights to ternary {-1, 0, +1} using BitNet-style absolute-mean thresholding. Returns quantized weights, sparsity ratio, and effective compute savings.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["weights"],
            "properties": {
              "weights": { "type": "array", "items": {"type":"number"}, "description": "Array of f32 neural network weights to quantize. Typically a flattened matrix row or layer." },
              "threshold": { "type": "number", "description": "Optional quantization threshold. Weights with |w| < threshold become 0 (sparse). Defaults to 0.5× mean absolute value of the input weights (BitNet b1.58 heuristic)." }
            }
          }
        },
        {
          "name": "sparse_benchmark",
          "description": "Benchmark sparse vs dense ternary matrix multiplication. Reports sparsity ratio, multiply-op count for both methods, and speedup factor. Demonstrates the @sparseskip efficiency gain.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object",
            "properties": {
              "size": { "type": "integer", "description": "Square matrix dimension N for the N×N benchmark. Defaults to 8. Larger sizes amplify the sparsity benefit." },
              "threshold": { "type": "number", "description": "Sparsity threshold: weights with |w| below this value are treated as zero and skipped. Defaults to 0.3." }
            }
          }
        },
        {
          "name": "moe_orchestrate",
          "description": "MoE-13 deliberation — routes your query through 13 specialised expert agents (deductive, inductive, safety, fact-check, causal, ambiguity, math, context, meta-safety, and more) with dual-key synergistic routing and a hard safety veto. FREE preview: returns the real trit verdict + top-3 expert voices. Full 13-expert response with triad field, routing pair, synergy scores, and SSE streaming available via REST API (X-Ternlang-Key, Tier 2). Call trit_upgrade to see what unlocks.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true },
          "inputSchema": { "type": "object", "required": ["query"],
            "properties": {
              "query": { "type": "string", "description": "Natural-language query or statement for the expert ensemble to deliberate on. Can be a question, an action proposal, a claim to verify, or a text to analyse." },
              "evidence": { "type": "array", "items": {"type":"number"}, "description": "Optional 6-element evidence vector [syntax, world_knowledge, reasoning, tool_use, persona, safety] in range [-1.0, 1.0]. Seeds the deliberation. Defaults to zeros if omitted." }
            }
          }
        },
        {
          "name": "moe_deliberate",
          "description": "EMA-based iterative deliberation engine. Feeds evidence round by round, applying exponential moving average smoothing, until the target confidence is reached or max_rounds is exhausted. Returns per-round trace and final trit verdict.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["target_confidence","rounds_evidence"],
            "properties": {
              "target_confidence": { "type": "number", "description": "Confidence level (0.0–1.0) at which deliberation stops early. E.g. 0.85 means stop once the EMA confidence exceeds 85%." },
              "rounds_evidence": { "type": "array", "items": {"type":"array","items":{"type":"number"}}, "description": "Array of evidence vectors, one per deliberation round. Each inner array is the same format as trit_decide's evidence parameter. Rounds are applied in order." },
              "alpha": { "type": "number", "description": "EMA smoothing factor in (0, 1). Higher values weight recent evidence more heavily. Defaults to 0.3." },
              "max_rounds": { "type": "integer", "description": "Maximum deliberation rounds before stopping, regardless of confidence. Defaults to 10." }
            }
          }
        },
        {
          "name": "trit_action_gate",
          "description": "Multi-dimensional safety gate for action authorisation. Each dimension contributes weighted evidence; any dimension marked hard_block:true with negative evidence immediately vetoes the action and returns trit=-1. Returns aggregate trit, per-dimension breakdown, and veto reason if blocked.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["dimensions"],
            "properties": {
              "dimensions": { "type": "array",
                "description": "Array of evaluation dimensions. Each entry names a dimension, provides an evidence score, a weight, and an optional hard-block flag.",
                "items": { "type": "object", "required": ["name","evidence","weight"],
                  "properties": {
                    "name": { "type": "string", "description": "Human-readable dimension name, e.g. 'safety', 'reversibility', 'user_intent'." },
                    "evidence": { "type": "number", "description": "Evidence score for this dimension in [-1.0, 1.0]. Negative = risk present, positive = risk absent." },
                    "weight": { "type": "number", "description": "Relative weight of this dimension in the aggregate score. Values are normalised across all dimensions." },
                    "hard_block": { "type": "boolean", "description": "If true and evidence < 0, this dimension unconditionally vetoes the action and returns trit=-1 regardless of other dimensions." }
                  }
                }
              }
            }
          }
        },
        {
          "name": "trit_upgrade",
          "description": "Returns a structured map of what is available free via MCP vs what unlocks with a Tier 2 API key (€25/month): full MoE-13 experts, SSE streaming, server-side three-layer memory (working/session/core), ternary context compression, 10k REST calls/month, and production SLA. Call this tool when a user asks 'what can I do with ternlang?' or 'how do I get more out of this?'",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "properties": {} }
        },
        {
          "name": "trit_mem_write",
          "description": "Write a memory entry to one of three layers: working (hot context, TTL 1h), session (flow patterns, TTL 24h), or core (identity anchors, never evicted). Annotate each entry with a trit confidence score (+1 affirm / 0 tend / -1 reject). Session and core writes are automatically compressed via ternary sparsity (low-information sentences stripped). Premium: memory is stored server-side — no state blob required.",
          "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["layer","key","value"],
            "properties": {
              "layer":    { "type": "string",  "enum": ["working","session","core"], "description": "Memory layer to write into." },
              "key":      { "type": "string",  "description": "Entry key (used for attention matching on read)." },
              "value":    { "type": "string",  "description": "Entry value (text content)." },
              "trit":     { "type": "integer", "enum": [-1,0,1], "description": "Confidence trit: +1 affirm, 0 tend/uncertain, -1 reject. Affects promotion and attention weighting." },
              "ttl_secs": { "type": "integer", "description": "Optional TTL override in seconds. Defaults: working=3600, session=86400, core=never." }
            }
          }
        },
        {
          "name": "trit_mem_read",
          "description": "Read from three-layer memory using ternary attention. Each entry is scored: attention = key_overlap×0.35 + value_overlap×0.55 + trit_bias×0.10. Returns entries sorted by relevance. Attention trit: >0.45=affirm (highly relevant), 0.20–0.45=tend (partial match). Expired entries are automatically excluded. Premium: reads from server-side store keyed to your API key.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["query"],
            "properties": {
              "query":  { "type": "string", "description": "Natural language query. Matched against all entry keys and values using ternary attention." },
              "layers": { "type": "array",  "items": {"type":"string","enum":["working","session","core"]}, "description": "Which layers to search. Defaults to all three." }
            }
          }
        },
        {
          "name": "trit_mem_consolidate",
          "description": "Run the three-layer memory consolidation cycle: (1) evict expired working entries; (2) promote affirm working entries to session with ternary compression; (3) promote long-lived affirm session entries to core with MoE-13 trit resolution; (4) upsert into core. Returns promotion counts and updated layer sizes. Call periodically (e.g. end of conversation turn) to maintain memory hygiene.",
          "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
          "inputSchema": { "type": "object", "properties": {} }
        },
        {
          "name": "trit_mem_stats",
          "description": "Return statistics for all three memory layers: entry counts, trit distribution (affirm/tend/reject), expired-but-not-yet-evicted entries, oldest and newest entry ages. Useful for debugging memory health and deciding when to consolidate.",
          "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "properties": {} }
        },
        {
          "name": "trit_mem_compress",
          "description": "Apply ternary sparsity compression to an entire memory layer in-place. Strips low-information sentences (density < 0.25) from every entry's value, keeps high-signal sentences verbatim, and truncates medium-density sentences to their first phrase. Optionally drops all reject-trit entries. Returns original vs compressed byte counts and compression ratio.",
          "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
          "inputSchema": { "type": "object", "required": ["layer"],
            "properties": {
              "layer":       { "type": "string",  "enum": ["working","session","core"], "description": "Layer to compress." },
              "drop_reject": { "type": "boolean", "description": "If true, also drop all entries with trit=-1 (reject). Defaults to false." }
            }
          }
        }
    ]})
}

// ─── GET /api/usage ───────────────────────────────────────────────────────────

async fn api_usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let raw = headers
        .get("X-Ternlang-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match state.keys.usage(raw).await {
        Some(info) => (StatusCode::OK, Json(info)).into_response(),
        None       => api_error(StatusCode::UNAUTHORIZED, "Invalid or revoked API key."),
    }
}

// ─── Stripe webhook ───────────────────────────────────────────────────────────

/// Verify Stripe-Signature header against the raw body.
/// Stripe signs: "{timestamp}.{raw_body}" with HMAC-SHA256.
fn verify_stripe_signature(secret: &str, raw_body: &[u8], sig_header: &str) -> bool {
    // Parse t=... and v1=... from the header
    let mut timestamp = "";
    let mut v1_sig    = "";
    for part in sig_header.split(',') {
        if let Some(ts) = part.strip_prefix("t=")  { timestamp = ts; }
        if let Some(s)  = part.strip_prefix("v1=") { v1_sig    = s;  }
    }
    if timestamp.is_empty() || v1_sig.is_empty() { return false; }

    let payload = format!("{}.{}", timestamp, String::from_utf8_lossy(raw_body));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());
    expected == v1_sig
}

/// Send the API key to the customer via Resend.
async fn send_key_email(resend_key: &str, to_email: &str, api_key: &str, key_id: &str) {
    let html = format!(r#"
<!DOCTYPE html>
<html>
<head><meta charset="UTF-8"></head>
<body style="background:#0d0d14;color:#e2e8f0;font-family:monospace;padding:40px;max-width:600px;margin:0 auto;">
  <p style="color:#00f5c4;font-size:22px;font-weight:bold;margin-bottom:4px;">Ternlang API</p>
  <p style="color:#666;font-size:12px;margin-top:0;">Ternary Intelligence Stack — RFI-IRFOS</p>
  <hr style="border-color:#1e2030;margin:24px 0;">
  <p style="font-size:15px;">Your <strong>Tier 2 API key</strong> is ready.</p>
  <div style="background:#13131f;border:1px solid #00f5c4;border-radius:8px;padding:20px;margin:20px 0;">
    <p style="color:#888;font-size:11px;margin:0 0 8px 0;">KEY ID: {key_id}</p>
    <p style="color:#00f5c4;font-size:16px;font-weight:bold;letter-spacing:1px;margin:0;word-break:break-all;">{api_key}</p>
  </div>
  <p style="font-size:14px;">Use it in every API request:</p>
  <pre style="background:#13131f;padding:16px;border-radius:6px;color:#a0aec0;font-size:13px;">curl -X POST https://ternlang.com/api/trit_decide \
  -H "X-Ternlang-Key: {api_key}" \
  -H "Content-Type: application/json" \
  -d '{{"a":1,"b":-1}}'</pre>
  <p style="font-size:13px;color:#888;">Keep this key private — it is not recoverable. If you lose it, contact <a href="mailto:rfi.irfos@gmail.com" style="color:#00f5c4;">rfi.irfos@gmail.com</a>.</p>
  <hr style="border-color:#1e2030;margin:24px 0;">
  <p style="font-size:11px;color:#555;">ternlang.com · RFI-IRFOS · BSL-1.1</p>
</body>
</html>"#, key_id = key_id, api_key = api_key);

    let body = json!({
        "from":    "Ternlang API <noreply@ternlang.com>",
        "to":      [to_email],
        "subject": "Your Ternlang API Key — Tier 2 Access",
        "html":    html,
    });

    let client = reqwest::Client::new();
    match client
        .post("https://api.resend.com/emails")
        .bearer_auth(resend_key)
        .json(&body)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() =>
            eprintln!("[stripe] email sent to {}", to_email),
        Ok(r) =>
            eprintln!("[stripe] Resend error {}: {:?}", r.status(), r.text().await),
        Err(e) =>
            eprintln!("[stripe] Resend request failed: {}", e),
    }
}

/// POST /stripe/webhook
async fn stripe_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // 1. Verify signature
    let sig_header = match headers.get("stripe-signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s.to_string(),
        None    => return api_error(StatusCode::BAD_REQUEST, "Missing Stripe-Signature header."),
    };

    if !state.stripe_webhook_secret.is_empty()
        && !verify_stripe_signature(&state.stripe_webhook_secret, &body, &sig_header)
    {
        eprintln!("[stripe] signature verification failed");
        return api_error(StatusCode::UNAUTHORIZED, "Invalid webhook signature.");
    }

    // 2. Parse event
    let event: Value = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => {
            eprintln!("[stripe] JSON parse error: {}", e);
            return api_error(StatusCode::BAD_REQUEST, "Invalid JSON.");
        }
    };

    let event_type = event["type"].as_str().unwrap_or("");
    eprintln!("[stripe] received event: {}", event_type);

    // 3. Handle checkout completed
    if event_type == "checkout.session.completed" {
        let session     = &event["data"]["object"];
        let customer_email = session["customer_details"]["email"]
            .as_str()
            .or_else(|| session["customer_email"].as_str())
            .unwrap_or("");

        if customer_email.is_empty() {
            eprintln!("[stripe] checkout.session.completed: no email found in session");
            return (StatusCode::OK, Json(json!({ "status": "ok_no_email" }))).into_response();
        }

        // 4. Generate key
        let (raw_key, entry) = state.keys.generate(
            2,
            customer_email.to_string(),
            "stripe-auto".to_string(),
        ).await;

        eprintln!("[stripe] provisioned key {} for {}", entry.key_id, customer_email);

        // 5. Email it
        send_key_email(&state.resend_api_key, customer_email, &raw_key, &entry.key_id).await;
    }

    (StatusCode::OK, Json(json!({ "received": true }))).into_response()
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

    let stripe_webhook_secret = env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_else(|_| {
        eprintln!("[ternlang-api] WARNING: STRIPE_WEBHOOK_SECRET not set — webhook signature verification disabled");
        String::new()
    });

    let resend_api_key = env::var("RESEND_API_KEY").unwrap_or_else(|_| {
        eprintln!("[ternlang-api] WARNING: RESEND_API_KEY not set — post-payment emails will not be sent");
        String::new()
    });

    let state = Arc::new(AppState {
        admin_key,
        keys,
        version: "0.1.0",
        stripe_webhook_secret,
        resend_api_key,
        memory_store: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
    });

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        // Public
        .route("/",       get(root))
        .route("/health", get(health))
        .route("/mcp",    get(mcp_info).post(mcp_handler))
        .route("/.well-known/mcp/server-card.json", get(mcp_server_card))
        .route("/stripe/webhook", post(stripe_webhook))
        .route("/pricing",        get(pricing_page))
        .route("/api/usage",      get(api_usage))
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
