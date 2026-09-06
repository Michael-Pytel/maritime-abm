use rayon::prelude::*;
use simulation::{
    iwrap,
    scenario::{Method, Scenario, SimConfig},
    state::SimStateWrapper,
};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::db::{self, RunRecord};
use crate::stats::{BatchResult, BatchState};

/// Method-dependent speed scale for the offline IWRAP `N_c` stamp.
fn iwrap_speed_scale(method: Method) -> f64 {
    match method {
        Method::BaselineA => 1.0,
        Method::BaselineB => 0.85,
        Method::ProposedSystem => 0.78,
    }
}

fn publish_status(tx: &broadcast::Sender<String>, batch: &Mutex<BatchState>) {
    if let Ok(state) = batch.lock() {
        let _ = tx.send(state.status_json().to_string());
    }
}

#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub fn spawn_batch(
    n_seeds: u32,
    n_ticks: Option<u32>,
    ais_path: String,
    land_mask_path: String,
    output_dir: String,
    scenarios: Vec<Scenario>,
    methods: Vec<Method>,
    batch_id: String,
    // Partial `SimConfig` override applied to every run in this batch (the
    // swept hyperparameters); empty object when none.
    config_override: serde_json::Value,
    // Auto-derived sweep-point label stored with each run.
    label: String,
    status_tx: broadcast::Sender<String>,
) -> Arc<Mutex<BatchState>> {
    let seeds: Vec<u64> = (1..=u64::from(n_seeds)).collect();

    let runs: Vec<(Scenario, Method, u64)> = scenarios
        .iter()
        .copied()
        .flat_map(|s| {
            let seeds = seeds.clone();
            let methods_clone = methods.clone();
            methods_clone.into_iter().flat_map(move |m| {
                let seeds = seeds.clone();
                seeds.into_iter().map(move |seed| (s, m, seed))
            })
        })
        .collect();

    let total = runs.len();
    let batch = Arc::new(Mutex::new(BatchState {
        batch_id,
        total,
        completed: 0,
        failed: 0,
        running: true,
        results: Vec::new(),
    }));
    let batch2 = Arc::clone(&batch);
    publish_status(&status_tx, &batch2);

    let params_json = serde_json::to_string(&config_override).unwrap_or_else(|_| "{}".into());

    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&output_dir).ok();

        // Latest-only playback tier: drop the previous batch's heavy tick logs and
        // manifests so disk doesn't grow ~7 GB per sweep point. The results tier
        // (SQLite, below) is append-only and keeps every batch for comparison.
        clear_playback_artifacts(&output_dir);

        // SQLite is the operational store: one row per run, keyed by batch_id.
        // Shared behind a Mutex so the parallel workers serialise their writes.
        let conn = db::open(&format!("{output_dir}/results.db"))
            .map(|c| Arc::new(Mutex::new(c)))
            .ok();

        // summary.csv mirrors the playback tier (latest batch only); the DB is
        // the retained store. Recreated each batch so it never mixes sweeps.
        let csv_path = format!("{output_dir}/summary.csv");
        if let Ok(mut f) = std::fs::File::create(&csv_path) {
            let _ = writeln!(
                f,
                "scenario,method,seed,collision_per_1k_hrs,iwrap_nc_per_year"
            );
        }

        let csv_mutex: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        let batch_id = batch2
            .lock()
            .map_or_else(|_| String::new(), |s| s.batch_id.clone());
        let override_obj = config_override.as_object().filter(|o| !o.is_empty());

        runs.into_par_iter().for_each(|(scenario, method, seed)| {
            let mut cfg = SimConfig::for_scenario(scenario, method, seed);
            cfg.ais_path.clone_from(&ais_path);
            cfg.land_mask_path.clone_from(&land_mask_path);
            if let Some(n) = n_ticks {
                cfg.n_ticks = n;
            }
            cfg.snapshot_every_n_ticks = 0;

            // Apply the swept parameter override (deep-merge over the scenario
            // defaults), then re-pin the experiment identity so it can't drift.
            if override_obj.is_some() {
                if let Ok(mut v) = serde_json::to_value(&cfg) {
                    db::deep_merge(&mut v, &config_override);
                    if let Ok(merged) = serde_json::from_value::<SimConfig>(v) {
                        cfg = merged;
                    }
                }
                cfg.scenario = scenario;
                cfg.method = method;
                cfg.seed = seed;
                cfg.compute_field_units();
            }

            let n_vessels = cfg.n_vessels;
            let run_n_ticks = cfg.n_ticks;
            let run_ais = cfg.ais_path.clone();
            let run_id = format!(
                "{}_{}_{seed}",
                format!("{scenario:?}").to_lowercase(),
                format!("{method:?}").to_lowercase()
            );

            let log_path = format!("{output_dir}/{run_id}.jsonl");
            cfg.log_path = Some(log_path.clone());
            cfg.log_every_n_ticks = 10;

            if let Ok(mut wrapper) = SimStateWrapper::new(cfg) {
                wrapper.run_blocking();
                let kpi = wrapper.inner.kpi_snapshot();

                let iwrap_nc = iwrap::estimate_nc_from_ais_path(
                    &run_ais,
                    n_vessels,
                    iwrap_speed_scale(method),
                )
                .unwrap_or(0.0);

                let completed_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());

                let manifest = serde_json::json!({
                    "scenario": format!("{scenario:?}"),
                    "method": format!("{method:?}"),
                    "seed": seed,
                    "n_ticks": run_n_ticks,
                    "n_vessels": n_vessels,
                    "completed_at": completed_at,
                    "kpis": kpi,
                    "iwrap_nc_per_year": iwrap_nc,
                });
                if let Ok(content) = serde_json::to_string_pretty(&manifest) {
                    std::fs::write(format!("{output_dir}/{run_id}_manifest.json"), content).ok();
                }

                if let Some(conn) = &conn {
                    let record = RunRecord {
                        batch_id: batch_id.clone(),
                        label: label.clone(),
                        params: params_json.clone(),
                        scenario: format!("{scenario:?}"),
                        method: format!("{method:?}"),
                        seed,
                        n_ticks: run_n_ticks,
                        n_vessels,
                        completed_at,
                        kpi: kpi.clone(),
                        iwrap_nc_per_year: iwrap_nc,
                        log_path: log_path.clone(),
                    };
                    if let Ok(c) = conn.lock() {
                        let _ = db::insert_run(&c, &record);
                    }
                }

                let csv_row = format!(
                    "{scenario:?},{method:?},{seed},{:.6},{:.6}\n",
                    kpi.collision_per_1k_hrs, iwrap_nc,
                );
                if let Ok(_guard) = csv_mutex.lock() {
                    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&csv_path) {
                        let _ = f.write_all(csv_row.as_bytes());
                    }
                }

                let result = BatchResult {
                    scenario: format!("{scenario:?}"),
                    method: format!("{method:?}"),
                    seed,
                    kpi,
                    iwrap_nc_per_year: iwrap_nc,
                };
                if let Ok(mut lock) = batch2.lock() {
                    lock.completed += 1;
                    lock.results.push(result);
                }
                publish_status(&status_tx, &batch2);
            } else if let Ok(mut lock) = batch2.lock() {
                lock.failed += 1;
                drop(lock);
                publish_status(&status_tx, &batch2);
            }
        });

        if let Ok(mut lock) = batch2.lock() {
            lock.running = false;
        }
        publish_status(&status_tx, &batch2);
    });

    batch
}

/// Removes the previous batch's playback artifacts (`*.jsonl` tick logs and
/// `*_manifest.json` files) from `output_dir`. Results in SQLite/Parquet and
/// any exported `*.parquet` files are deliberately left untouched.
fn clear_playback_artifacts(output_dir: &str) {
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_jsonl = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"));
        let is_manifest = name.to_ascii_lowercase().ends_with("_manifest.json");
        if is_jsonl || is_manifest {
            let _ = std::fs::remove_file(&path);
        }
    }
}
