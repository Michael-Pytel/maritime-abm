use simulation::human::fatigue_circadian;
use std::f64::consts::PI;

#[test]
fn nadir_at_0300() {
    let nadir = fatigue_circadian(3.0);
    assert!((nadir - (-1.0)).abs() < 1e-9, "F_circ(03:00) = -cos(0) = -1; got {nadir}");
    for h in (0..240).map(|i| i as f64 / 10.0) {
        if (h - 3.0).abs() > 0.05 {
            assert!(
                fatigue_circadian(h) > nadir,
                "h={h:.1} should be less fatiguing than nadir; F_circ={:.4}", fatigue_circadian(h)
            );
        }
    }
}

#[test]
fn peak_at_1500() {
    let peak = fatigue_circadian(15.0);
    assert!((peak - 1.0).abs() < 1e-9, "F_circ(15:00) = -cos(π) = 1; got {peak}");
    for h in (0..240).map(|i| i as f64 / 10.0) {
        if (h - 15.0).abs() > 0.05 {
            assert!(
                fatigue_circadian(h) < peak,
                "h={h:.1} should be less alert than peak; F_circ={:.4}", fatigue_circadian(h)
            );
        }
    }
}

#[test]
fn range_minus1_to_plus1() {
    for h in (0..=240).map(|i| i as f64 / 10.0) {
        let v = fatigue_circadian(h);
        assert!(v >= -1.0 - 1e-9 && v <= 1.0 + 1e-9, "F_circ({h:.1}) = {v} out of [-1,1]");
    }
}

#[test]
fn period_24h() {
    for h in (0..=240).map(|i| i as f64 / 10.0) {
        let v1 = fatigue_circadian(h);
        let v2 = fatigue_circadian(h + 24.0);
        assert!((v1 - v2).abs() < 1e-9, "F_circ should be 24-periodic at h={h:.1}");
    }
}

#[test]
fn formula_spot_check() {
    // F_circ(h) = -cos(2π(h-3)/24)
    let h = 9.0;
    let expected = -f64::cos(2.0 * PI / 24.0 * (h - 3.0));
    assert!((fatigue_circadian(h) - expected).abs() < 1e-12);
}
