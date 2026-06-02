use simulation::human::{p_err, fatigue_circadian, CrewArchetype};

#[test]
fn range_zero_to_one() {
    for ha in [0.0_f64, 8.0, 16.0, 24.0] {
        for h in [0.0_f64, 3.0, 9.0, 15.0, 21.0] {
            for arch in [CrewArchetype::Veteran, CrewArchetype::Standard, CrewArchetype::Green] {
                let v = p_err(ha, h, arch);
                assert!(
                    v >= 0.0 && v <= 1.0,
                    "p_err({ha},{h},{arch:?}) = {v} out of [0,1]"
                );
            }
        }
    }
}

#[test]
fn archetype_ordering_same_conditions() {
    // At identical hours_awake and utc_hour, Veteran < Standard < Green
    for ha in [0.0_f64, 8.0, 16.0] {
        for h in [0.0_f64, 3.0, 15.0] {
            let veteran  = p_err(ha, h, CrewArchetype::Veteran);
            let standard = p_err(ha, h, CrewArchetype::Standard);
            let green    = p_err(ha, h, CrewArchetype::Green);
            assert!(
                veteran <= standard,
                "h_awake={ha}, utc={h}: Veteran {veteran} should be ≤ Standard {standard}"
            );
            assert!(
                standard <= green,
                "h_awake={ha}, utc={h}: Standard {standard} should be ≤ Green {green}"
            );
        }
    }
}

#[test]
fn increases_with_hours_awake() {
    // More hours awake → higher error probability (fatigue accumulates)
    let low  = p_err(4.0,  9.0, CrewArchetype::Standard);
    let mid  = p_err(12.0, 9.0, CrewArchetype::Standard);
    let high = p_err(24.0, 9.0, CrewArchetype::Standard);
    assert!(low < mid,  "p_err should rise from 4→12 hours awake");
    assert!(mid < high, "p_err should rise from 12→24 hours awake");
}

#[test]
fn fatigue_component_drives_error() {
    // At nadir (03:00) the circadian term is minimum (-1); at peak (15:00) maximum (+1).
    // Both should produce different p_err, with 15:00 > 03:00.
    let ha = 8.0;
    let nadir_err = p_err(ha, 3.0,  CrewArchetype::Standard);
    let peak_err  = p_err(ha, 15.0, CrewArchetype::Standard);
    assert!(peak_err > nadir_err,
        "p_err at 15:00 ({peak_err:.4}) should exceed p_err at 03:00 ({nadir_err:.4})");
}

#[test]
fn formula_spot_check_standard() {
    // p_err = σ(0.12*ha + 1.5*F_circ + archetype_mod - 4.0)
    let ha = 8.0;
    let h  = 9.0;
    let f_circ = fatigue_circadian(h);
    let logit  = 0.12 * ha + 1.5 * f_circ + 0.0 /* Standard */ - 4.0;
    let expected = 1.0 / (1.0 + (-logit).exp());
    let got = p_err(ha, h, CrewArchetype::Standard);
    assert!((got - expected).abs() < 1e-12, "expected {expected}, got {got}");
}
