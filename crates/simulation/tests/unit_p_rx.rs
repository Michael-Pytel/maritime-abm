use simulation::coastal::CoastalStation;
use simulation::human::CrewArchetype;
use simulation::scenario::SimConfig;
use simulation::vessel::{VesselAgent, VesselState};
use simulation::weather::WeatherField;

fn vessel_at(pos: (f64, f64)) -> VesselAgent {
    VesselAgent {
        id: 1,
        name: "T".into(),
        vessel_type: "cargo".into(),
        route: vec![],
        route_index: 0,
        route_direction: 1,
        position: pos,
        last_valid_position: pos,
        heading_deg: 0.0,
        speed_kn: 0.0,
        state: VesselState::Active,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep: 0.5,
        shore_age_ticks: 99,
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore: 0.0,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

/// Vessel directly on the station (d=0) should always receive the broadcast.
/// P_rx = σ((R - 0)/κ - 0) = σ(R/κ) ≈ 1 for R >> κ.
#[test]
fn vessel_at_station_receives() {
    let station_pos = (500.0, 500.0);
    let station = CoastalStation::new(1, station_pos);

    let mut cfg = SimConfig::default();
    cfg.shore_broadcast_radius_nm = 50.0;
    cfg.shore_noise_std = 0.0; // no noise so w_hat == w_true
    cfg.compute_field_units();

    let weather = WeatherField::default();
    let mut vessels = vec![vessel_at(station_pos)]; // d = 0
    let mut rng_state = 42u64;

    // Run broadcast 20 times; vessel should always receive (d=0, P_rx ≈ 1)
    for _ in 0..20 {
        station.broadcast(0, &weather, &mut vessels, &cfg, &mut rng_state);
    }

    // With P_rx ≈ 1, vessel should receive in nearly all broadcasts
    // We check shore_age_ticks reset: if it received, shore_age_ticks resets to 0
    // After 20 broadcasts with d=0, it should have received at least once
    let final_age = vessels[0].shore_age_ticks;
    assert!(final_age == 0,
        "Vessel at station (d=0): shore_age_ticks should be 0 after broadcast; got {final_age}");
}

/// Vessel far outside broadcast radius (d >> R) should almost never receive.
/// P_rx = σ((R - d)/κ) ≈ 0 for d >> R.
#[test]
fn vessel_far_outside_radius_does_not_receive() {
    let station_pos = (500.0, 500.0);
    let station = CoastalStation::new(2, station_pos);

    let mut cfg = SimConfig::default();
    cfg.shore_broadcast_radius_nm = 50.0;
    cfg.compute_field_units();

    let r = cfg.shore_broadcast_radius_field;
    // Place vessel at 100× the broadcast radius — P_rx ≈ 0
    let far_pos = (station_pos.0 + 100.0 * r, station_pos.1);

    let weather = WeatherField::default();
    let mut vessels = vec![vessel_at(far_pos)];
    let mut rng_state = 7u64;

    // Run broadcast 50 times; vessel should almost never receive
    let mut received = false;
    for tick in 0..50 {
        let initial_shore_w = vessels[0].w_shore;
        station.broadcast(tick, &weather, &mut vessels, &cfg, &mut rng_state);
        if (vessels[0].w_shore - initial_shore_w).abs() > 1e-12 {
            received = true;
            break;
        }
    }
    assert!(!received,
        "Vessel far outside radius (100×R) should not receive shore broadcast in 50 attempts");
}
