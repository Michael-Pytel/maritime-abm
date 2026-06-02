use simulation::human::CrewArchetype;
use simulation::mesh::MeshPacket;
use simulation::scenario::{Method, SimConfig};
use simulation::vessel::{VesselAgent, VesselState};

fn vessel(w_shore: f64) -> VesselAgent {
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
        shore_age_ticks: 99, // stale → λ≈1 so w_blend ≈ w_ship
        radio_inbox: vec![],
        w_blend: 0.0,
        w_shore,
        evac_started_tick: None,
        dock_until_tick: None,
        wreck_cause: None,
    }
}

fn pkt(w: f64, tick_sent: u64, current_tick: u64, hop: u8) -> MeshPacket {
    // age = current_tick - tick_sent
    let _ = current_tick; // stored in MeshPacket.tick; age computed in compute_w_blend
    MeshPacket { sender_id: hop as u64 + 1, observed_w: w, position: (0.0, 0.0), tick: tick_sent, hop_count: hop, is_sos: false }
}

fn proposed_stale() -> SimConfig {
    let mut cfg = SimConfig::default();
    cfg.method = Method::ProposedSystem;
    cfg.shore_trust_decay_k = 5.0;
    cfg
}

/// c_k = 1/(1+age) × 1/(1+hops). At equal age, hop=0 has 3× weight of hop=2.
/// Two packets: hop=0 says w=0.2, hop=2 says w=0.8 → w_ship biased toward 0.2.
#[test]
fn hop0_dominates_hop2_same_age() {
    let cfg = proposed_stale();
    let current_tick = 5u64;
    let mut v = vessel(0.0); // w_shore=0 so w_blend ≈ w_ship

    // Both sent at tick=5 (age=0), but different hop counts
    v.radio_inbox.push(pkt(0.2, current_tick, current_tick, 0)); // c = 1*1 = 1.0
    v.radio_inbox.push(pkt(0.8, current_tick, current_tick, 2)); // c = 1*(1/3) ≈ 0.333

    v.step(current_tick, 0.0, 0.0, &cfg, &[]);

    // w_ship = (1.0*0.2 + 0.333*0.8) / (1.0 + 0.333) = (0.2 + 0.267) / 1.333 ≈ 0.35
    // With λ≈1, w_blend ≈ w_ship ≈ 0.35 — much closer to 0.2 than 0.8
    let mid = 0.5;
    assert!(v.w_blend < mid,
        "hop=0 low-w packet should dominate; w_blend={:.4} should be < {mid}", v.w_blend);
}

/// Fresh packet (age=0) has 2× weight of 1-tick-old packet (age=1).
#[test]
fn fresh_packet_dominates_stale_same_hop() {
    let cfg = proposed_stale();
    let current_tick = 10u64;
    let mut v = vessel(0.0);

    // Both hop=0 but different ages
    v.radio_inbox.push(pkt(0.1, current_tick,     current_tick, 0)); // age=0, c=1.0
    v.radio_inbox.push(pkt(0.9, current_tick - 1, current_tick, 0)); // age=1, c=0.5

    v.step(current_tick, 0.0, 0.0, &cfg, &[]);

    // w_ship = (1.0*0.1 + 0.5*0.9) / (1.0 + 0.5) = (0.1 + 0.45) / 1.5 = 0.55/1.5 ≈ 0.367
    let mid = 0.5;
    assert!(v.w_blend < mid,
        "Fresh low-w packet should dominate; w_blend={:.4} should be < {mid}", v.w_blend);
}

/// Single packet: w_blend == observed_w (age=0, hop=0, λ≈1).
#[test]
fn single_fresh_hop0_packet() {
    let cfg = proposed_stale();
    let current_tick = 3u64;
    let mut v = vessel(0.0);
    v.radio_inbox.push(pkt(0.7, current_tick, current_tick, 0));

    v.step(current_tick, 0.0, 0.0, &cfg, &[]);
    // w_ship = 0.7, λ≈1, w_blend ≈ 0.7
    assert!((v.w_blend - 0.7).abs() < 0.01,
        "Single fresh hop=0 packet: w_blend≈0.7; got {:.4}", v.w_blend);
}
