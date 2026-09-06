use crate::weather::WeatherPreset;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scenario {
    #[default]
    CalmPassage,
    StormCorridor,
    BlindShore,
    DeepWaterRescue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Method {
    BaselineA,
    BaselineB,
    #[default]
    ProposedSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    // --- experiment identity ---
    pub scenario: Scenario,
    pub method: Method,
    pub seed: u64,

    // --- fleet ---
    pub n_vessels: u32,
    pub n_crew_per_vessel: u32,

    // --- time ---
    pub n_ticks: u32,

    // --- physics ---
    pub collision_radius_nm: f64,
    /// Within this distance (nm) of any port, encounters are not counted as
    /// collisions (harbour-approach / VTS-controlled water).
    pub port_collision_exclusion_nm: f64,

    // --- same-destination following (anti-overtake / queueing) ---
    /// Two vessels follow each other when within this vicinity (nm) and bound
    /// for the same destination; the trailing one matches the leader's speed.
    pub follow_vicinity_nm: f64,
    /// Closer than this separation (nm) the follower holds (anchors) to keep
    /// distance behind its leader.
    pub follow_keep_distance_nm: f64,
    /// Two vessels share a destination when their route endpoints (in the
    /// travel direction) are within this distance (nm) of each other.
    pub same_destination_nm: f64,

    /// SIMCOL collision-consequence surrogate parameters.
    #[serde(default)]
    pub simcol: crate::simcol::SimcolParams,

    /// Search-and-rescue subsystem parameters.
    #[serde(default)]
    pub sar: crate::sar::SarParams,

    /// Optional artificial-force-field avoidance track (default off).
    #[serde(default)]
    pub forcefield: crate::forcefield::ForceFieldParams,

    /// Calendar month (1–12) driving Ashrafi seasonal SAR degradation.
    #[serde(default = "default_sim_month")]
    pub sim_month: u8,

    // --- port / voyage model ---
    pub port_dwell_min_ticks: u32,
    pub port_dwell_max_ticks: u32,
    pub initial_dwell_spread_ticks: u32,
    pub port_dedup_radius_nm: f64,

    // --- collision-avoidance comms ---
    /// Radius within which two vessels are checked for CPA (nm).
    pub collision_warn_radius_nm: f64,
    /// Trigger avoidance if predicted CPA is closer than this (nm).
    pub collision_warn_cpa_nm: f64,
    /// Trigger avoidance only if TTA is within this many ticks.
    pub collision_warn_tta_max_ticks: u32,
    /// Ticks before the give-way vessel accepts a new avoidance manoeuvre.
    pub avoidance_cooldown_ticks: u32,
    /// How far to starboard (nm) the avoidance waypoint is offset.
    /// In poor-weather/comms scenarios this can be reduced to model degraded
    /// manoeuvring confidence.
    pub avoidance_offset_nm: f64,
    /// How far ahead (nm) the avoidance waypoint is placed.
    pub avoidance_forward_nm: f64,
    /// Probability [0, 1] that a CPA warning is successfully communicated.
    /// 1.0 = perfect weather; lower values model comms degradation.
    pub comms_success_rate: f64,

    // --- weather field ---
    /// Controls initial regime and bias of the 50×50 stochastic weather grid.
    pub weather_preset: WeatherPreset,

    // --- storm zone (used by StormCorridor; disabled for other scenarios) ---
    /// Whether a storm zone is active this run.
    pub storm_enabled: bool,
    /// Storm radius (nm).  Vessels inside receive degraded comms.
    pub storm_radius_nm: f64,
    /// Effective `comms_success_rate` while a vessel is inside the storm.
    pub storm_comms_success_rate: f64,
    /// Speed multiplier applied to vessels navigating inside the storm (0–1).
    pub storm_speed_factor: f64,
    /// How far the storm centre moves per tick (nm) toward the next waypoint.
    pub storm_drift_nm_per_tick: f64,
    /// Ordered list of `[lat, lon]` waypoints the storm tracks through.
    /// The storm spawns at index 0 and advances toward each successive point.
    /// An empty list means the storm is stationary (position = first snapshot).
    pub storm_track: Vec<[f64; 2]>,

    // --- data paths ---
    pub ais_path: String,
    pub ports_path: String,
    pub land_mask_path: String,
    pub log_path: Option<String>,

    // --- run control ---
    pub loop_mode: bool,
    /// 0 = disabled, N = write JSONL every N ticks.
    pub log_every_n_ticks: u32,
    /// 0 = disabled, N = broadcast WebSocket snapshot every N ticks.
    pub snapshot_every_n_ticks: u32,

    // --- pre-computed field-unit values ---
    #[serde(skip)]
    pub world_width_nm: f64,
    #[serde(skip)]
    pub world_height_nm: f64,
    #[serde(skip)]
    pub collision_trigger_field: f64,
}

/// Default calendar month for `SimConfig::sim_month` (July → Ashrafi Group A,
/// the mildest SAR conditions, i.e. no seasonal speed penalty by default).
fn default_sim_month() -> u8 {
    7
}

impl Default for SimConfig {
    fn default() -> Self {
        let mut cfg = Self {
            scenario: Scenario::default(),
            method: Method::default(),
            seed: 0,
            // Concurrent underway default ≈ Geomatics Baltic AIS average
            // (≈774 moving ships); scenario presets override below.
            n_vessels: 750,
            // Typical merchant minimum safe manning is ~15–25; we use 20 as a
            // fleet-mean exposure count for overboard Bernoulli draws (IMO
            // Res. A.1047(27) principles; cargo/tanker practice).
            n_crew_per_vessel: 20,
            // Default horizon = Calm exposure (14 d at 5 min/tick). Scenarios
            // override: Storm ~4 d, Blind/Deep 7 d.
            n_ticks: crate::time::ticks_for_days(14.0),
            // Hull-contact radius at concurrent underway density: hard hit at
            // 2·r = 0.024 nm (~45 m, beam-scale). Wider CPA avoidance (below)
            // carries most COLREGs separation; the thin contact disc keeps
            // Calm BaselineA near the 1.0–2.5 / 1k ship-hrs calibration band.
            collision_radius_nm: 0.012,
            port_collision_exclusion_nm: 2.5,
            follow_vicinity_nm: 3.0,
            follow_keep_distance_nm: 0.5,
            same_destination_nm: 5.0,
            simcol: crate::simcol::SimcolParams::default(),
            sar: crate::sar::SarParams::default(),
            forcefield: crate::forcefield::ForceFieldParams::default(),
            sim_month: default_sim_month(),
            // Port stay 2–12 h (was 8–48 ticks at 15 min).
            port_dwell_min_ticks: crate::time::ticks_for_hours(2.0),
            port_dwell_max_ticks: crate::time::ticks_for_hours(12.0),
            initial_dwell_spread_ticks: crate::time::ticks_for_hours(4.0),
            port_dedup_radius_nm: 8.0,
            // Perfect-weather COLREGS baseline: wide detection, generous berth.
            // Tuned with N≈750 at 15 min; TTA/cooldown scaled to 5 min so the
            // physical 4 h / 10 min gates are unchanged.
            collision_warn_radius_nm: 25.0,
            collision_warn_cpa_nm: 8.0,
            collision_warn_tta_max_ticks: crate::time::ticks_for_hours(4.0),
            avoidance_cooldown_ticks: crate::time::ticks_for_hours(10.0 / 60.0),
            avoidance_offset_nm: 9.0,
            avoidance_forward_nm: 5.0,
            comms_success_rate: 1.0,
            weather_preset: WeatherPreset::Mixed,
            // Storm zone — disabled by default; StormCorridor overrides below.
            storm_enabled: false,
            storm_radius_nm: 120.0,
            storm_comms_success_rate: 0.30,
            storm_speed_factor: 0.70,
            // Placeholder; StormCorridor sets a ~3-day basin transit.
            storm_drift_nm_per_tick: crate::time::nm_per_tick(1.2),
            storm_track: vec![],
            ais_path: "data/ais_paths.json".into(),
            ports_path: "data/ports.json".into(),
            land_mask_path: "data/coastline.geojson".into(),
            log_path: None,
            loop_mode: false,
            log_every_n_ticks: 1,
            snapshot_every_n_ticks: 1,
            world_width_nm: 0.0,
            world_height_nm: 0.0,
            collision_trigger_field: 0.0,
        };
        cfg.compute_field_units();
        cfg
    }
}

impl SimConfig {
    pub fn compute_field_units(&mut self) {
        self.world_width_nm = crate::ais::field_width_nm();
        self.world_height_nm = crate::ais::field_height_nm();
        self.collision_trigger_field = 2.0 * self.collision_radius_nm;
    }

    #[must_use]
    pub fn for_scenario(scenario: Scenario, method: Method, seed: u64) -> Self {
        let mut cfg = Self {
            scenario,
            method,
            seed,
            ..Self::default()
        };
        match scenario {
            Scenario::CalmPassage => {
                // Concurrent underway fleet aligned with recent Baltic AIS:
                // ~774 moving ships on average (Geomatics 2025). Fair-weather
                // calibration arm over a 14-day exposure window (not a claim of
                // 14 unbroken meteorological calm days).
                cfg.n_vessels = 750;
                cfg.n_ticks = crate::time::ticks_for_days(14.0);
                cfg.weather_preset = WeatherPreset::Calm;
            }
            Scenario::StormCorridor => {
                // Event-scale storm stress: ~4 simulated days with a translating
                // cell that crosses Channel→Gdańsk (~860 nm) in ≈3 days ≈12 kn.
                cfg.n_vessels = 900;
                cfg.n_ticks = crate::time::ticks_for_days(4.0);
                cfg.weather_preset = WeatherPreset::Stormy;
                cfg.storm_enabled = true;
                cfg.storm_radius_nm = 160.0;
                cfg.storm_comms_success_rate = 0.08;
                cfg.storm_speed_factor = 0.45;
                cfg.storm_drift_nm_per_tick = crate::time::nm_per_tick(12.0);
                // Track: English Channel → Southern North Sea → Skagerrak
                //        → Kattegat → Western Baltic → Gdańsk
                cfg.storm_track = vec![
                    [51.0, 2.0],  // English Channel (spawn point)
                    [53.5, 4.5],  // Southern North Sea, off Netherlands coast
                    [57.0, 9.5],  // Skagerrak entrance
                    [56.5, 12.0], // Kattegat
                    [55.5, 15.0], // Western Baltic, south of Gotland
                    [54.4, 18.6], // Gdańsk Bay (terminus)
                ];
            }
            Scenario::BlindShore => {
                // Coordination is degraded everywhere, not just inside a storm:
                // away from dense, well-covered shipping lanes the shore VHF /
                // AIS picture is patchy, so warning exchanges frequently fail.
                cfg.n_vessels = 900;
                cfg.n_ticks = crate::time::ticks_for_days(7.0);
                cfg.comms_success_rate = 0.55;
                cfg.avoidance_offset_nm = 3.0;
            }
            Scenario::DeepWaterRescue => {
                // Week-long offshore SAR stress: distant bases, winter Ashrafi D,
                // slowed assets vs the fixed MOB survival window.
                cfg.n_vessels = 500;
                cfg.n_ticks = crate::time::ticks_for_days(7.0);
                cfg.sim_month = 1; // January → Ashrafi Group D
                cfg.sar.mobilisation_delay_ticks = crate::time::ticks_for_hours(1.0);
                cfg.sar.helicopter_speed_kn = 55.0;
                cfg.sar.patrol_speed_kn = 16.0;
            }
        }

        // ── Method overrides ──────────────────────────────────────────────────
        // Applied after scenario settings so they cleanly override any
        // scenario-specific weather parameters.
        match method {
            Method::BaselineA => {
                // Classical COLREGs baseline: vessels follow COLREGS
                // deterministically with no weather *awareness* and therefore no
                // weather *mitigation*.
                //
                // Crucially, comms degradation inside the storm is an
                // ENVIRONMENTAL effect (heavy weather destroys VHF/AIS), so it
                // applies to every method equally — Baseline A does NOT get
                // "perfect radio". Granting it perfect storm comms previously
                // made A artificially collision-free and inverted the intended
                // H1/H3 ordering (Proposed should beat A under the storm).
                //
                // Revisited after the depth+lane route rebase (multi-seed probe,
                // Storm/Calm ≈ 2.39×, Proposed storm collisions < BaselineA):
                // keep honest environmental storm-comms loss. The only thing A
                // lacks is behavioural mitigation — it does not slow in the
                // storm (storm_speed_factor stays 1.0).
                cfg.storm_speed_factor = 1.0;
            }
            Method::BaselineB | Method::ProposedSystem => {
                // BaselineB: shore-only; mesh absent so offshore vessels are blind.
                // ProposedSystem: full system — scenario defaults already set above.
            }
        }

        cfg.compute_field_units();
        cfg
    }
}
