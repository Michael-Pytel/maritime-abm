use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simulation::scenario::{Method, Scenario, SimConfig};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::runner::{SimHandle, spawn_batch, spawn_sim};
use crate::stats::{BatchState, compute_hypothesis_tests};

pub struct AppState {
    pub handle: Mutex<Option<SimHandle>>,
    pub batch: Mutex<Option<Arc<std::sync::Mutex<BatchState>>>>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handle: Mutex::new(None),
            batch: Mutex::new(None),
        })
    }
}

// ── Live simulation ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub scenario: Option<String>,
    pub method: Option<String>,
    pub seed: Option<u64>,
    pub n_ticks: Option<u32>,
    pub n_vessels: Option<u32>,
    pub loop_mode: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
    pub status: String,
    pub seed: u64,
}

pub async fn post_start(
    State(app): State<Arc<AppState>>,
    Json(req): Json<StartRequest>,
) -> Result<Json<StartResponse>, (StatusCode, String)> {
    let scenario = match req.scenario.as_deref() {
        Some("StormCorridor") => Scenario::StormCorridor,
        Some("BlindShore") => Scenario::BlindShore,
        Some("DeepWaterRescue") => Scenario::DeepWaterRescue,
        _ => Scenario::CalmPassage,
    };
    let method = match req.method.as_deref() {
        Some("BaselineA") => Method::BaselineA,
        Some("BaselineB") => Method::BaselineB,
        _ => Method::ProposedSystem,
    };
    let seed = req.seed.unwrap_or(42);
    let mut config = SimConfig::for_scenario(scenario, method, seed);

    if let Some(v) = req.n_ticks   { config.n_ticks = v; }
    if let Some(v) = req.n_vessels { config.n_vessels = v; }
    if let Some(v) = req.loop_mode { config.loop_mode = v; }
    config.compute_field_units();

    let handle = spawn_sim(config)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut lock = app.handle.lock().await;
    if let Some(old) = lock.take() {
        let _ = old.stop_tx.send(true);
    }
    *lock = Some(handle);

    Ok(Json(StartResponse { status: "started".into(), seed }))
}

pub async fn post_stop(State(app): State<Arc<AppState>>) -> Json<Value> {
    let mut lock = app.handle.lock().await;
    if let Some(handle) = lock.take() {
        let _ = handle.stop_tx.send(true);
        Json(json!({ "status": "stopped" }))
    } else {
        Json(json!({ "status": "no_run_active" }))
    }
}

pub async fn get_kpi(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.handle.lock().await;
    if let Some(handle) = &*lock {
        if let Ok(kpi) = handle.latest_kpi.lock() {
            return Json(serde_json::to_value(&*kpi).unwrap_or(json!({})));
        }
    }
    Json(json!({ "error": "no_run_active" }))
}

pub async fn get_health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

// ── Batch runs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub n_seeds: Option<u32>,
    pub n_ticks: Option<u32>,
    /// Subset of ["CalmPassage","StormCorridor","BlindShore","DeepWaterRescue"].
    /// Runs all four if absent.
    pub scenarios: Option<Vec<String>>,
    /// Subset of ["BaselineA","BaselineB","ProposedSystem"].
    /// Runs all three if absent.
    pub methods: Option<Vec<String>>,
    /// Directory for tick logs, manifests, and summary CSV. Defaults to "outputs".
    pub output_dir: Option<String>,
}

