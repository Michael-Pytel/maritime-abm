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
}

impl RescuePhase {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            RescuePhase::Mobilising => "mobilising",
            RescuePhase::Transiting => "transiting",
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
    /// Tick at which the asset was dispatched (for Time-to-Arrival).
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
        }
    }
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
