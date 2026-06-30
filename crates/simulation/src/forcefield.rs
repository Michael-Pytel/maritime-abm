//! Optional artificial-potential-field collision avoidance (Xiao et al. 2013).
//!
//! This is an *alternative* steering track to the CPA/COLREGS give-way logic in
//! [`crate::state`]. It is off by default (`ForceFieldParams::enabled == false`);
//! when enabled it replaces the waypoint-injection avoidance for one tick of
//! steering. With it off, the engine behaves byte-for-byte as before.
//!
//! Model (Xiao, Ligteringen, van Gulijk & Ale, *Nautical traffic simulation with
//! multi-agent system*, IEEE ITSC 2013):
//!
//! * Each obstacle within an activation distance `d_s` exerts a repulsive force
//!   `F_i = k_obst / d_i^p` along the unit vector pointing from the obstacle to
//!   the subject ship (their Eq. 10, with their exponent `k` written here as
//!   `p`). The forces sum into a resultant.
//! * The resultant's lateral (starboard) component sets a rudder command, which
//!   drives a first-order Nomoto turn response (their Eqs. 1–9) — heading lags
//!   the rudder rather than snapping to it.
//! * A small starboard bias breaks the symmetry of a dead-ahead head-on so the
//!   COLREGS "both alter to starboard" outcome emerges (their Art. 14 reading).
//! * Crew preparedness scales the whole rudder: a fatigued crew avoids later and
//!   more weakly.
//!
//! **Exponent `p`.** The source never pins a value: it states the parameters of
//! Eq. 10 were "set by coarse optimization" and that AIS calibration "is
//! currently being developed". We therefore expose `p` as a tunable parameter,
//! defaulting to inverse-square (`p = 2`), rather than hardcoding a false-precision
//! constant.
//!
//! **Scale.** Xiao's calibrated activation distances are micro-scale (head-on
//! `N(1548, 706²)` m ≈ 0.84 nm, overtaking `N(384, 358²)` m ≈ 0.21 nm) for a
//! <10 km waterway stepped every second. At this model's basin scale with 15-min
//! ticks (a 15-kn ship advances ~3.75 nm per tick) those distances are smaller
//! than a single step, so we rescale the magnitudes to operationally useful
//! values while preserving the source's ~4:1 head-on:overtaking ratio.

use serde::{Deserialize, Serialize};

/// Tunable parameters for the artificial-force-field steering track.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ForceFieldParams {
    /// Master switch. When `false` the engine uses the CPA/COLREGS avoidance and
    /// this module is never invoked (behaviour unchanged).
    pub enabled: bool,
    /// Repulsion exponent `p` in `F = k_obst / d^p` (Xiao's `k`). The source
    /// leaves this a free, hand-tuned constant; inverse-square by default.
    pub repulsion_exponent: f64,
    /// Repulsion scale/steepness `k_obst` (degrees · nm^p), folding the
    /// force→rudder gain so the resultant's lateral component is already a
    /// rudder angle in degrees.
    pub k_obst: f64,
    /// Activation distance for a head-on encounter (nm). Rescaled from Xiao's
    /// `N(1548, 706²)` m to this model's tick/basin scale.
    pub d_s_head_on_nm: f64,
    /// Activation distance for an overtaking encounter (nm). Rescaled from
    /// Xiao's `N(384, 358²)` m, preserving the ~4:1 head-on:overtaking ratio.
    pub d_s_overtaking_nm: f64,
    /// Effective Nomoto gain `K'` (turn-rate response per degree of rudder).
    pub nomoto_gain: f64,
    /// Effective Nomoto lag `T'` (response time constant, in ticks; `>= 1`).
    pub nomoto_lag: f64,
    /// Rudder clamp (degrees) — the largest course change commanded per tick.
    pub max_rudder_deg: f64,
    /// COLREGS starboard preference (degrees) applied when an activated obstacle
    /// lies ahead, so a symmetric head-on still resolves to starboard.
    pub starboard_bias_deg: f64,
    /// Fraction by which full crew fatigue weakens the avoidance rudder
    /// (`prep = 1 − fatigue_weakening · fatigue`, floored so it never vanishes).
    pub fatigue_weakening: f64,
}

impl Default for ForceFieldParams {
    fn default() -> Self {
        Self {
            enabled: false,
            // Xiao leaves the exponent unpinned; inverse-square assumed.
            repulsion_exponent: 2.0,
            k_obst: 50.0,
            // Rescaled to the 15-min tick / basin scale (ratio ≈ 4:1 preserved).
            d_s_head_on_nm: 8.0,
            d_s_overtaking_nm: 2.0,
            nomoto_gain: 0.5,
            nomoto_lag: 2.0,
            max_rudder_deg: 15.0,
            starboard_bias_deg: 3.0,
            fatigue_weakening: 0.5,
        }
    }
}

