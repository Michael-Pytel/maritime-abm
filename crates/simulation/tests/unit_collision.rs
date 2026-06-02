use simulation::scenario::SimConfig;

/// collision_trigger_field = 2 × collision_radius_nm × nm_to_field
#[test]
fn collision_trigger_field_formula() {
    let cfg = SimConfig::default();
    // nm_to_field = 1000/750 ≈ 1.3333
    let expected = 2.0 * cfg.collision_radius_nm * cfg.nm_to_field;
    assert!(
        (cfg.collision_trigger_field - expected).abs() < 1e-9,
        "collision_trigger_field = {:.6}, expected {:.6}",
        cfg.collision_trigger_field,
        expected
    );
}

#[test]
fn default_collision_radius_nm() {
    let cfg = SimConfig::default();
    assert!((cfg.collision_radius_nm - 0.15).abs() < 1e-9,
        "default collision_radius_nm should be 0.15 nm");
}

#[test]
fn default_stability_threshold() {
    let cfg = SimConfig::default();
    assert_eq!(cfg.stability_threshold, 3,
        "default stability_threshold should be 3");
}

#[test]
fn collision_trigger_field_approx_0_4() {
    let cfg = SimConfig::default();
    // 2 × 0.15 × (1000/750) = 0.3 × 1.333... ≈ 0.400
    assert!(
        (cfg.collision_trigger_field - 0.4).abs() < 0.01,
        "collision_trigger_field should be ≈0.4 field units; got {}",
        cfg.collision_trigger_field
    );
}

#[test]
fn nm_to_field_value() {
    let cfg = SimConfig::default();
    let expected = 1000.0 / 750.0;
    assert!((cfg.nm_to_field - expected).abs() < 1e-9,
        "nm_to_field should be 1000/750 ≈ {expected:.6}; got {}", cfg.nm_to_field);
}

#[test]
fn kpi_record_collision_tracked() {
    use simulation::kpi::KpiAccumulator;
    let mut acc = KpiAccumulator::new();
    acc.ship_hrs = 1000.0;
    acc.record_collision();
    acc.record_collision();
    let snap = acc.snapshot(0);
    assert!((snap.collision_per_1k_hrs - 2.0).abs() < 1e-9,
        "2 collisions over 1000 ship-hrs = 2.0/1k-hrs; got {}", snap.collision_per_1k_hrs);
}
