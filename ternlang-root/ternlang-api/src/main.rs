/// ternlang-api — REST HTTP server for the Ternary Intelligence Stack
///
/// Powers ternlang.com/api
///
/// Routes:
///   GET  /                      — API info + available endpoints
///   GET  /health                — health check (no auth)
///   POST /api/trit_decide       — scalar ternary decision
///   POST /api/trit_vector       — multi-dimensional evidence aggregation
///   POST /api/trit_consensus    — consensus(a, b)
///   POST /api/quantize_weights  — BitNet f32 → ternary
///   POST /api/sparse_benchmark  — sparse vs dense matmul stats
///
/// Auth: X-Ternlang-Key header (set via TERNLANG_API_KEY env var)
/// CORS: open (for browser playground)
///
/// Run:
///   TERNLANG_API_KEY=your-key cargo run --release --bin ternlang-api
///   TERNLANG_API_KEY=your-key PORT=8080 cargo run --release --bin ternlang-api

use axum::{
    Router,
    Json,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{json, Value};
use std::{env, sync::Arc};
use tower_http::cors::{Any, CorsLayer};

use ternlang_core::trit::Trit;
use ternlang_ml::{
    TritScalar, TritEvidenceVec, TEND_BOUNDARY,
    bitnet_threshold, benchmark, dense_matmul, sparse_matmul, TritMatrix,
};

// ─── App state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    api_key: String,
    version: &'static str,
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

// ─── Auth middleware ──────────────────────────────────────────────────────────

async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Public endpoints — no key required
    if path == "/" || path == "/health" {
        return next.run(request).await;
    }

    let provided = headers
        .get("X-Ternlang-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided.is_empty() {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "Missing X-Ternlang-Key header. Get a key at https://ternlang.com/api",
        );
    }

    if provided != state.api_key {
        return api_error(StatusCode::UNAUTHORIZED, "Invalid API key.");
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
            "POST /api/trit_decide":      "Scalar ternary decision: evidence[] → reject/tend/affirm + confidence",
            "POST /api/trit_vector":      "Multi-dimensional evidence: named dimensions + weights → aggregate",
            "POST /api/trit_consensus":   "consensus(a, b) → ternary result",
            "POST /api/quantize_weights": "f32[] → ternary weights via BitNet threshold",
            "POST /api/sparse_benchmark": "Sparse vs dense matmul performance stats",
        }
    }))
}

// ─── GET /health ─────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "engine": "BET VM", "trit": 1 }))
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

    let recommendation = match agg.trit() {
        Trit::PosOne => format!(
            "Affirm — scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Act." } else { ". Below threshold — gather more evidence." }
        ),
        Trit::NegOne => format!(
            "Reject — scalar {:.3}, confidence {:.0}%{}.",
            agg.raw(), agg.confidence() * 100.0,
            if actionable { ". Do not act." } else { ". Below threshold — gather more evidence." }
        ),
        Trit::Zero => format!(
            "Tend — scalar {:.3} in deliberation zone. Strongest signal: {}.",
            agg.raw(),
            ev.dominant().map(|(l, _)| l).unwrap_or("none")
        ),
    };

    (StatusCode::OK, Json(json!({
        "aggregate": {
            "scalar":        (agg.raw() * 1000.0).round() / 1000.0,
            "trit":          trit_to_i8(agg.trit()),
            "label":         agg.label(),
            "confidence":    (agg.confidence() * 1000.0).round() / 1000.0,
            "is_actionable": actionable,
        },
        "breakdown":      breakdown,
        "dominant":       dominant,
        "tend_boundary":  TEND_BOUNDARY,
        "recommendation": recommendation,
    }))).into_response()
}

// ─── POST /api/trit_consensus ────────────────────────────────────────────────

async fn trit_consensus(Json(body): Json<Value>) -> Response {
    let a_val = match body["a"].as_i64() {
        Some(v) => v,
        None => return api_error(StatusCode::BAD_REQUEST, "a must be -1, 0, or 1"),
    };
    let b_val = match body["b"].as_i64() {
        Some(v) => v,
        None => return api_error(StatusCode::BAD_REQUEST, "b must be -1, 0, or 1"),
    };

    let a = match i8_to_trit(a_val) {
        Some(t) => t,
        None => return api_error(StatusCode::BAD_REQUEST, "a must be -1, 0, or 1"),
    };
    let b = match i8_to_trit(b_val) {
        Some(t) => t,
        None => return api_error(StatusCode::BAD_REQUEST, "b must be -1, 0, or 1"),
    };

    let (sum, carry) = a + b;
    let s = TritScalar::new(trit_to_i8(sum) as f32);

    (StatusCode::OK, Json(json!({
        "result":     trit_to_i8(sum),
        "label":      s.label(),
        "carry":      trit_to_i8(carry),
        "expression": format!("consensus({}, {}) = {}", a_val, b_val, trit_to_i8(sum)),
    }))).into_response()
}

// ─── POST /api/quantize_weights ──────────────────────────────────────────────

async fn quantize_weights(Json(body): Json<Value>) -> Response {
    let weights: Vec<f32> = match body["weights"].as_array() {
        Some(arr) => match arr.iter()
            .map(|v| v.as_f64().map(|f| f as f32).ok_or(()))
            .collect::<Result<Vec<_>, _>>() {
                Ok(v) => v,
                Err(_) => return api_error(StatusCode::BAD_REQUEST, "weight values must be numbers"),
            },
        None => return api_error(StatusCode::BAD_REQUEST, "weights must be an array"),
    };

    if weights.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "weights cannot be empty");
    }

    let threshold = body["threshold"].as_f64()
        .unwrap_or_else(|| bitnet_threshold(&weights) as f64) as f32;

    let trits: Vec<i8> = weights.iter().map(|&w| {
        if w > threshold { 1 } else if w < -threshold { -1 } else { 0 }
    }).collect();

    let zeros = trits.iter().filter(|&&t| t == 0).count();
    let sparsity = zeros as f64 / trits.len() as f64;

    (StatusCode::OK, Json(json!({
        "trits":           trits,
        "threshold_used":  threshold,
        "sparsity":        sparsity,
        "zero_count":      zeros,
        "nonzero_count":   trits.len() - zeros,
        "total":           trits.len(),
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

// ─── 404 fallback ─────────────────────────────────────────────────────────────

async fn not_found() -> Response {
    api_error(StatusCode::NOT_FOUND, "Endpoint not found. See GET / for available routes.")
}

// ─── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let api_key = env::var("TERNLANG_API_KEY").unwrap_or_else(|_| {
        eprintln!("[ternlang-api] WARNING: TERNLANG_API_KEY not set — using 'dev-key'");
        eprintln!("[ternlang-api] Set TERNLANG_API_KEY=<your-key> in production");
        "dev-key".to_string()
    });

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3731);   // 3731 — ternary 🙂

    let state = Arc::new(AppState { api_key, version: "0.1.0" });

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        .route("/",                      get(root))
        .route("/health",                get(health))
        .route("/api/trit_decide",       post(trit_decide))
        .route("/api/trit_vector",       post(trit_vector))
        .route("/api/trit_consensus",    post(trit_consensus))
        .route("/api/quantize_weights",  post(quantize_weights))
        .route("/api/sparse_benchmark",  post(sparse_benchmark))
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(state.clone(), require_api_key))
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("[ternlang-api] listening on http://{}", addr);
    eprintln!("[ternlang-api] docs: https://ternlang.com/docs/api");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
