//! SIMCOL collision-consequence model.
//!
//! Replaces the ad-hoc per-crew Bernoulli casualty rule with a physics chain
//! grounded in the DTU SIMCOL model (Lützen 2001) and Minorsky (1959):
//!
//! 1. **External dynamics** — the kinetic energy released for hull deformation
//!    is the reduced-mass energy on the *normal* (side-penetrating) component of
//!    the closing velocity, using Lützen's sway/surge added-mass coefficients
//!    (her Ch. 3, after Pedersen & Zhang 1998). It reduces to the classical
//!    right-angle closed form when the impact is perpendicular.
//! 2. **Internal mechanics** — Minorsky's absorbed-energy / resistance-volume
//!    correlation `E_abs = a·R_T + b` gives the destroyed material volume, which
//!    maps to a non-dimensional penetration `t/B`.
//! 3. **Survivability** — a SOLAS II-1-shaped survival factor `S_i ∈ [0, 1]`
//!    that decreases with relative penetration. Expected fatalities are
//!    `n_b·(1 − S_i)`; an `S_i` below the founder threshold sinks the vessel
//!    into the search-and-rescue chain.
//!
//! The fixed module consts below are the *accredited physical* constants
//! (added-mass coefficients, Minorsky correlation), verified against Lützen
//! (2001) Ch. 3 and Minorsky (1959). The tunable fields in [`SimcolParams`]
//! marked `TODO-VERIFY` are the *surrogate* calibration knobs — the penetration
//! geometry and the SOLAS-shaped survival knee — which the source does not pin
//! to closed-form constants at this fidelity.

use serde::{Deserialize, Serialize};

// ── Verified physical constants ──────────────────────────────────────────────

/// Hydrodynamic added-mass coefficient for sway (struck ship moving sideways).
/// Lützen (2001) Ch. 3 uses 0.85 in her worked examples.
const ADDED_MASS_SWAY: f64 = 0.85;

/// Hydrodynamic added-mass coefficient for surge (striking ship forward motion).
/// Lützen (2001): `m_ax ≈ 0.02–0.07`; 0.05 used in her examples.
const ADDED_MASS_SURGE: f64 = 0.05;

/// Minorsky (1959) absorbed-energy / resistance-volume correlation slope
/// in `E_abs[MJ] = a·R_T[m³] + b`.
const MINORSKY_A_MJ_PER_M3: f64 = 47.2;

/// Minorsky (1959) correlation intercept (MJ).
const MINORSKY_B_MJ: f64 = 32.7;

/// One knot in metres per second.
const KNOTS_TO_MPS: f64 = 0.514_444;

/// One tonne in kilograms.
const TONNE_TO_KG: f64 = 1000.0;

// ── Calibration / surrogate parameters ───────────────────────────────────────

/// Tunable SIMCOL surrogate parameters (overridable via `SimConfig`).
///
/// These are *not* the accredited physical constants (which are fixed module
/// consts); they parameterise the damage-geometry and survival surrogates that
/// the source treats as distributions / regulatory stability calculations
/// rather than closed-form constants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SimcolParams {
    /// Representative side damage length as a fraction of struck-ship length.
    /// Lützen non-dimensional damage-length distributions: median ≈ 0.10–0.15.
    pub damage_length_frac: f64,

    /// Effective smeared steel thickness of the struck side (m), converting the
    /// Minorsky resistance volume into a penetration depth via
    /// `t = R_T / (ℓ · τ)`. TODO-VERIFY — structural surrogate (representative
    /// merchant value); Lützen reports damage as fitted distributions, not a τ.
    pub effective_steel_thickness_m: f64,

    /// Relative penetration `t/B` at/below which the ship survives intact
    /// (`S_i = 1`). TODO-VERIFY — SOLAS-shaped survival-surrogate knee.
    pub survival_knee_tb: f64,

    /// Relative penetration `t/B` at/above which survival reaches 0 (certain
    /// foundering). TODO-VERIFY — SOLAS-shaped survival surrogate.
    pub survival_full_loss_tb: f64,

    /// Exponent shaping the survival curve between knee and full loss.
    /// SOLAS II-1 uses 0.25. TODO-VERIFY.
    pub survival_exponent: f64,

    /// Survival factor below which the struck vessel founders and enters the
    /// SAR chain (transitions to `Evac`). TODO-VERIFY.
    pub founder_threshold: f64,
}

impl Default for SimcolParams {
    fn default() -> Self {
        Self {
            damage_length_frac: 0.12,
            effective_steel_thickness_m: 0.08,
            survival_knee_tb: 0.10,
            survival_full_loss_tb: 0.30,
            survival_exponent: 0.25,
            founder_threshold: 0.50,
        }
    }
}

// ── Inputs / outputs ─────────────────────────────────────────────────────────

