//! Search-and-rescue (SAR) subsystem.
//!
//! When a collision founders a vessel (SIMCOL `S_i` below the founder
//! threshold, see [`crate::simcol`]) the survivors enter a liferaft and a
//! rescue asset is dispatched from the nearest shore base. The asset is a
//! helicopter (fast) or a patrol vessel (slow) depending on how far offshore
//! the datum lies; after a mobilisation delay it transits to the datum and, on
//! arrival, recovers the survivors — unless the liferaft survival time has
//! elapsed, in which case the case becomes a loss.
//!
//! Phase 3 models deterministic arrival (the datum is the foundered vessel's
//! position). Phase 4 (MASSIM) adds liferaft drift and random-search detection;
//! Phase 5 (Ashrafi) degrades the transit/boarding speeds with the environment.

use serde::{Deserialize, Serialize};

/// Rescue asset type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescueKind {
    Helicopter,
    Patrol,
}

impl RescueKind {
    /// Transit speed (knots) for this asset.
    #[must_use]
    pub fn speed_kn(self, p: &SarParams) -> f64 {
        match self {
            RescueKind::Helicopter => p.helicopter_speed_kn,
            RescueKind::Patrol => p.patrol_speed_kn,
        }
    }

    /// Short label for telemetry.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            RescueKind::Helicopter => "helicopter",
            RescueKind::Patrol => "patrol",
        }
    }
}

/// Phase of a dispatched rescue asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescuePhase {
    /// Still at base, counting down the mobilisation delay.
    Mobilising,
    /// En route to the datum.
    Transiting,
    /// On scene, running a random search over the drifting datum.
    Searching,
    /// Survivors located; embarking them under the season-degraded boarding rate.
    Embarking,
}

impl RescuePhase {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            RescuePhase::Mobilising => "mobilising",
            RescuePhase::Transiting => "transiting",
            RescuePhase::Searching => "searching",
            RescuePhase::Embarking => "embarking",
        }
    }
}

/// A dispatched rescue asset.
#[derive(Debug, Clone)]
pub struct RescueAgent {
    pub id: u64,
    pub kind: RescueKind,
    /// Current position in field coordinates (nm).
    pub position: (f64, f64),
    /// Home shore base, in field coordinates.
    pub base: (f64, f64),
    /// Id of the foundered vessel (datum) this asset is dispatched to.
    pub target_vessel_id: u64,
    pub phase: RescuePhase,
    /// Mobilisation ticks remaining before transit begins.
    pub mobilising_ticks_left: u32,
    /// Boarding ticks remaining once survivors have been located.
    pub embark_ticks_left: u32,
    /// Tick at which the asset was dispatched (for Time-to-Arrival).
    pub dispatch_tick: u64,
}

/// A single person in the water following a man-overboard (MOB) event.
///
/// Each person is an independent search target (MASSIM single-MOB model,
/// Karatas et al. 2018): they drift on their own bearing and are detected by
/// their own per-tick Koopman draw, so a search may recover some of an
/// incident's people before others.
#[derive(Debug, Clone)]
pub struct MobPerson {
    pub id: u64,
    /// Collision incident this person belongs to (one searcher is dispatched
    /// per incident cluster).
    pub incident_id: u64,
    /// Current position in field coordinates (nm).
    pub position: (f64, f64),
    /// Independent drift bearing (compass degrees) for this person.
    pub drift_bearing_deg: f64,
    /// Tick at which the person entered the water (for the survival window).
    pub since_tick: u64,
}

/// A searcher dispatched to a man-overboard incident cluster. Unlike a
/// liferaft [`RescueAgent`] it has no embarking phase — it recovers persons
/// individually as its per-tick search detects them.
#[derive(Debug, Clone)]
pub struct MobSearchAgent {
    pub id: u64,
    pub kind: RescueKind,
    /// Current position in field coordinates (nm).
    pub position: (f64, f64),
    /// Home shore base.
    pub base: (f64, f64),
    /// Incident cluster this searcher is tasked to.
    pub incident_id: u64,
    pub phase: RescuePhase,
    pub mobilising_ticks_left: u32,
    pub dispatch_tick: u64,
}

