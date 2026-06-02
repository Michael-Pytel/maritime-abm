use simulation::mesh::{MeshPacket, RelayEngine};

fn pkt(sender: u64, tick: u64, hop: u8) -> MeshPacket {
    MeshPacket { sender_id: sender, observed_w: 0.5, position: (0.0, 0.0), tick, hop_count: hop, is_sos: false }
}

#[test]
fn is_new_dedup_same_sender_same_tick() {
    let mut relay = RelayEngine::new();
    let p = pkt(1, 10, 0);
    assert!(relay.is_new(&p));
    assert!(!relay.is_new(&p), "identical packet should be a duplicate");
}

#[test]
fn is_new_different_tick_not_dup() {
    let mut relay = RelayEngine::new();
    assert!(relay.is_new(&pkt(1, 10, 0)));
    assert!(relay.is_new(&pkt(1, 11, 0)), "same sender, different tick should not be dup");
}

#[test]
fn reset_tick_clears_seen_set() {
    let mut relay = RelayEngine::new();
    let p = pkt(7, 5, 0);
    relay.is_new(&p);
    relay.reset_tick();
    assert!(relay.is_new(&p), "after reset_tick the same packet should be seen as new");
}

#[test]
fn sos_dedup_same_vessel() {
    let mut relay = RelayEngine::new();
    relay.enqueue_sos(42, (0.0, 0.0), 1);
    relay.enqueue_sos(42, (1.0, 1.0), 2);
    assert_eq!(relay.sos_queue.len(), 1, "duplicate SOS for same vessel should be suppressed");
}

#[test]
fn sos_different_vessels_both_queued() {
    let mut relay = RelayEngine::new();
    relay.enqueue_sos(1, (0.0, 0.0), 1);
    relay.enqueue_sos(2, (0.0, 0.0), 1);
    assert_eq!(relay.sos_queue.len(), 2);
}

#[test]
fn drain_sos_returns_and_clears() {
    let mut relay = RelayEngine::new();
    relay.enqueue_sos(1, (10.0, 20.0), 5);
    relay.enqueue_sos(2, (30.0, 40.0), 5);
    let drained = relay.drain_sos();
    assert_eq!(drained.len(), 2);
    assert!(relay.sos_queue.is_empty(), "queue should be empty after drain");
}

#[test]
fn drained_sos_has_correct_fields() {
    let mut relay = RelayEngine::new();
    relay.enqueue_sos(99, (7.0, 8.0), 42);
    let mut drained = relay.drain_sos();
    let s = drained.remove(0);
    assert_eq!(s.vessel_id, 99);
    assert_eq!(s.tick, 42);
    assert!((s.position.0 - 7.0).abs() < 1e-12 && (s.position.1 - 8.0).abs() < 1e-12);
}

#[test]
fn reset_tick_does_not_clear_sos_queue() {
    let mut relay = RelayEngine::new();
    relay.enqueue_sos(1, (0.0, 0.0), 1);
    relay.reset_tick();
    assert_eq!(relay.sos_queue.len(), 1, "reset_tick should only clear seen set, not SOS queue");
}
