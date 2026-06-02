use simulation::human::{p_prep, CrewArchetype};

#[test]
fn range_zero_to_one() {
    for e in [0.0_f64, 0.3, 0.6, 1.0, 2.0] {
        for arch in [CrewArchetype::Veteran, CrewArchetype::Standard, CrewArchetype::Green] {
            let v = p_prep(e, arch);
            assert!(v >= 0.0 && v <= 1.0, "p_prep({e},{arch:?})={v} out of [0,1]");
        }
    }
}

#[test]
fn decreases_with_forecast_error() {
    for arch in [CrewArchetype::Veteran, CrewArchetype::Standard, CrewArchetype::Green] {
        let lo = p_prep(0.0, arch);
        let hi = p_prep(0.5, arch);
        assert!(lo >= hi, "p_prep should be non-increasing in e_wx for {arch:?}");
    }
}

#[test]
fn archetype_ordering_same_error() {
    // Veteran has mod=-1.5, Green has mod=+2.0 → Veteran higher p_prep
    for e in [0.0_f64, 0.2, 0.4] {
        let vet  = p_prep(e, CrewArchetype::Veteran);
        let std_ = p_prep(e, CrewArchetype::Standard);
        let grn  = p_prep(e, CrewArchetype::Green);
        assert!(vet >= std_, "Veteran p_prep {vet} should be ≥ Standard {std_} at e_wx={e}");
        assert!(std_ >= grn, "Standard p_prep {std_} should be ≥ Green {grn} at e_wx={e}");
    }
}

#[test]
fn clamped_at_zero_for_large_error() {
    // Large e_wx must clamp to 0 rather than going negative
    let v = p_prep(10.0, CrewArchetype::Green);
    assert_eq!(v, 0.0, "p_prep should clamp to 0 for large e_wx");
}

#[test]
fn clamped_at_one_for_zero_error_veteran() {
    // Veteran at e_wx=0: 1 - 0 - 0.2*(-1.5) = 1 + 0.3 = 1.3 → clamped to 1
    let v = p_prep(0.0, CrewArchetype::Veteran);
    assert!((v - 1.0).abs() < 1e-12, "p_prep(0,Veteran)=1 (clamped); got {v}");
}

#[test]
fn formula_spot_check_standard() {
    // P_prep = clip(1 - 1.5*e_wx - 0.2*mod, 0, 1)  with mod=0 for Standard
    let e = 0.3;
    let expected = (1.0_f64 - 1.5 * e).clamp(0.0, 1.0);
    let got = p_prep(e, CrewArchetype::Standard);
    assert!((got - expected).abs() < 1e-12, "expected {expected}, got {got}");
}
