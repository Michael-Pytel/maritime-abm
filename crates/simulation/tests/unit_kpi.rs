use simulation::kpi::{KpiAccumulator, RescueRecord};
use simulation::human::CrewArchetype;
use simulation::vessel::{VesselAgent, VesselState};

fn active_vessel(p_prep: f64) -> VesselAgent {
    VesselAgent {
        id: 1,
        name: "T".into(),
        vessel_type: "cargo".into(),
        route: vec![],
        route_index: 0,
        route_direction: 1,
        position: (0.0, 0.0),
        last_valid_position: (0.0, 0.0),
        heading_deg: 0.0,
        speed_kn: 15.0,
        state: VesselState::Active,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep,
        shore_age_ticks: 0,
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore: 0.0,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

#[test]
fn fatal_per_1k_hrs_formula() {
    let mut acc = KpiAccumulator::new();
    acc.ship_hrs = 500.0;
    acc.fatal_events = 3;
    let snap = acc.snapshot(0);
    let expected = 3.0 / 500.0 * 1000.0;
    assert!((snap.fatal_per_1k_hrs - expected).abs() < 1e-9);
}

#[test]
fn collision_per_1k_hrs_formula() {
    let mut acc = KpiAccumulator::new();
    acc.ship_hrs = 2000.0;
    acc.collision_events = 4;
    let snap = acc.snapshot(0);
    let expected = 4.0 / 2000.0 * 1000.0;
    assert!((snap.collision_per_1k_hrs - expected).abs() < 1e-9);
}

#[test]
fn survival_ratio_formula() {
    let mut acc = KpiAccumulator::new();
    acc.survivor_count = 7;
    acc.fatal_events   = 3;
    let snap = acc.snapshot(0);
    assert!((snap.survival_ratio - 0.7).abs() < 1e-9);
}

#[test]
fn survival_ratio_one_when_no_outcomes() {
    let acc = KpiAccumulator::new();
    let snap = acc.snapshot(0);
    assert!((snap.survival_ratio - 1.0).abs() < 1e-9);
}

#[test]
fn avg_tta_hours_formula() {
    let mut acc = KpiAccumulator::new();
    acc.tta_samples = vec![2.0, 4.0, 6.0];
    let snap = acc.snapshot(0);
    assert!((snap.avg_tta_hours - 4.0).abs() < 1e-9);
}

#[test]
fn avg_tta_zero_when_empty() {
    let acc = KpiAccumulator::new();
    let snap = acc.snapshot(0);
    assert!((snap.avg_tta_hours - 0.0).abs() < 1e-9);
}

#[test]
fn evac_activation_rate_formula() {
    let mut acc = KpiAccumulator::new();
    acc.total_spawns     = 10;
    acc.evac_activations = 3;
    let snap = acc.snapshot(0);
    assert!((snap.evac_activation_rate - 0.3).abs() < 1e-9);
}

#[test]
fn evac_activation_rate_zero_when_no_spawns() {
    let acc = KpiAccumulator::new();
    let snap = acc.snapshot(0);
    assert!((snap.evac_activation_rate - 0.0).abs() < 1e-9);
}

#[test]
fn mean_p_prep_formula() {
    let mut acc = KpiAccumulator::new();
    acc.p_prep_sum   = 1.5;
    acc.p_prep_count = 3;
    let snap = acc.snapshot(0);
    assert!((snap.mean_p_prep - 0.5).abs() < 1e-9);
}

#[test]
fn mean_p_prep_zero_when_no_samples() {
    let acc = KpiAccumulator::new();
    let snap = acc.snapshot(0);
    assert!((snap.mean_p_prep - 0.0).abs() < 1e-9);
}

#[test]
fn record_tick_accumulates_ship_hrs_and_p_prep() {
    let mut acc = KpiAccumulator::new();
    let vessels = vec![active_vessel(0.8)];
    acc.record_tick(&vessels, &[]);
    // One active vessel × Δt=0.25 hr
    assert!((acc.ship_hrs - 0.25).abs() < 1e-12);
    assert!((acc.p_prep_sum - 0.8).abs() < 1e-12);
    assert_eq!(acc.p_prep_count, 1);
}

#[test]
fn record_tick_rescue_record_updates_tta_and_survivors() {
    let mut acc = KpiAccumulator::new();
    let rec = RescueRecord { vessel_id: 1, tta_hours: 3.0, survivors: 8, fatalities: 2 };
    acc.record_tick(&[], &[rec]);
    assert_eq!(acc.tta_samples, vec![3.0]);
    assert_eq!(acc.survivor_count, 8);
    assert_eq!(acc.fatal_events, 2);
}

#[test]
fn record_collision_increments_counter() {
    let mut acc = KpiAccumulator::new();
    acc.record_collision();
    acc.record_collision();
    assert_eq!(acc.collision_events, 2);
}

#[test]
fn snapshot_step_field() {
    let acc = KpiAccumulator::new();
    let snap = acc.snapshot(99);
    assert_eq!(snap.step, 99);
}
