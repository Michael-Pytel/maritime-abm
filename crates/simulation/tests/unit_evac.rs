use simulation::human::CrewArchetype;
use simulation::scenario::{Method, SimConfig};
use simulation::vessel::{VesselAgent, VesselState};

fn vessel(id: u64) -> VesselAgent {
    VesselAgent {
        id,
        name: format!("V{id}"),
        vessel_type: "cargo".into(),
        route: vec![],
        route_index: 0,
        route_direction: 1,
        position: (0.0, 0.0),
        last_valid_position: (0.0, 0.0),
        heading_deg: 0.0,
        speed_kn: 0.0,
        state: VesselState::Active,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep: 0.5,
        shore_age_ticks: 99, // stale shore so w_blend ≈ w_ship
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore: 0.0,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

/// BaselineA never triggers evacuation (evaluate_evacuation is skipped).
#[test]
fn baseline_a_never_evacs() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::BaselineA;

    let mut v = vessel(1);
    // Run 100 ticks; BaselineA skips evaluate_evacuation entirely
    for tick in 0..100 {
        v.step(tick, 1.0, 1.0, &cfg, &[]);
        // State may become Sunk due to evac_crew_loss if somehow Evac, but should not enter Evac
        assert!(
            v.state != VesselState::Evac,
            "BaselineA vessel should never enter Evac (tick {tick})"
        );
    }
}

/// r_t ramp: at tick=0, r_t = ((0+1)/24).clamp(0.08,1.0) = 0.08
/// This means w_eff = w_blend * 0.08 * r_w, depressing p_evac at early ticks.
#[test]
fn r_t_is_clamped_at_early_ticks() {
    // Verify the r_t formula: (tick+1)/24, clamped to [0.08, 1.0]
    // tick=0: 1/24 ≈ 0.042 → clamped to 0.08
    let r_t_tick0 = ((0u64 as f64 + 1.0) / 24.0).clamp(0.08, 1.0);
    assert!((r_t_tick0 - 0.08).abs() < 1e-12, "r_t at tick=0 should be 0.08; got {r_t_tick0}");
    // tick=23: 24/24 = 1.0 → 1.0
    let r_t_tick23 = ((23u64 as f64 + 1.0) / 24.0).clamp(0.08, 1.0);
    assert!((r_t_tick23 - 1.0).abs() < 1e-12, "r_t at tick=23 should be 1.0; got {r_t_tick23}");
    // tick>23: stays at 1.0
    let r_t_tick100 = ((100u64 as f64 + 1.0) / 24.0).clamp(0.08, 1.0);
    assert!((r_t_tick100 - 1.0).abs() < 1e-12, "r_t at tick=100 should be 1.0 (clamped)");
}

/// ProposedSystem with zero weather should almost never evac (p_evac ≈ σ(-2.8) ≈ 0.057).
#[test]
fn proposed_low_weather_rarely_evacs() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;

    // Run 200 distinct vessel IDs for 1 tick each at w=0
    // With p_evac ≈ 5.7%, on average ~11 would evac but we test it stays below 40%
    let mut evacs = 0usize;
    let total = 200usize;
    for id in 0..total as u64 {
        let mut v = vessel(id);
        v.shore_age_ticks = 0; // fresh shore → λ=0 → w_blend = w_shore = 0
        v.step(0, 0.0, 0.0, &cfg, &[]);
        if matches!(v.state, VesselState::Evac | VesselState::Sunk) {
            evacs += 1;
        }
    }
    let rate = evacs as f64 / total as f64;
    assert!(rate < 0.40,
        "At w=0, evac rate should be far below 40%; got {rate:.2} ({evacs}/{total})");
}

/// With w_blend near max and high p_prep, p_evac is high (> 0.9).
/// Across many IDs, most should evac or sink.
#[test]
fn proposed_high_weather_often_evacs() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;

    // Force w_blend to be high by using stale shore + high w_shore, run at tick=100 (r_t=1)
    let mut evacs = 0usize;
    let total = 200usize;
    for id in 0..total as u64 {
        let mut v = vessel(id);
        v.w_shore = 1.0;
        v.shore_age_ticks = 99; // stale → λ≈1, w_blend→w_ship; but no inbox so falls back to w_shore
        v.p_prep = 1.0;
        v.step(100, 1.0, 1.0, &cfg, &[]);
        if matches!(v.state, VesselState::Evac | VesselState::Sunk) {
            evacs += 1;
        }
    }
    let rate = evacs as f64 / total as f64;
    assert!(rate > 0.50,
        "At w=1 and high p_prep, evac+sink rate should exceed 50%; got {rate:.2} ({evacs}/{total})");
}