/// An obstacle the subject ship senses: another vessel's position and heading.
#[derive(Debug, Clone, Copy)]
pub struct Obstacle {
    pub position: (f64, f64),
    pub heading_deg: f64,
}

/// Result of one steering evaluation.
#[derive(Debug, Clone, Copy)]
pub struct SteerResult {
    /// New Nomoto yaw rate (deg/tick) — carry this forward as `prev_yaw`.
    pub yaw: f64,
    /// Heading after applying the yaw (compass degrees, wrapped to [0, 360)).
    pub new_heading: f64,
}

/// Folded heading difference in `[0, 180]` degrees.
#[must_use]
pub fn heading_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

/// Activation distance (nm) for the encounter geometry: heading difference > 90°
/// is a head-on/crossing encounter (longer reach), otherwise overtaking.
#[must_use]
pub fn activation_distance_nm(subj_heading: f64, obst_heading: f64, p: &ForceFieldParams) -> f64 {
    if heading_diff(subj_heading, obst_heading) > 90.0 {
        p.d_s_head_on_nm
    } else {
        p.d_s_overtaking_nm
    }
}

/// Unit forward vector for a compass heading in field coordinates
/// (heading 0° = +y "north", 90° = +x "east").
#[must_use]
pub fn forward(heading_deg: f64) -> (f64, f64) {
    let r = heading_deg.to_radians();
    (r.sin(), r.cos())
}

/// Unit starboard (right-of-heading) vector for a compass heading.
#[must_use]
pub fn starboard(heading_deg: f64) -> (f64, f64) {
    let r = heading_deg.to_radians();
    (r.cos(), -r.sin())
}

/// Sum of the repulsive forces (Xiao Eq. 10) on a subject ship, as a
/// degree-scaled field-coordinate vector. Obstacles beyond their encounter's
/// activation distance contribute nothing.
#[must_use]
pub fn resultant_force(
    subj_pos: (f64, f64),
    subj_heading: f64,
    obstacles: &[Obstacle],
    p: &ForceFieldParams,
) -> (f64, f64) {
    let mut fx = 0.0;
    let mut fy = 0.0;
    for o in obstacles {
        let dx = subj_pos.0 - o.position.0;
        let dy = subj_pos.1 - o.position.1;
        let d2 = dx * dx + dy * dy;
        if d2 <= f64::EPSILON {
            continue;
        }
        let d = d2.sqrt();
        if d >= activation_distance_nm(subj_heading, o.heading_deg, p) {
            continue;
        }
        let mag = p.k_obst / d.powf(p.repulsion_exponent);
        fx += mag * dx / d;
        fy += mag * dy / d;
    }
    (fx, fy)
}

/// Rudder command (degrees, +starboard) from a resultant force: the starboard
/// projection plus a starboard bias when an obstacle is ahead, clamped.
#[must_use]
pub fn rudder_command(
    force: (f64, f64),
    subj_heading: f64,
    obstacle_ahead: bool,
    p: &ForceFieldParams,
) -> f64 {
    let s = starboard(subj_heading);
    let lateral = force.0 * s.0 + force.1 * s.1;
    let bias = if obstacle_ahead {
        p.starboard_bias_deg
    } else {
        0.0
    };
    (lateral + bias).clamp(-p.max_rudder_deg, p.max_rudder_deg)
}

/// One step of the first-order Nomoto turn response: the yaw rate relaxes toward
/// `K' · rudder` with time constant `T'` (Xiao Eqs. 1–9, discretised at Δt = 1).
#[must_use]
pub fn nomoto_step(prev_yaw: f64, rudder: f64, p: &ForceFieldParams) -> f64 {
    let t = p.nomoto_lag.max(1.0);
    prev_yaw + (p.nomoto_gain * rudder - prev_yaw) / t
}