/// Minimal kinematic + physical description of one vessel in a collision.
#[derive(Debug, Clone, Copy)]
pub struct CollisionShip {
    /// Compass heading (degrees).
    pub heading_deg: f64,
    /// Speed (knots).
    pub speed_kn: f64,
    /// Displacement mass (tonnes).
    pub mass_tonnes: f64,
    /// Moulded beam (m).
    pub beam_m: f64,
    /// Length between perpendiculars (m).
    pub length_m: f64,
}

/// Result of evaluating the consequence to one struck vessel.
#[derive(Debug, Clone, Copy)]
pub struct CollisionOutcome {
    /// Absorbed (crushing) energy, MJ.
    pub energy_mj: f64,
    /// Non-dimensional penetration `t/B`.
    pub penetration_tb: f64,
    /// Survival factor `S_i ∈ [0, 1]`.
    pub survival_factor: f64,
    /// Whether the struck vessel founders (enters the SAR chain).
    pub founders: bool,
}

// ── Geometry helper ──────────────────────────────────────────────────────────

/// Field-frame velocity (knots) for a compass heading, matching the engine's
/// convention `v(ψ) = v·(sinψ, cosψ)`.
fn velocity_kn(heading_deg: f64, speed_kn: f64) -> (f64, f64) {
    let psi = heading_deg.to_radians();
    (speed_kn * psi.sin(), speed_kn * psi.cos())
}

// ── Public evaluation ────────────────────────────────────────────────────────

/// Evaluate the consequence of `striker` hitting the side of `struck`.
///
/// Returns the absorbed energy, non-dimensional penetration, survival factor and
/// foundering flag for the **struck** vessel. Both directions are evaluated
/// separately by the caller so each vessel's own crew is assessed.
#[must_use]
pub fn evaluate(
    struck: CollisionShip,
    striker: CollisionShip,
    p: &SimcolParams,
) -> CollisionOutcome {
    // Relative (closing) velocity in field frame, knots.
    let (sx, sy) = velocity_kn(striker.heading_deg, striker.speed_kn);
    let (ux, uy) = velocity_kn(struck.heading_deg, struck.speed_kn);
    let (rvx, rvy) = (sx - ux, sy - uy);

    // Unit normal to the struck ship's side = its transverse (sway) axis.
    // Struck centreline ĉ = (sinψ, cosψ); transverse n̂ = (cosψ, −sinψ).
    let psi = struck.heading_deg.to_radians();
    let (nx, ny) = (psi.cos(), -psi.sin());

    // Normal component of the closing speed — the side-penetrating velocity.
    // This carries the oblique-angle dependence: a parallel graze contributes
    // ~0, a perpendicular T-bone contributes the full closing speed.
    let v_normal_kn = (rvx * nx + rvy * ny).abs();
    let v_normal = v_normal_kn * KNOTS_TO_MPS; // m/s

    // Reduced mass with added mass: struck moves in sway, striker in surge.
    let m_struck = struck.mass_tonnes * TONNE_TO_KG * (1.0 + ADDED_MASS_SWAY);
    let m_striker = striker.mass_tonnes * TONNE_TO_KG * (1.0 + ADDED_MASS_SURGE);
    let reduced = (m_struck * m_striker) / (m_struck + m_striker);

    // External dynamics: kinetic energy released for crushing (J → MJ).
    let energy_mj = 0.5 * reduced * v_normal * v_normal / 1.0e6;

    // Internal mechanics: Minorsky resistance volume (m³), then penetration.
    let r_t = ((energy_mj - MINORSKY_B_MJ) / MINORSKY_A_MJ_PER_M3).max(0.0);
    let damage_len = (p.damage_length_frac * struck.length_m).max(1.0);
    let penetration_m = r_t / (damage_len * p.effective_steel_thickness_m);
    let penetration_tb = penetration_m / struck.beam_m;

    let survival_factor = survival(penetration_tb, p);
    let founders = survival_factor < p.founder_threshold;

    CollisionOutcome {
        energy_mj,
        penetration_tb,
        survival_factor,
        founders,
    }
}

// ── Collision geometry classification ────────────────────────────────────────

/// Encounter geometry of a collision, classified by the angle between the two
/// vessels' headings (folded to `[0°, 180°]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CollisionType {
    /// Near-parallel headings (overtaking / sideswipe): the low-energy case.
    SideToSide,
    /// Roughly perpendicular headings (crossing): one ship strikes the other's
    /// side — the classic T-bone.
    FrontToSide,
    /// Reciprocal headings: the two vessels meet bows-on.
    HeadOn,
}

impl CollisionType {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CollisionType::SideToSide => "side_to_side",
            CollisionType::FrontToSide => "front_to_side",
            CollisionType::HeadOn => "head_on",
        }
    }
}

/// The angle (degrees, `[0, 180]`) between two compass headings.
#[must_use]
pub fn heading_difference_deg(first_deg: f64, second_deg: f64) -> f64 {
    let raw = (first_deg - second_deg).rem_euclid(360.0);
    raw.min(360.0 - raw)
}

