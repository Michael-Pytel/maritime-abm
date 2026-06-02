use anyhow::Result;
use rayon::prelude::*;
use simulation::{
    kpi::KpiSnapshot,
    scenario::{Method, Scenario, SimConfig},
    state::SimStateWrapper,
};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

use crate::stats::{BatchResult, BatchState};

pub struct SimHandle {
    pub snapshot_rx: broadcast::Receiver<String>,
    pub stop_tx: watch::Sender<bool>,
    pub latest_kpi: Arc<Mutex<KpiSnapshot>>,
}

pub fn spawn_sim(config: SimConfig) -> Result<SimHandle> {
    let (snapshot_tx, snapshot_rx) = broadcast::channel::<String>(256);
    let (stop_tx, stop_rx) = watch::channel(false);

    let latest_kpi: Arc<Mutex<KpiSnapshot>> = Arc::new(Mutex::new(KpiSnapshot {
        step: 0,
        fatal_per_1k_hrs: 0.0,
        collision_per_1k_hrs: 0.0,
        survival_ratio: 1.0,
        avg_tta_hours: 0.0,
        evac_activation_rate: 0.0,
        mean_p_prep: 0.0,
    }));
    let kpi_clone = Arc::clone(&latest_kpi);

    let mut wrapper = SimStateWrapper::new(config)?;
    wrapper.inner.snapshot_tx = Some(snapshot_tx);
    wrapper.inner.stop_rx = Some(stop_rx);

    tokio::task::spawn_blocking(move || {
        wrapper.run_blocking();
        if let Ok(mut lock) = kpi_clone.lock() {
            *lock = wrapper.inner.kpi_snapshot();
        }
    });

    Ok(SimHandle {
        snapshot_rx,
        stop_tx,
        latest_kpi,
    })
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
pub fn spawn_batch(
    n_seeds: u32,
    n_ticks: u32,
    ais_path: String,
    land_mask_path: String,
    output_dir: String,
    scenarios: Vec<Scenario>,
    methods: Vec<Method>,
    batch_id: String,
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

    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&output_dir).ok();

        let csv_path = format!("{output_dir}/summary.csv");
        if !std::path::Path::new(&csv_path).exists() {
            if let Ok(mut f) = std::fs::File::create(&csv_path) {
                let _ = writeln!(f, "scenario,method,seed,collision_per_1k_hrs");
            }
        }

        let csv_mutex: Arc<Mutex<()>> = Arc::new(Mutex::new(()));

        runs.into_par_iter().for_each(|(scenario, method, seed)| {
            let mut cfg = SimConfig::for_scenario(scenario, method, seed);
            cfg.ais_path.clone_from(&ais_path);
            cfg.land_mask_path.clone_from(&land_mask_path);
            cfg.n_ticks = n_ticks;
            cfg.snapshot_every_n_ticks = 0; // no live WS broadcasting during batch

            let n_vessels = cfg.n_vessels;
            let run_id = format!(
                "{}_{}_{seed}",
                format!("{scenario:?}").to_lowercase(),
                format!("{method:?}").to_lowercase()
            );

            cfg.log_path = Some(format!("{output_dir}/{run_id}.jsonl"));
            cfg.log_every_n_ticks = 10; // one frame every 10 ticks keeps files ~2 MB

            if let Ok(mut wrapper) = SimStateWrapper::new(cfg) {
                wrapper.run_blocking();
                let kpi = wrapper.inner.kpi_snapshot();

                let completed_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());

                let manifest = serde_json::json!({
                    "scenario": format!("{scenario:?}"),
                    "method": format!("{method:?}"),
                    "seed": seed,
                    "n_ticks": n_ticks,
                    "n_vessels": n_vessels,
                    "completed_at": completed_at,
                    "kpis": kpi,
                });
                if let Ok(content) = serde_json::to_string_pretty(&manifest) {
                    std::fs::write(format!("{output_dir}/{run_id}_manifest.json"), content).ok();
                }

                let csv_row = format!(
                    "{scenario:?},{method:?},{seed},{:.6}\n",
                    kpi.collision_per_1k_hrs,
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
                };
                if let Ok(mut lock) = batch2.lock() {
                    lock.completed += 1;
                    lock.results.push(result);
                }
            } else if let Ok(mut lock) = batch2.lock() {
                lock.failed += 1;
            }
        });

        if let Ok(mut lock) = batch2.lock() {
            lock.running = false;
        }
    });

    batch
}