/// Evaluate one tick of force-field steering for a subject ship.
///
/// `prep ∈ (0, 1]` is the crew-preparedness factor scaling the whole rudder
/// (1 = fresh, lower = fatigued → weaker, later avoidance).
#[must_use]
pub fn steer(
    subj_pos: (f64, f64),
    subj_heading: f64,
    prev_yaw: f64,
    obstacles: &[Obstacle],
    prep: f64,
    p: &ForceFieldParams,
) -> SteerResult {
    let force = resultant_force(subj_pos, subj_heading, obstacles, p);
    // An obstacle is "ahead" when the net repulsion pushes against the heading.
    let fwd = forward(subj_heading);
    let ahead = force.0 * fwd.0 + force.1 * fwd.1 < 0.0;
    let rudder = rudder_command(force, subj_heading, ahead, p) * prep;
    let yaw = nomoto_step(prev_yaw, rudder, p);
    SteerResult {
        yaw,
        new_heading: (subj_heading + yaw).rem_euclid(360.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ForceFieldParams {
        ForceFieldParams {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn repulsion_decays_with_distance_and_scales_with_k() {
        let p = params();
        let near = resultant_force((0.0, 0.0), 0.0, &[Obstacle { position: (0.0, 1.0), heading_deg: 180.0 }], &p);
        let far = resultant_force((0.0, 0.0), 0.0, &[Obstacle { position: (0.0, 4.0), heading_deg: 180.0 }], &p);
        // Both obstacles are dead ahead (north); force points south (−y).
        assert!(near.1 < 0.0 && far.1 < 0.0);
        assert!(near.1.abs() > far.1.abs(), "closer obstacle repels harder");

        // Doubling k_obst doubles the magnitude.
        let mut p2 = p;
        p2.k_obst *= 2.0;
        let near2 = resultant_force((0.0, 0.0), 0.0, &[Obstacle { position: (0.0, 1.0), heading_deg: 180.0 }], &p2);
        assert!((near2.1 - 2.0 * near.1).abs() < 1e-9);
    }

    #[test]
    fn obstacle_beyond_activation_is_ignored() {
        let p = params(); // overtaking activation = 2 nm
        // Same-heading (overtaking) obstacle 3 nm ahead → beyond 2 nm reach.
        let f = resultant_force((0.0, 0.0), 0.0, &[Obstacle { position: (0.0, 3.0), heading_deg: 0.0 }], &p);
        assert_eq!(f, (0.0, 0.0), "overtaking obstacle past d_s exerts no force");
        // A head-on obstacle at the same range is within the 8 nm head-on reach.
        let f2 = resultant_force((0.0, 0.0), 0.0, &[Obstacle { position: (0.0, 3.0), heading_deg: 180.0 }], &p);
        assert!(f2.1 < 0.0, "head-on obstacle within its longer reach repels");
    }

    #[test]
    fn head_on_resolves_to_starboard() {
        // Subject heads north (0°); an opposing ship is dead ahead heading south.
        // The encounter is symmetric (no lateral force), so the starboard bias
        // must turn the subject to starboard (heading increases toward east).
        let p = params();
        let obstacles = [Obstacle { position: (0.0, 5.0), heading_deg: 180.0 }];
        let r = steer((0.0, 0.0), 0.0, 0.0, &obstacles, 1.0, &p);
        assert!(r.yaw > 0.0, "yaw is to starboard, got {}", r.yaw);
        assert!(
            r.new_heading > 0.0 && r.new_heading < 90.0,
            "subject turns toward starboard (east), heading={}",
            r.new_heading
        );
    }

    #[test]
    fn fatigue_weakens_the_avoidance() {
        let p = params();
        let obstacles = [Obstacle { position: (0.0, 5.0), heading_deg: 180.0 }];
        let fresh = steer((0.0, 0.0), 0.0, 0.0, &obstacles, 1.0, &p);
        let tired = steer((0.0, 0.0), 0.0, 0.0, &obstacles, 0.3, &p);
        assert!(
            tired.yaw < fresh.yaw,
            "a fatigued crew turns less: tired={} fresh={}",
            tired.yaw,
            fresh.yaw
        );
    }

    #[test]
    fn nomoto_lags_toward_steady_state() {
        let p = params();
        // With constant rudder the yaw rate approaches K'·rudder monotonically.
        let target = p.nomoto_gain * 10.0;
        let y1 = nomoto_step(0.0, 10.0, &p);
        let y2 = nomoto_step(y1, 10.0, &p);
        assert!(y1 > 0.0 && y2 > y1, "yaw builds up over ticks");
        assert!(y2 < target + 1e-9, "never overshoots the steady-state turn rate");
    }

    #[test]
    fn no_obstacles_means_no_turn() {
        let p = params();
        let r = steer((0.0, 0.0), 45.0, 0.0, &[], 1.0, &p);
        assert!((r.yaw).abs() < 1e-12, "no obstacles → no rudder → no yaw");
        assert!((r.new_heading - 45.0).abs() < 1e-12);
    }

    #[test]
    fn yaw_decays_when_force_clears() {
        // A residual yaw with no obstacles relaxes back toward zero.
        let p = params();
        let r = steer((0.0, 0.0), 0.0, 4.0, &[], 1.0, &p);
        assert!(r.yaw < 4.0 && r.yaw > 0.0, "yaw relaxes toward zero, got {}", r.yaw);
    }
}