/// Classify a collision from the angle between the two vessels' headings.
///
/// Bands (conventional COLREGS-flavoured split on the folded heading
/// difference `θ ∈ [0°, 180°]`):
///   * `θ < 45°`        → [`CollisionType::SideToSide`] (overtaking / sideswipe)
///   * `45° ≤ θ < 135°` → [`CollisionType::FrontToSide`] (crossing / T-bone)
///   * `θ ≥ 135°`       → [`CollisionType::HeadOn`] (reciprocal courses)
#[must_use]
pub fn classify_collision(first_deg: f64, second_deg: f64) -> (CollisionType, f64) {
    let theta = heading_difference_deg(first_deg, second_deg);
    let kind = if theta < 45.0 {
        CollisionType::SideToSide
    } else if theta < 135.0 {
        CollisionType::FrontToSide
    } else {
        CollisionType::HeadOn
    };
    (kind, theta)
}

/// SOLAS II-1-shaped survival factor: 1 below the knee, falling to 0 at the
/// full-loss penetration, with a `survival_exponent`-shaped transition.
#[must_use]
pub fn survival(penetration_tb: f64, p: &SimcolParams) -> f64 {
    if penetration_tb <= p.survival_knee_tb {
        return 1.0;
    }
    if penetration_tb >= p.survival_full_loss_tb {
        return 0.0;
    }
    let frac =
        (p.survival_full_loss_tb - penetration_tb) / (p.survival_full_loss_tb - p.survival_knee_tb);
    frac.powf(p.survival_exponent).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship(heading_deg: f64, speed_kn: f64) -> CollisionShip {
        CollisionShip {
            heading_deg,
            speed_kn,
            mass_tonnes: 45_000.0,
            beam_m: 30.0,
            length_m: 190.0,
        }
    }

    #[test]
    fn survival_is_bounded_and_monotone_decreasing() {
        let p = SimcolParams::default();
        let mut prev = 1.0;
        let mut tb = 0.0;
        while tb <= 0.5 {
            let s = survival(tb, &p);
            assert!((0.0..=1.0).contains(&s), "S_i out of range at t/B={tb}");
            assert!(s <= prev + 1e-9, "S_i must not increase with penetration");
            prev = s;
            tb += 0.02;
        }
        assert!(survival(0.05, &p) > 1.0 - 1e-12, "below knee → survives");
        assert!(survival(0.40, &p) < 1e-12, "past full-loss → founders");
    }

    #[test]
    fn perpendicular_impact_absorbs_more_energy_than_glancing() {
        let p = SimcolParams::default();
        // Struck heading north (0°). A striker heading west (270°) T-bones its
        // side; a striker heading north (≈parallel) barely grazes.
        let struck = ship(0.0, 6.0);
        let tbone = evaluate(struck, ship(270.0, 12.0), &p);
        let parallel = evaluate(struck, ship(0.0, 12.0), &p);
        assert!(
            tbone.energy_mj > parallel.energy_mj,
            "perpendicular impact must release more crushing energy"
        );
    }

    #[test]
    fn faster_and_heavier_strikes_penetrate_deeper() {
        let p = SimcolParams::default();
        let struck = ship(0.0, 0.0);
        let slow = evaluate(struck, ship(270.0, 6.0), &p);
        let fast = evaluate(struck, ship(270.0, 18.0), &p);
        assert!(fast.energy_mj > slow.energy_mj);
        assert!(fast.penetration_tb >= slow.penetration_tb);
        // A high-speed perpendicular strike on a laden ship should founder it.
        assert!(
            fast.founders,
            "20 kn T-bone should founder the struck vessel"
        );
    }

    #[test]
    fn collision_geometry_is_classified_by_heading_angle() {
        // Same heading → overtaking / sideswipe.
        assert_eq!(classify_collision(90.0, 90.0).0, CollisionType::SideToSide);
        assert_eq!(classify_collision(0.0, 30.0).0, CollisionType::SideToSide);
        // ~90° apart → crossing / T-bone.
        assert_eq!(classify_collision(0.0, 90.0).0, CollisionType::FrontToSide);
        assert_eq!(classify_collision(350.0, 80.0).0, CollisionType::FrontToSide);
        // Reciprocal → head-on; angle folds across the 360° wrap.
        assert_eq!(classify_collision(0.0, 180.0).0, CollisionType::HeadOn);
        let (kind, theta) = classify_collision(10.0, 200.0);
        assert_eq!(kind, CollisionType::HeadOn);
        assert!((theta - 170.0).abs() < 1e-9, "folded angle, not 190°");
    }

    #[test]
    fn low_energy_grazing_is_a_near_miss() {
        let p = SimcolParams::default();
        // Two vessels on near-parallel tracks at modest speed barely deform.
        let out = evaluate(ship(0.0, 8.0), ship(5.0, 8.0), &p);
        assert!(out.survival_factor > 1.0 - 1e-12);
        assert!(!out.founders);
    }
}
