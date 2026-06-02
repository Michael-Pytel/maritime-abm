/// Scenario 1 calibration guard (Section 5.2 of the report).
///
/// Run with: cargo test --test calibration -- --include-ignored
use simulation::{
    scenario::{Method, Scenario, SimConfig},
    state::SimStateWrapper,
};

fn cfg_calm_baseline_a(seed: u64) -> SimConfig {
    let mut cfg = SimConfig::for_scenario(Scenario::CalmPassage, Method::BaselineA, seed);
    cfg.ais_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ais_paths.json").into();
    cfg.ports_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ports.json").into();
    cfg.land_mask_path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/coastline.geojson").into();
    cfg.n_ticks = 2880;
    cfg
}

/// Scenario 1 (CalmPassage / BaselineA) calibration target from report Section 5.2:
/// mean collision_per_1k_hrs < 0.5 across 5 seeds.
#[test]
#[ignore = "slow calibration run (5 seeds × 2880 ticks); run with --include-ignored"]
fn test_scenario1_collision_calibration() {
    let seeds = [42u64, 7, 13, 99, 1234];
    let mut total_collision_rate = 0.0f64;

    for seed in seeds {
        let mut wrapper = SimStateWrapper::new(cfg_calm_baseline_a(seed))
            .expect("failed to init SimState");
        wrapper.run_blocking();
        let kpi = wrapper.inner.kpi_snapshot();
        total_collision_rate += kpi.collision_per_1k_hrs;
    }

    let mean_collision_per_1k_hrs = total_collision_rate / seeds.len() as f64;
    assert!(
        mean_collision_per_1k_hrs < 0.5,
        "calibration failed: {mean_collision_per_1k_hrs:.3} >= 0.5 collisions/1k hrs \
         (report Section 5.2 target for Scenario 1 / BaselineA)"
    );
}

/// Quick smoke-test version using 200 ticks to verify the simulation runs without panic.
/// Not a calibration target — just ensures the code path is exercised in regular CI.
#[test]
fn test_scenario1_calibration_smoke() {
    let mut cfg = cfg_calm_baseline_a(42);
    cfg.n_ticks = 200;
    let mut wrapper = SimStateWrapper::new(cfg).expect("failed to init SimState");
    wrapper.run_blocking();
    let kpi = wrapper.inner.kpi_snapshot();
    assert_eq!(kpi.step, 200);
    assert!(kpi.collision_per_1k_hrs >= 0.0);
    assert!(kpi.survival_ratio >= 0.0 && kpi.survival_ratio <= 1.0);
}
