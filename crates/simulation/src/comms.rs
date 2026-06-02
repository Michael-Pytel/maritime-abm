/// Vessel-to-vessel collision-avoidance communication.
///
/// Each simulation tick the `PostTickAgent` scans active vessel pairs.  When
/// two ships are within `collision_warn_radius_nm` AND their Closest Point of
/// Approach (CPA) is predicted to be dangerously close within
/// `collision_warn_tta_max_ticks`, a three-message conversation is logged and
/// the give-way vessel has an avoidance waypoint injected into its route.
///
/// # Give-way rule
/// The vessel with the lower `id` gives way (turns to starboard); the other
/// maintains course and speed.  This is a simplified, deterministic rule —
/// close enough to COLREGS crossing / head-on behaviour for the simulation.
use serde::Serialize;

/// Lateral distance (nm) added to the give-way vessel's current track
/// (used when no config override is supplied; the live value comes from `SimConfig`).
pub const AVOIDANCE_OFFSET_NM: f64 = 5.0;
/// Distance ahead (nm) at which the avoidance waypoint is placed
/// (used when no config override is supplied; the live value comes from `SimConfig`).
pub const AVOIDANCE_FORWARD_NM: f64 = 3.0;

// ── Message types ─────────────────────────────────────────────────────────────

/// A single message in a vessel-to-vessel collision-avoidance conversation.
#[derive(Debug, Clone, Serialize)]
pub struct VesselMsg {
    pub tick: u64,
    pub from_id: u64,
    pub from_name: String,
    pub to_id: u64,
    pub to_name: String,
    pub kind: MsgKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MsgKind {
    /// Stand-on vessel alerts give-way vessel: collision predicted.
    CollisionWarning { cpa_nm: f64, tta_ticks: u64 },
    /// Give-way vessel confirms it is altering course to starboard.
    GivingWay,
    /// Stand-on vessel confirms it is maintaining course and speed.
    MaintainingCourse,
    /// Give-way vessel reports it has completed the avoidance manoeuvre.
    ResumeRoute,
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Returns `(cpa_nm, tta_ticks)` — closest point of approach distance (nm) and
/// the time in simulation ticks until that point is reached.
///
/// `pos_*` and `vel_*` are in field-coordinate nautical miles and nm/tick
/// respectively.  Returns `(current_distance, 0.0)` when vessels are already
/// diverging and `(current_distance, f64::MAX)` when on parallel courses.
pub fn compute_cpa(
    pos_a: (f64, f64),
    vel_a: (f64, f64),
    pos_b: (f64, f64),
    vel_b: (f64, f64),
) -> (f64, f64) {
    let dp = (pos_b.0 - pos_a.0, pos_b.1 - pos_a.1);
    let dv = (vel_b.0 - vel_a.0, vel_b.1 - vel_a.1);
    let dv_sq = dv.0 * dv.0 + dv.1 * dv.1;

    if dv_sq < 1e-9 {
        // Parallel / stationary — CPA is current distance, TTA is undefined.
        let d = (dp.0 * dp.0 + dp.1 * dp.1).sqrt();
        return (d, f64::MAX);
    }

    let t = -(dp.0 * dv.0 + dp.1 * dv.1) / dv_sq;
    if t <= 0.0 {
        // Already past CPA — vessels are diverging.
        let d = (dp.0 * dp.0 + dp.1 * dp.1).sqrt();
        return (d, 0.0);
    }

    let cpx = dp.0 + t * dv.0;
    let cpy = dp.1 + t * dv.1;
    let cpa = (cpx * cpx + cpy * cpy).sqrt();
    (cpa, t)
}

/// Returns the field-coordinate velocity vector `(vx, vy)` in nm/tick.
///
/// Compass heading: 0 = north, 90 = east.  Field coords: x = east, y = north.
pub fn vessel_velocity(heading_deg: f64, speed_kn: f64) -> (f64, f64) {
    let nm_per_tick = speed_kn * 0.25; // 1 tick = 15 min = 0.25 h
    let h = heading_deg.to_radians();
    (nm_per_tick * h.sin(), nm_per_tick * h.cos())
}

/// Computes an avoidance waypoint for the give-way vessel.
///
/// Places it `forward_nm` ahead and `offset_nm` to starboard of the vessel's
/// current position and compass heading.
///
/// Starboard direction for compass heading θ: x = cos θ, y = −sin θ.
pub fn avoidance_waypoint(
    pos: (f64, f64),
    heading_deg: f64,
    offset_nm: f64,
    forward_nm: f64,
) -> (f64, f64) {
    let h = heading_deg.to_radians();
    // Forward:   x = sin h,  y = cos h
    // Starboard: x = cos h,  y = −sin h
    let fwd_x = h.sin() * forward_nm;
    let fwd_y = h.cos() * forward_nm;
    let stbd_x = h.cos() * offset_nm;
    let stbd_y = -h.sin() * offset_nm;
    (pos.0 + fwd_x + stbd_x, pos.1 + fwd_y + stbd_y)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_on_collision_detected() {
        // A heads north at 3.75 nm/tick, B heads south at 3.75 nm/tick,
        // separated by 10 nm.  CPA ≈ 0, TTA ≈ 10/7.5 ≈ 1.33 ticks.
        let vel_a = vessel_velocity(0.0, 15.0);   // north
        let vel_b = vessel_velocity(180.0, 15.0); // south
        let (cpa, tta) = compute_cpa((0.0, 0.0), vel_a, (0.0, 10.0), vel_b);
        assert!(cpa < 0.01, "head-on CPA should be ≈0, got {cpa:.4}");
        assert!((tta - 10.0 / 7.5).abs() < 0.01, "TTA should be ≈1.33 ticks, got {tta:.4}");
    }

    #[test]
    fn same_velocity_vessels_return_max_tta() {
        // A and B have identical velocity — dv = 0, so TTA is undefined (MAX).
        // The detection code filters MAX as "no approaching threat", which is correct.
        let vel = vessel_velocity(0.0, 15.0);
        let (_, tta) = compute_cpa((0.0, 0.0), vel, (0.0, 5.0), vel);
        assert_eq!(tta, f64::MAX, "identical velocities → dv=0 → TTA = MAX (undefined)");
    }

    #[test]
    fn already_past_cpa_returns_zero_tta() {
        // A moves east, B is west of A but moving faster east — B is pulling away.
        let vel_a = vessel_velocity(90.0, 5.0);  // 1.25 nm/tick east
        let vel_b = vessel_velocity(90.0, 15.0); // 3.75 nm/tick east, already ahead
        // B starts at (10, 0) moving faster east than A at (0, 0) — diverging.
        let (_, tta) = compute_cpa((0.0, 0.0), vel_a, (10.0, 0.0), vel_b);
        assert_eq!(tta, 0.0, "faster vessel ahead → already past CPA → TTA = 0");
    }

    #[test]
    fn avoidance_wp_is_to_starboard() {
        // Vessel heading north (0°): starboard is east (+x).
        let (wx, wy) = avoidance_waypoint((0.0, 0.0), 0.0, AVOIDANCE_OFFSET_NM, AVOIDANCE_FORWARD_NM);
        assert!(wx > 0.0, "avoidance wp should be east of northbound vessel");
        assert!(wy > 0.0, "avoidance wp should be ahead (north) of vessel");
    }

    #[test]
    fn avoidance_wp_east_heading_starboard_is_south() {
        // Vessel heading east (90°): starboard is south (−y).
        let (wx, wy) = avoidance_waypoint((0.0, 0.0), 90.0, AVOIDANCE_OFFSET_NM, AVOIDANCE_FORWARD_NM);
        assert!(wx > 0.0, "avoidance wp should be ahead (east) of eastbound vessel");
        assert!(wy < 0.0, "avoidance wp should be south (starboard) of eastbound vessel");
    }
}
