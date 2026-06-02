/// P_raft = clip(B_raft − δ_wx·W + δ_prep·P_prep, 0, 1)
/// Constants: B_RAFT=0.90, DELTA_WX=0.60, DELTA_PREP=0.25  (Eq. 16)

fn p_raft_formula(w: f64, p_prep: f64) -> f64 {
    const B_RAFT: f64 = 0.90;
    const DELTA_WX: f64 = 0.60;
    const DELTA_PREP: f64 = 0.25;
    (B_RAFT - DELTA_WX * w + DELTA_PREP * p_prep).clamp(0.0, 1.0)
}

#[test]
fn calm_water_no_prep() {
    let v = p_raft_formula(0.0, 0.0);
    assert!((v - 0.90).abs() < 1e-12, "P_raft(W=0,Pp=0)=0.90; got {v}");
}

#[test]
fn moderate_water_no_prep() {
    // W=1, Pp=0: 0.90 - 0.60 = 0.30
    let v = p_raft_formula(1.0, 0.0);
    assert!((v - 0.30).abs() < 1e-12, "P_raft(W=1,Pp=0)=0.30; got {v}");
}

#[test]
fn severe_water_clamps_to_zero() {
    // W=2, Pp=0: 0.90 - 1.20 = -0.30 → clamp to 0
    let v = p_raft_formula(2.0, 0.0);
    assert!((v - 0.0).abs() < 1e-12, "P_raft(W=2,Pp=0)=0 (clamped); got {v}");
}

#[test]
fn p_prep_bonus() {
    // W=0, Pp=1: 0.90 + 0.25 = 1.15 → clamp to 1.0
    let v = p_raft_formula(0.0, 1.0);
    assert!((v - 1.0).abs() < 1e-12, "P_raft(W=0,Pp=1)=1.0 (clamped); got {v}");
}

#[test]
fn decreases_with_weather() {
    let low  = p_raft_formula(0.0, 0.5);
    let mid  = p_raft_formula(0.5, 0.5);
    let high = p_raft_formula(1.0, 0.5);
    assert!(low >= mid,  "P_raft should be non-increasing in W");
    assert!(mid >= high, "P_raft should be non-increasing in W");
}

#[test]
fn increases_with_p_prep() {
    let low  = p_raft_formula(0.5, 0.0);
    let high = p_raft_formula(0.5, 1.0);
    assert!(high >= low, "P_raft should be non-decreasing in P_prep");
}

#[test]
fn range_zero_to_one() {
    for w in [0.0_f64, 0.5, 1.0, 1.5, 2.0] {
        for pp in [0.0_f64, 0.5, 1.0] {
            let v = p_raft_formula(w, pp);
            assert!(v >= 0.0 && v <= 1.0, "P_raft({w},{pp})={v} out of [0,1]");
        }
    }
}