pub async fn post_batch(
    State(app): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Json<Value> {
    let n_seeds = req.n_seeds.unwrap_or(30).min(100);
    let n_ticks = req.n_ticks.unwrap_or(2880);
    let output_dir = req.output_dir.unwrap_or_else(|| "outputs".into());

    let scenarios: Vec<Scenario> = req.scenarios
        .as_deref()
        .map(|names| {
            names.iter().filter_map(|n| match n.as_str() {
                "CalmPassage"    => Some(Scenario::CalmPassage),
                "StormCorridor"  => Some(Scenario::StormCorridor),
                "BlindShore"     => Some(Scenario::BlindShore),
                "DeepWaterRescue"=> Some(Scenario::DeepWaterRescue),
                _                => None,
            }).collect()
        })
        .unwrap_or_else(|| vec![
            Scenario::CalmPassage,
            Scenario::StormCorridor,
            Scenario::BlindShore,
            Scenario::DeepWaterRescue,
        ]);

    let methods: Vec<Method> = req.methods
        .as_deref()
        .map(|names| {
            names.iter().filter_map(|n| match n.as_str() {
                "BaselineA"      => Some(Method::BaselineA),
                "BaselineB"      => Some(Method::BaselineB),
                "ProposedSystem" => Some(Method::ProposedSystem),
                _                => None,
            }).collect()
        })
        .unwrap_or_else(|| vec![
            Method::BaselineA,
            Method::BaselineB,
            Method::ProposedSystem,
        ]);

    let batch_id = Uuid::new_v4().to_string();

    let batch = spawn_batch(
        n_seeds,
        n_ticks,
        "data/ais_paths.json".into(),
        "data/coastline.geojson".into(),
        output_dir,
        scenarios,
        methods,
        batch_id.clone(),
    );

    let total = batch.lock().map_or(0, |s| s.total);
    let mut lock = app.batch.lock().await;
    *lock = Some(batch);

    Json(json!({ "batch_id": batch_id, "status": "started", "total": total }))
}

pub async fn get_batch_status(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    if let Some(batch) = &*lock {
        if let Ok(state) = batch.lock() {
            return Json(json!({
                "batch_id": state.batch_id,
                "total": state.total,
                "completed": state.completed,
                "failed": state.failed,
                "running": state.running,
                "progress_pct": state.completed as f64 / state.total.max(1) as f64 * 100.0,
            }));
        }
    }
    Json(json!({ "status": "no_batch" }))
}

pub async fn get_batch_results(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(&*lock);
    Json(serde_json::to_value(&results).unwrap_or(json!([])))
}

pub async fn get_batch_stats(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(&*lock);
    let tests = compute_hypothesis_tests(&results);
    Json(serde_json::to_value(&tests).unwrap_or(json!([])))
}

pub async fn get_batch_hypothesis(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(&*lock);
    let tests = compute_hypothesis_tests(&results);
    Json(serde_json::to_value(&tests).unwrap_or(json!([])))
}

// ── Disk-backed result loading ───────────────────────────────────────────────

/// Scans `outputs/*_manifest.json` and reconstructs BatchResult records.
/// Used as a fallback when no in-memory batch is present (e.g. after restart).
fn load_results_from_disk() -> Vec<crate::stats::BatchResult> {
    use simulation::kpi::KpiSnapshot;
    let mut results = Vec::new();
    let Ok(entries) = std::fs::read_dir("outputs") else { return results };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with("_manifest.json") { continue; }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let scenario = v["scenario"].as_str().unwrap_or("").to_string();
        let method   = v["method"].as_str().unwrap_or("").to_string();
        let seed     = v["seed"].as_u64().unwrap_or(0);
        let k = &v["kpis"];
        if scenario.is_empty() || method.is_empty() { continue; }
        let kpi = KpiSnapshot {
            step:                 k["step"].as_u64().unwrap_or(0),
            fatal_per_1k_hrs:     k["fatal_per_1k_hrs"].as_f64().unwrap_or(0.0),
            collision_per_1k_hrs: k["collision_per_1k_hrs"].as_f64().unwrap_or(0.0),
            survival_ratio:       k["survival_ratio"].as_f64().unwrap_or(1.0),
            avg_tta_hours:        k["avg_tta_hours"].as_f64().unwrap_or(0.0),
            evac_activation_rate: k["evac_activation_rate"].as_f64().unwrap_or(0.0),
            mean_p_prep:          k["mean_p_prep"].as_f64().unwrap_or(0.0),
        };
        results.push(crate::stats::BatchResult { scenario, method, seed, kpi });
    }
    results
}

/// Returns results from the in-memory batch if it has data, otherwise from disk.
fn resolve_results(batch_lock: &Option<Arc<std::sync::Mutex<BatchState>>>) -> Vec<crate::stats::BatchResult> {
    if let Some(batch) = batch_lock {
        if let Ok(state) = batch.lock() {
            if !state.results.is_empty() {
                return state.results.clone();
            }
        }
    }
    load_results_from_disk()
}

// ── AIS path data ────────────────────────────────────────────────────────────

pub async fn get_ais_paths() -> (StatusCode, Json<Value>) {
    match std::fs::read_to_string("data/ais_paths.json") {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(data) => (StatusCode::OK, Json(data)),
            Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))),
        },
        Err(_) => (StatusCode::NOT_FOUND, Json(json!({ "error": "data/ais_paths.json not found" }))),
    }
}

// ── Completed-run playback ────────────────────────────────────────────────────

/// Lists all completed runs by scanning `outputs/` for `*_manifest.json` files.
/// Returns an array sorted newest-first (by `completed_at`).
pub async fn get_runs() -> Json<Value> {
    let mut runs: Vec<Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("outputs") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if name.ends_with("_manifest.json") {
                let run_id = name.trim_end_matches("_manifest.json").to_string();
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(mut manifest) = serde_json::from_str::<Value>(&text) {
                        manifest["run_id"] = json!(run_id);
                        // Tell the frontend whether a playback log exists for this run.
                        let log_path = format!("outputs/{run_id}.jsonl");
                        manifest["has_log"] = json!(std::path::Path::new(&log_path).exists());
                        runs.push(manifest);
                    }
                }
            }
        }
    }
    runs.sort_by(|a, b| {
        b["completed_at"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["completed_at"].as_u64().unwrap_or(0))
    });
    Json(json!(runs))
}

/// Returns the raw JSONL tick log for a completed run.
pub async fn get_run_log(Path(run_id): Path<String>) -> (StatusCode, String) {
    match std::fs::read_to_string(format!("outputs/{run_id}.jsonl")) {
        Ok(content) => (StatusCode::OK, content),
        Err(_) => (StatusCode::NOT_FOUND, String::new()),
    }
}

/// Returns the manifest JSON for a completed run.
pub async fn get_run_manifest(Path(run_id): Path<String>) -> Json<Value> {
    match std::fs::read_to_string(format!("outputs/{run_id}_manifest.json")) {
        Ok(text) => Json(serde_json::from_str(&text).unwrap_or(json!({}))),
        Err(_) => Json(json!({ "error": "not_found" })),
    }
}