/// Tunable SAR parameters (overridable via `SimConfig`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SarParams {
    /// Ticks between SOS and an asset beginning its transit.
    pub mobilisation_delay_ticks: u32,
    /// Helicopter transit speed (knots).
    pub helicopter_speed_kn: f64,
    /// Patrol-vessel transit speed (knots).
    pub patrol_speed_kn: f64,
    /// Datum distance (nm) beyond which a helicopter is dispatched instead of a
    /// patrol vessel (helicopters cover open-water datums faster).
    pub helicopter_min_datum_nm: f64,
    /// Ticks survivors last in the liferaft before the case becomes a loss.
    pub liferaft_survival_ticks: u32,
    /// Distance (nm) within which an asset is considered to have reached the datum.
    pub arrival_radius_nm: f64,

    // ── MASSIM random-search detection (Koopman) ─────────────────────────────
    /// Number of cooperating searchers per datum.
    pub searchers: u32,
    /// Searcher sweep width (nm).
    pub sweep_width_nm: f64,
    /// Initial datum uncertainty radius (nm) at the moment of the incident.
    pub initial_uncertainty_nm: f64,
    /// Liferaft drift speed (nm/tick) — also grows the search area each tick.
    pub drift_speed_nm_per_tick: f64,
    /// Prevailing drift bearing (compass degrees) the liferaft is advected along.
    pub drift_bearing_deg: f64,

    // ── Man-overboard (person-in-water) search ───────────────────────────────
    /// Ticks a person survives in the water before the case becomes a fatality.
    /// Far shorter than the liferaft window — cold-water immersion (Karatas
    /// et al. 2018; Ashrafi et al. 2024 cold-environment framing).
    pub mob_survival_ticks: u32,
    /// Radius (nm) over which an incident's casualties are initially scattered
    /// around the collision point, giving each person a distinct datum.
    pub mob_scatter_nm: f64,
}

impl Default for SarParams {
    fn default() -> Self {
        Self {
            mobilisation_delay_ticks: 2,
            helicopter_speed_kn: 80.0,
            patrol_speed_kn: 25.0,
            helicopter_min_datum_nm: 40.0,
            liferaft_survival_ticks: 96, // 24 h at 15 min/tick
            arrival_radius_nm: 1.0,
            searchers: 1,
            sweep_width_nm: 5.0,
            initial_uncertainty_nm: 2.0,
            drift_speed_nm_per_tick: 0.2, // ≈ 0.8 kn current
            drift_bearing_deg: 45.0,
            mob_survival_ticks: 12, // ≈ 3 h cold-water immersion
            mob_scatter_nm: 0.5,
        }
    }
}

/// Per-tick Koopman random-search detection probability.
///
/// The datum uncertainty grows with the response latency: with initial radius
/// `r0` and drift speed `u_d`, the search area after `elapsed_ticks` is
/// `A = π(r0 + u_d·τ)²`. A searcher of sweep width `w` covering `v_s·Δt` per
/// tick yields per-tick coverage `dC = n·w·v_s·Δt / A` and detection
/// probability `1 − e^{−dC}` (Koopman; MASSIM). The longer the response is
/// delayed, the larger `A` and the harder the detection — the quantitative
/// link from response latency to survival.
#[must_use]
pub fn detection_prob_tick(p: &SarParams, searcher_speed_kn: f64, elapsed_ticks: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let tau = elapsed_ticks as f64;
    let radius = p.initial_uncertainty_nm + p.drift_speed_nm_per_tick * tau;
    let area = (std::f64::consts::PI * radius * radius).max(1e-9);
    let vs_nm = searcher_speed_kn * 0.25; // nm covered per tick
    let coverage = f64::from(p.searchers) * p.sweep_width_nm * vs_nm / area;
    1.0 - (-coverage).exp()
}

/// Select the rescue asset for a datum at `datum_distance_nm` from its base:
/// a helicopter for open-water datums, a patrol vessel close to shore.
#[must_use]
pub fn select_asset(datum_distance_nm: f64, p: &SarParams) -> RescueKind {
    if datum_distance_nm > p.helicopter_min_datum_nm {
        RescueKind::Helicopter
    } else {
        RescueKind::Patrol
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_probability_is_bounded() {
        let p = SarParams::default();
        for elapsed in [0u64, 1, 10, 50, 200] {
            let d = detection_prob_tick(&p, 25.0, elapsed);
            assert!((0.0..=1.0).contains(&d), "detection prob out of range");
        }
    }

    #[test]
    fn detection_decays_as_response_is_delayed() {
        // A growing datum (later response → larger search area) is harder to
        // detect per tick: the latency → survival link.
        let p = SarParams::default();
        let early = detection_prob_tick(&p, 25.0, 1);
        let late = detection_prob_tick(&p, 25.0, 100);
        assert!(late < early, "later response must lower per-tick detection");
    }

    #[test]
    fn faster_searcher_detects_better() {
        let p = SarParams::default();
        let patrol = detection_prob_tick(&p, p.patrol_speed_kn, 10);
        let helo = detection_prob_tick(&p, p.helicopter_speed_kn, 10);
        assert!(helo > patrol, "a faster searcher covers more area per tick");
    }
}
