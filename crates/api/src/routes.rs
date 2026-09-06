use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json, Response,
    },
};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use simulation::scenario::{Method, Scenario};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::runner::spawn_batch;
use crate::stats::{compute_hypothesis_tests, BatchState};

pub struct AppState {
    pub batch: Mutex<Option<Arc<std::sync::Mutex<BatchState>>>>,
    /// Fan-out channel for batch progress SSE (`/sim/batch/events`).
    pub status_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let (status_tx, _) = broadcast::channel(64);
        Arc::new(Self {
            batch: Mutex::new(None),
            status_tx,
        })
    }
}

pub async fn get_health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

// ── Batch runs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub n_seeds: Option<u32>,
    pub n_ticks: Option<u32>,
    /// Subset of [`CalmPassage`, `StormCorridor`, `BlindShore`, `DeepWaterRescue`].
    /// Runs all four if absent.
    pub scenarios: Option<Vec<String>>,
    /// Subset of [`BaselineA`, `BaselineB`, `ProposedSystem`].
    /// Runs all three if absent.
    pub methods: Option<Vec<String>>,
    /// Directory for tick logs, manifests, and summary CSV. Defaults to "outputs".
    pub output_dir: Option<String>,
    /// Partial `SimConfig` override (the swept hyperparameters) applied to every
    /// run in this batch — any field, including nested `simcol`/`sar`/`forcefield`.
    /// Deep-merged over the scenario defaults; experiment identity
    /// (scenario/method/seed) is always re-pinned and cannot be overridden.
    pub config_override: Option<Value>,
}

