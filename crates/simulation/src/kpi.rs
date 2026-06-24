use crate::scenario::Method;
use crate::vessel::{VesselAgent, VesselState};
use crate::weather::WeatherField;
use rand::Rng;
use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Awareness factor used in `P_prep` computation.
///   `BaselineA` → 2.0  (weather-unaware crew; hazard hits twice as hard)
///   `BaselineB` → 1.3  (partially aware; limited benefit)
///   `ProposedSystem` → 1.0 (full awareness; hazard mitigated by preparation)
fn awareness_factor(method: Method) -> f64 {
    match method {
        Method::BaselineA => 2.0,
        Method::BaselineB => 1.3,
        Method::ProposedSystem => 1.0,
    }
}

// ── Accumulator ───────────────────────────────────────────────────────────────

/// Running totals updated each tick.
#[derive(Debug, Default)]
pub struct KpiAccumulator {
    pub ship_hrs: f64,

    /// Number of hard collision events (both vessels within `collision_trigger_field`).
    pub collision_events: u64,

    /// Total vessel spawns (for lifetime accounting).
    pub total_spawns: u64,

    /// Number of evacuation activations triggered by collisions.
    pub evac_events: u64,

    /// Total crew fatalities across all collisions.
    pub fatal_crew: u64,

    /// Total crew present on active vessels at the moment a collision occurred.
    /// Used to compute `survival_ratio`.
    pub crew_exposed: u64,

    /// Sum of TTA (ticks) from every issued collision warning.
    pub tta_ticks_sum: f64,

    /// Number of collision warnings issued (denominator for avg TTA).
    pub tta_count: u64,

    /// Running sum of per-vessel `P_prep` values accumulated across all ticks.
    pub p_prep_sum: f64,

    /// Total vessel-ticks contributing to `p_prep` (denominator).
    pub p_prep_vessel_ticks: u64,
}

impl KpiAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ── Per-tick accumulation ────────────────────────────────────────────────

    /// Accumulate ship-hours for every active vessel this tick (1 tick = 0.25 h).
    pub fn record_tick(&mut self, vessels: &[VesselAgent]) {
        const DT: f64 = 0.25;
        for v in vessels {
            if v.state == VesselState::Active {
                self.ship_hrs += DT;
            }
        }
    }

    /// Record the mean `P_prep` for this tick, evaluated for every active vessel.
    ///
    /// `P_prep` = probability that the crew is adequately prepared for an emergency,
    /// given their local weather hazard, situational awareness, and fatigue level.
    ///
    /// Formula:
    ///   `p_weather`  = max(0,  1 − `awareness_factor` · W)
    ///   `p_fatigue`  = max(0,  1 − 0.70 · fatigue)
    ///   `p_prep`     = `p_weather` · `p_fatigue`
    ///
    /// The two factors are independent: weather-unaware and exhausted crew are
    /// doubly penalised.
    pub fn record_p_prep_tick(
        &mut self,
        vessels: &[VesselAgent],
        weather: &WeatherField,
        world_w: f64,
        world_h: f64,
        method: Method,
    ) {
        let af = awareness_factor(method);
        for v in vessels {
            if v.state != VesselState::Active {
                continue;
            }
            let w = weather.hazard_at(v.position.0, v.position.1, world_w, world_h);
            let p_weather = (1.0 - af * w).max(0.0);
            let p_fatigue = (1.0 - 0.70 * v.fatigue).max(0.0);
            self.p_prep_sum += p_weather * p_fatigue;
            self.p_prep_vessel_ticks += 1;
        }
    }

    // ── Event recording ──────────────────────────────────────────────────────

    /// Record a raw collision event (vessels entered physical overlap).
    pub fn record_collision(&mut self) {
        self.collision_events += 1;
    }

    /// Record a new vessel spawned into the fleet.
    pub fn record_spawn(&mut self) {
        self.total_spawns += 1;
    }

    /// Record a collision warning being issued with a predicted TTA in ticks.
    pub fn record_collision_warning(&mut self, tta_ticks: u64) {
        #[allow(clippy::cast_precision_loss)]
        let tta_f64 = tta_ticks as f64;
        self.tta_ticks_sum += tta_f64;
        self.tta_count += 1;
    }

    /// Record the casualty outcome for **one struck vessel** in a hard
    /// collision, using the SIMCOL survival factor `S_i`.
    ///
    /// Expected fatalities are `crew · (1 − S_i)`, drawn per crew member as a
    /// Bernoulli trial so the running total is unbiased. A foundering vessel
    /// (`founders`) registers an evacuation activation and enters the SAR chain
    /// (handled by the caller). Returns the number of fatalities drawn.
    ///
    /// Arguments:
    ///   `crew`             — crew count on the struck vessel.
    ///   `survival_factor`  — SIMCOL `S_i ∈ [0, 1]` for this struck vessel.
    ///   `founders`         — whether the struck vessel founders.
    ///   `rng`              — simulation RNG for stochastic draws.
    pub fn record_struck_outcome(
        &mut self,
        crew: u32,
        survival_factor: f64,
        founders: bool,
        rng: &mut impl Rng,
    ) -> u32 {
        self.crew_exposed += u64::from(crew);

        let p_fatal = (1.0 - survival_factor).clamp(0.0, 1.0);
        let mut fatalities = 0u32;
        for _ in 0..crew {
            if rng.gen::<f64>() < p_fatal {
                fatalities += 1;
            }
        }
        self.fatal_crew += u64::from(fatalities);

        if founders {
            self.evac_events += 1;
        }
        fatalities
    }

    // ── Snapshot ─────────────────────────────────────────────────────────────

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn snapshot(&self, step: u64) -> KpiSnapshot {
        let ship_hrs = self.ship_hrs.max(1e-9);

        let collision_per_1k_hrs = self.collision_events as f64 / ship_hrs * 1000.0;
        let fatal_per_1k_hrs = self.fatal_crew as f64 / ship_hrs * 1000.0;
        let evac_activation_rate = self.evac_events as f64 / ship_hrs * 1000.0;

        let survival_ratio = if self.crew_exposed == 0 {
            1.0
        } else {
            1.0 - self.fatal_crew as f64 / self.crew_exposed as f64
        };

        let avg_tta_hours = if self.tta_count == 0 {
            0.0
        } else {
            // 1 tick = 0.25 hours
            (self.tta_ticks_sum / self.tta_count as f64) * 0.25
        };

        let mean_p_prep = if self.p_prep_vessel_ticks == 0 {
            0.0
        } else {
            self.p_prep_sum / self.p_prep_vessel_ticks as f64
        };

        KpiSnapshot {
            step,
            fatal_per_1k_hrs,
            collision_per_1k_hrs,
            survival_ratio,
            avg_tta_hours,
            evac_activation_rate,
            mean_p_prep,
        }
    }
}

// ── Snapshot type (serialised over WebSocket / logs) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiSnapshot {
    pub step: u64,
    pub fatal_per_1k_hrs: f64,
    pub collision_per_1k_hrs: f64,
    pub survival_ratio: f64,
    pub avg_tta_hours: f64,
    pub evac_activation_rate: f64,
    pub mean_p_prep: f64,
}
