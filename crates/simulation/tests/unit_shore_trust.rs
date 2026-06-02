use simulation::human::CrewArchetype;
use simulation::mesh::MeshPacket;
use simulation::scenario::{Method, SimConfig};
use simulation::vessel::{VesselAgent, VesselState};

fn vessel_with_age(shore_age: u32, w_shore: f64) -> VesselAgent {
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
        speed_kn: 0.0,
        state: VesselState::Active,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep: 0.5,
        shore_age_ticks: shore_age,
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

fn inbox_packet(w: f64) -> MeshPacket {
    MeshPacket { sender_id: 99, observed_w: w, position: (0.0, 0.0), tick: 0, hop_count: 0, is_sos: false }
}

fn proposed_cfg(k_lambda: f64) -> SimConfig {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;
    cfg.shore_trust_decay_k = k_lambda;
    cfg
}

/// λ at age=1: 1 - exp(-(1-1)/k) = 0. Shore fully trusted.
#[test]
fn lambda_zero_at_age_1() {
    let cfg = proposed_cfg(5.0);
    let mut v = vessel_with_age(0, 0.2); // age 0 → step makes it 1
    v.radio_inbox.push(inbox_packet(0.9)); // mesh says 0.9

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // λ=0 → w_blend = w_shore = 0.2
    assert!((v.w_blend - 0.2).abs() < 1e-9,
        "Fresh (age=1, λ=0): w_blend should equal w_shore=0.2; got {}", v.w_blend);
}

/// λ at age=30 (→ 31 in step): 1 - exp(-30/5) ≈ 0.9975. Mesh dominates.
#[test]
fn lambda_near_one_stale() {
    let cfg = proposed_cfg(5.0);
    let mut v = vessel_with_age(30, 0.0); // w_shore=0 (stale calm)
    v.radio_inbox.push(inbox_packet(0.8)); // mesh says 0.8

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // λ ≈ 0.9975 → w_blend ≈ 0.9975 * 0.8 ≈ 0.798
    assert!(v.w_blend > 0.78,
        "Stale shore (age~30, λ≈1): w_blend should be near mesh value ≈0.8; got {}", v.w_blend);
}

/// k_lambda=5: at effective age=5 (shore_age_ticks=5→6 in step),
/// λ = 1 - exp(-5/5) = 1 - e⁻¹ ≈ 0.632.
#[test]
fn lambda_at_age_6_approx_0_63() {
    let cfg = proposed_cfg(5.0);
    let mut v = vessel_with_age(5, 0.0); // age 5 → 6 in step
    v.radio_inbox.push(inbox_packet(1.0)); // mesh at 1.0, shore at 0.0
    // w_blend = λ * 1.0 + (1-λ) * 0.0 = λ

    v.step(0, 0.0, 0.0, &cfg, &[]);
    let lambda = v.w_blend; // = λ * 1.0
    let expected = 1.0 - (-5.0_f64 / 5.0).exp();
    assert!(
        (lambda - expected).abs() < 1e-9,
        "λ(age=6,k=5) should be ≈{expected:.4}; got {lambda:.4}"
    );
}

/// receive_shore_broadcast resets shore_age_ticks to 0 and updates w_shore.
#[test]
fn receive_shore_broadcast_resets_age() {
    let mut v = vessel_with_age(20, 0.5);
    v.receive_shore_broadcast(0.7);
    assert_eq!(v.shore_age_ticks, 0, "shore_age_ticks should reset to 0 after broadcast");
    assert!((v.w_shore - 0.7).abs() < 1e-12, "w_shore should update to broadcast value");
}

/// After receiving a broadcast, the next step starts fresh (λ=0).
#[test]
fn fresh_after_reset_gives_lambda_zero() {
    let cfg = proposed_cfg(5.0);
    let mut v = vessel_with_age(20, 0.1);
    v.receive_shore_broadcast(0.2); // reset: age=0, w_shore=0.2
    v.radio_inbox.push(inbox_packet(0.9));

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // age 0→1, λ=0, w_blend = w_shore = 0.2
    assert!((v.w_blend - 0.2).abs() < 1e-9,
        "After broadcast reset: w_blend should equal w_shore=0.2; got {}", v.w_blend);
}
