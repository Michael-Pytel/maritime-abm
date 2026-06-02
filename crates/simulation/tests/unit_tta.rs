use simulation::rescue::{RescueAgent, RescueAssetType};
use simulation::scenario::SimConfig;
use simulation::vessel::{VesselAgent, VesselState};
use simulation::human::CrewArchetype;

fn make_vessel(id: u64, state: VesselState, pos: (f64, f64)) -> VesselAgent {
    VesselAgent {
        id,
        name: format!("V{id}"),
        vessel_type: "cargo".into(),
        route: vec![],
        route_index: 0,
        route_direction: 1,
        position: pos,
        heading_deg: 0.0,
        speed_kn: 15.0,
        state,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep: 1.0,
        shore_age_ticks: 0,
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore: 0.0,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

/// TTA = (rescue_tick - dispatch_tick) × 0.25 hours  (Eq. 17)
#[test]
fn tta_zero_when_rescue_at_dispatch_tick() {
    let mut cfg = SimConfig::default();
    // Place rescue agent directly on the distress vessel so it rescues immediately
    cfg.rescue_radius_nm = 1000.0; // huge radius: guaranteed rescue on first step
    cfg.compute_field_units();

    let dispatch_tick = 10u64;
    let mut agent = RescueAgent {
        id: 1,
        asset_type: RescueAssetType::Helicopter,
        position: (500.0, 500.0),
        last_valid_position: (500.0, 500.0),
        target_vessel_id: 42,
        target_pos: (500.0, 500.0),
        speed_kn: 60.0,
        mob_ticks_remaining: 0,
        dispatch_tick,
        done: false,
    };
    let mut vessels = vec![make_vessel(42, VesselState::Evac, (500.0, 500.0))];

    let result = agent.step(&mut vessels, 0.0, &cfg, dispatch_tick);
    let tta = result.expect("rescue should complete");
    assert!((tta - 0.0).abs() < 1e-9, "TTA at same tick as dispatch should be 0; got {tta}");
}

#[test]
fn tta_formula_dispatch_to_rescue() {
    let mut cfg = SimConfig::default();
    cfg.rescue_radius_nm = 1000.0;
    cfg.compute_field_units();

    let dispatch_tick = 5u64;
    let rescue_tick   = 13u64;
    let expected_tta  = (rescue_tick - dispatch_tick) as f64 * 0.25;

    let mut agent = RescueAgent {
        id: 2,
        asset_type: RescueAssetType::PatrolVessel,
        position: (500.0, 500.0),
        last_valid_position: (500.0, 500.0),
        target_vessel_id: 7,
        target_pos: (500.0, 500.0),
        speed_kn: 60.0,
        mob_ticks_remaining: 0,
        dispatch_tick,
        done: false,
    };
    let mut vessels = vec![make_vessel(7, VesselState::Evac, (500.0, 500.0))];

    let result = agent.step(&mut vessels, 0.0, &cfg, rescue_tick);
    let tta = result.expect("rescue should complete");
    assert!(
        (tta - expected_tta).abs() < 1e-9,
        "TTA should be {expected_tta} hrs; got {tta}"
    );
}

#[test]
fn mob_ticks_delay_rescue() {
    let mut cfg = SimConfig::default();
    cfg.rescue_radius_nm = 1000.0;
    cfg.compute_field_units();

    let dispatch_tick = 0u64;
    let mut agent = RescueAgent {
        id: 3,
        asset_type: RescueAssetType::Helicopter,
        position: (500.0, 500.0),
        last_valid_position: (500.0, 500.0),
        target_vessel_id: 99,
        target_pos: (500.0, 500.0),
        speed_kn: 60.0,
        mob_ticks_remaining: 3,
        dispatch_tick,
        done: false,
    };
    let mut vessels = vec![make_vessel(99, VesselState::Evac, (500.0, 500.0))];

    // Ticks during mobilisation return None
    for tick in 0..3 {
        let r = agent.step(&mut vessels, 0.0, &cfg, tick);
        assert!(r.is_none(), "should not rescue during mob_ticks (tick {tick})");
    }
    // After mob_ticks_remaining reaches 0, should rescue
    let r = agent.step(&mut vessels, 0.0, &cfg, 3);
    assert!(r.is_some(), "should rescue after mob_ticks exhausted");
}
