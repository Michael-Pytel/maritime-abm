use simulation::human::CrewArchetype;

#[test]
fn modifier_values() {
    assert!((CrewArchetype::Veteran.modifier()  - (-1.5)).abs() < 1e-12);
    assert!((CrewArchetype::Standard.modifier() -   0.0 ).abs() < 1e-12);
    assert!((CrewArchetype::Green.modifier()    -   2.0 ).abs() < 1e-12);
}

#[test]
fn ordering() {
    assert!(CrewArchetype::Veteran.modifier() < CrewArchetype::Standard.modifier());
    assert!(CrewArchetype::Standard.modifier() < CrewArchetype::Green.modifier());
}

#[test]
fn equality_derives() {
    assert_eq!(CrewArchetype::Veteran,  CrewArchetype::Veteran);
    assert_ne!(CrewArchetype::Veteran,  CrewArchetype::Green);
    assert_eq!(CrewArchetype::Standard, CrewArchetype::Standard);
}

#[test]
fn copy_semantics() {
    let a = CrewArchetype::Green;
    let b = a; // Copy
    assert_eq!(a, b);
}