pub async fn post_batch(
    State(app): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Json<Value> {
    let n_seeds = req.n_seeds.unwrap_or(30).min(100);
    let n_ticks = req.n_ticks.unwrap_or(2880);
    let output_dir = req.output_dir.unwrap_or_else(|| "outputs".into());

    let scenarios: Vec<Scenario> = req.scenarios.as_deref().map_or_else(
        || {
            vec![
                Scenario::CalmPassage,
                Scenario::StormCorridor,
                Scenario::BlindShore,
                Scenario::DeepWaterRescue,
            ]
        },
        |names| {
            names
                .iter()
                .filter_map(|n| match n.as_str() {
                    "CalmPassage" => Some(Scenario::CalmPassage),
                    "StormCorridor" => Some(Scenario::StormCorridor),
                    "BlindShore" => Some(Scenario::BlindShore),
                    "DeepWaterRescue" => Some(Scenario::DeepWaterRescue),
                    _ => None,
                })
                .collect()
        },
    );

    let methods: Vec<Method> = req.methods.as_deref().map_or_else(
        || vec![Method::BaselineA, Method::BaselineB, Method::ProposedSystem],
        |names| {
            names
                .iter()
                .filter_map(|n| match n.as_str() {
                    "BaselineA" => Some(Method::BaselineA),
                    "BaselineB" => Some(Method::BaselineB),
                    "ProposedSystem" => Some(Method::ProposedSystem),
                    _ => None,
                })
                .collect()
        },
    );

    let batch_id = Uuid::new_v4().to_string();

    // Normalise the override to an object and auto-derive the sweep-point label.
    let config_override = match req.config_override {
        Some(Value::Object(map)) => Value::Object(map),
        _ => json!({}),
    };
    let label = crate::db::derive_label(&config_override);

    let batch = spawn_batch(
        n_seeds,
        n_ticks,
        "data/ais_paths.json".into(),
        "data/coastline.geojson".into(),
        output_dir,
        scenarios,
        methods,
        batch_id.clone(),
        config_override,
        label.clone(),
        app.status_tx.clone(),
    );

    let total = batch.lock().map_or(0, |s| s.total);
    let mut lock = app.batch.lock().await;
    *lock = Some(batch);

    Json(json!({ "batch_id": batch_id, "status": "started", "total": total, "label": label }))
}

pub async fn get_batch_status(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    if let Some(batch) = &*lock {
        if let Ok(state) = batch.lock() {
            return Json(state.status_json());
        }
    }
    Json(json!({ "status": "no_batch" }))
}

/// Server-Sent Events stream of batch progress (replaces 2.5 s frontend polling).
///
/// Emits the same JSON shape as `GET /sim/batch/status` whenever a run completes
/// or the batch finishes. Sends the current snapshot immediately on connect.
pub async fn get_batch_events(
    State(app): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = app.status_tx.subscribe();

    // Snapshot current status so a late subscriber sees progress immediately.
    let initial = {
        let lock = app.batch.lock().await;
        lock.as_ref()
            .and_then(|b| b.lock().ok().map(|s| s.status_json().to_string()))
            .unwrap_or_else(|| json!({ "status": "no_batch", "running": false }).to_string())
    };
    let _ = app.status_tx.send(initial);

    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(data) => Some(Ok(Event::default().data(data))),
        Err(BroadcastStreamRecvError::Lagged(_)) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

pub async fn get_batch_results(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(lock.as_ref());
    Json(serde_json::to_value(&results).unwrap_or(json!([])))
}

pub async fn get_batch_stats(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(lock.as_ref());
    let tests = compute_hypothesis_tests(&results);
    Json(serde_json::to_value(&tests).unwrap_or(json!([])))
}

pub async fn get_batch_hypothesis(State(app): State<Arc<AppState>>) -> Json<Value> {
    let lock = app.batch.lock().await;
    let results = resolve_results(lock.as_ref());
    let tests = compute_hypothesis_tests(&results);
    Json(serde_json::to_value(&tests).unwrap_or(json!([])))
}

// ── Disk-backed result loading ───────────────────────────────────────────────

/// Scans `outputs/*_manifest.json` and reconstructs `BatchResult` records.
/// Used as a fallback when no in-memory batch is present (e.g. after restart).
fn load_results_from_disk() -> Vec<crate::stats::BatchResult> {
    use simulation::kpi::KpiSnapshot;
    let mut results = Vec::new();
    let Ok(entries) = std::fs::read_dir("outputs") else {
        return results;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with("_manifest.json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let scenario = v["scenario"].as_str().unwrap_or("").to_string();
        let method = v["method"].as_str().unwrap_or("").to_string();
        let seed = v["seed"].as_u64().unwrap_or(0);
        let k = &v["kpis"];
        if scenario.is_empty() || method.is_empty() {
            continue;
        }
        let kpi = KpiSnapshot {
            step: k["step"].as_u64().unwrap_or(0),
            fatal_per_1k_hrs: k["fatal_per_1k_hrs"].as_f64().unwrap_or(0.0),
            collision_per_1k_hrs: k["collision_per_1k_hrs"].as_f64().unwrap_or(0.0),
            survival_ratio: k["survival_ratio"].as_f64().unwrap_or(1.0),
            avg_tta_hours: k["avg_tta_hours"].as_f64().unwrap_or(0.0),
            evac_activation_rate: k["evac_activation_rate"].as_f64().unwrap_or(0.0),
            mean_p_prep: k["mean_p_prep"].as_f64().unwrap_or(0.0),
        };
        results.push(crate::stats::BatchResult {
            scenario,
            method,
            seed,
            kpi,
            iwrap_nc_per_year: v["iwrap_nc_per_year"].as_f64().unwrap_or(0.0),
        });
    }
    results
}

/// Resolves batch results, scoped to a single batch so runs never mix.
///
/// Prefers the `SQLite` store (queried by the active batch's id, or the most
/// recent batch after a restart). Falls back to the in-memory vec and then to
/// legacy on-disk manifests for batches produced before the DB existed.
fn resolve_results(
    batch_lock: Option<&Arc<std::sync::Mutex<BatchState>>>,
) -> Vec<crate::stats::BatchResult> {
    let batch_id = batch_lock
        .and_then(|b| b.lock().ok().map(|s| s.batch_id.clone()))
        .filter(|id| !id.is_empty());

    let from_db = crate::db::results(batch_id.as_deref());
    if !from_db.is_empty() {
        return from_db;
    }

    if let Some(batch) = batch_lock {
        if let Ok(state) = batch.lock() {
            if !state.results.is_empty() {
                return state.results.clone();
            }
        }
    }
    load_results_from_disk()
}

// ── Parquet export (for offline analysis) ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// Explicit batch to export; defaults to the active/most-recent batch.
    pub batch_id: Option<String>,
    /// Export every batch (the whole hyperparameter sweep) as one table.
    pub all: Option<bool>,
}

/// Streams KPI rows as a Parquet file for downstream analysis
/// (pandas / polars / R). `all=true` exports the whole sweep; otherwise the
/// requested `batch_id`, else the active batch, else the most recent one.
pub async fn get_batch_export(
    State(app): State<Arc<AppState>>,
    Query(q): Query<ExportQuery>,
) -> Result<Response, (StatusCode, String)> {
    let all = q.all.unwrap_or(false);
    let batch_id = q.batch_id.filter(|id| !id.is_empty()).or_else(|| {
        // No explicit id: fall back to the active batch if one is loaded.
        app.batch
            .try_lock()
            .ok()
            .and_then(|lock| {
                lock.as_ref()
                    .and_then(|b| b.lock().ok().map(|s| s.batch_id.clone()))
            })
            .filter(|id| !id.is_empty())
    });

    match crate::db::export_parquet(batch_id.as_deref(), all) {
        Ok((filename, bytes)) => Ok((
            [
                (
                    header::CONTENT_TYPE,
                    "application/vnd.apache.parquet".to_string(),
                ),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                ),
            ],
            bytes,
        )
            .into_response()),
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

// ── AIS path data ────────────────────────────────────────────────────────────

pub async fn get_ais_paths() -> (StatusCode, Json<Value>) {
    match std::fs::read_to_string("data/ais_paths.json") {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(data) => (StatusCode::OK, Json(data)),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ),
        },
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "data/ais_paths.json not found" })),
        ),
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
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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
