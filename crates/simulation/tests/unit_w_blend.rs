use simulation::human::CrewArchetype;
use simulation::mesh::MeshPacket;
use simulation::scenario::{Method, SimConfig};
use simulation::vessel::{VesselAgent, VesselState};

fn base_vessel(id: u64, method_cfg: &SimConfig) -> VesselAgent {
    let _ = method_cfg;
    VesselAgent {
        id,
        name: format!("V{id}"),
        vessel_type: "cargo".into(),
        route: vec![],
        route_index: 0,
        route_direction: 1,
        position: (500.0, 500.0),
        last_valid_position: (500.0, 500.0),
        heading_deg: 0.0,
        speed_kn: 0.0,
        state: VesselState::Active,
        damage: 0,
        n_crew: 10,
        hours_awake: 0.0,
        archetype: CrewArchetype::Standard,
        p_prep: 0.5,
        shore_age_ticks: 0,
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore: 0.4,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

fn packet(sender: u64, w: f64, tick: u64, hop: u8) -> MeshPacket {
    MeshPacket { sender_id: sender, observed_w: w, position: (0.0, 0.0), tick, hop_count: hop, is_sos: false }
}

/// BaselineA → w_blend equals w_shore (shore-broadcast only; mesh packets ignored).
#[test]
fn baseline_a_always_zero() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::BaselineA;

    let mut v = base_vessel(1, &cfg);
    v.w_shore = 0.9;
    // Mesh packet has a different value; BaselineA must ignore it and return w_shore.
    v.radio_inbox.push(packet(99, 0.1, 0, 0));

    v.step(0, 0.0, 0.0, &cfg, &[]);
    assert!((v.w_blend - 0.9).abs() < 1e-9,
        "BaselineA should produce w_blend=w_shore=0.9; got {}", v.w_blend);
}

/// BaselineB → w_blend is always w_shore (shore trust always dominates).
#[test]
fn baseline_b_always_w_shore() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::BaselineB;

    let mut v = base_vessel(2, &cfg);
    let w_shore_val = 0.4;
    v.w_shore = w_shore_val;
    // Put a very different radio packet in inbox; BaselineB should ignore it
    v.radio_inbox.push(packet(99, 0.9, 0, 0));

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // BaselineB match arm always returns w_shore
    assert!((v.w_blend - w_shore_val).abs() < 1e-9,
        "BaselineB should produce w_blend=w_shore={w_shore_val}; got {}", v.w_blend);
}

/// Proposed with no inbox → falls back to w_shore.
#[test]
fn proposed_no_inbox_uses_w_shore() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;

    let mut v = base_vessel(3, &cfg);
    v.w_shore = 0.6;
    // No packets in inbox; w_ship = None

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // With no w_ship, code returns w_shore regardless of lambda
    assert!((v.w_blend - 0.6).abs() < 1e-9,
        "Proposed with empty inbox should fall back to w_shore; got {}", v.w_blend);
}

/// Proposed with fresh shore (age=1 → λ=0) and a mesh packet → w_blend = w_shore.
#[test]
fn proposed_fresh_shore_prioritises_shore() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;
    cfg.shore_trust_decay_k = 5.0;

    let mut v = base_vessel(4, &cfg);
    v.w_shore = 0.3;
    v.shore_age_ticks = 0; // will become 1 in step(), λ = 1-exp(0) = 0
    v.radio_inbox.push(packet(9, 0.9, 0, 0)); // very different observed_w

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // λ=0 → w_blend = 0*w_ship + 1*w_shore = 0.3
    assert!((v.w_blend - 0.3).abs() < 1e-9,
        "Fresh shore (age=1, λ=0): w_blend should equal w_shore=0.3; got {}", v.w_blend);
}

/// Proposed with stale shore → w_blend approaches mesh estimate.
#[test]
fn proposed_stale_shore_prioritises_mesh() {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;
    cfg.shore_trust_decay_k = 5.0;

    let mut v = base_vessel(5, &cfg);
    v.w_shore = 0.0; // stale shore says calm
    v.shore_age_ticks = 99; // will become 100, λ ≈ 1 - exp(-99/5) ≈ 1
    // Mesh packet says very dangerous
    v.radio_inbox.push(packet(10, 0.8, 0, 0));

    v.step(0, 0.0, 0.0, &cfg, &[]);
    // λ ≈ 1 → w_blend ≈ w_ship ≈ 0.8
    assert!(v.w_blend > 0.7,
        "Stale shore (λ≈1): w_blend should approach mesh estimate ≈0.8; got {}", v.w_blend);
}
