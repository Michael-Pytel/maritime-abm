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

    // --- port-approach queueing (anti-funnel) ---
    /// Distance (nm) from a port within which approach-queueing applies.
    pub port_approach_radius_nm: f64,
    /// Minimum separation (nm) a vessel keeps behind a leader heading to the
    /// same port; closer than this it holds (anchors) until the gap opens.
    pub port_queue_gap_nm: f64,

    /// SIMCOL collision-consequence surrogate parameters.
    #[serde(default)]
    pub simcol: crate::simcol::SimcolParams,

    /// Search-and-rescue subsystem parameters.
    #[serde(default)]
    pub sar: crate::sar::SarParams,

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
            n_vessels: 20,
            n_crew_per_vessel: 10,
            n_ticks: 2880,
            // Hull-contact radius: a hard collision triggers at 2·r = 0.10 nm
            // (~185 m, about one ship-length sum) rather than the old 0.30 nm.
            collision_radius_nm: 0.05,
            port_approach_radius_nm: 3.0,
            port_queue_gap_nm: 0.5,
            simcol: crate::simcol::SimcolParams::default(),
            sar: crate::sar::SarParams::default(),
            sim_month: default_sim_month(),
            port_dwell_min_ticks: 8,
            port_dwell_max_ticks: 48,
            initial_dwell_spread_ticks: 16,
            port_dedup_radius_nm: 8.0,
            // Perfect-weather COLREGS baseline: wide detection, generous berth,
            // fast re-evaluation.  Weather simulation will degrade these.
            collision_warn_radius_nm: 20.0,
            collision_warn_cpa_nm: 5.0,
            collision_warn_tta_max_ticks: 12,
            avoidance_cooldown_ticks: 3,
            avoidance_offset_nm: 5.0,
            avoidance_forward_nm: 3.0,
            comms_success_rate: 1.0,
            weather_preset: WeatherPreset::Mixed,
            // Storm zone — disabled by default; StormCorridor overrides below.
            storm_enabled: false,
            storm_radius_nm: 120.0,
            storm_comms_success_rate: 0.30,
            storm_speed_factor: 0.70,
            storm_drift_nm_per_tick: 0.30,
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
                cfg.n_vessels = 20;
                cfg.weather_preset = WeatherPreset::Calm;
            }
            Scenario::StormCorridor => {
                cfg.n_vessels = 25;
                cfg.weather_preset = WeatherPreset::Stormy;
                cfg.storm_enabled = true;
                cfg.storm_radius_nm = 160.0; // wider zone
                cfg.storm_comms_success_rate = 0.08; // 92 % of warnings lost — near-blackout
                cfg.storm_speed_factor = 0.45; // vessels crawl at 45 % speed
                                               // 0.30 nm/tick ≈ 1.2 kn.  Total track ≈ 860 nm → ~2870 ticks,
                                               // which fills a standard 2880-tick run almost exactly.
                cfg.storm_drift_nm_per_tick = 0.30;
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
                cfg.n_vessels = 25;
            }
            Scenario::DeepWaterRescue => {
                cfg.n_vessels = 15;
            }
        }

        // ── Method overrides ──────────────────────────────────────────────────
        // Applied after scenario settings so they cleanly override any
        // scenario-specific weather parameters.
        match method {
            Method::BaselineA => {
                // Classical COLREGs baseline: vessels follow COLREGS deterministically
                // with no weather awareness whatsoever.
                //
                // The storm zone still exists on the map as a geographic feature,
                // but vessels have no knowledge of it:
                //   • Collision-avoidance comms are not degraded (VHF always gets
                //     through — "perfect radio" assumption of classical COLREGs).
                //   • No hazard-gated speed reduction (no W field visible to crew).
                //   • comms_success_rate stays at its default 1.0.
                cfg.storm_comms_success_rate = 1.0;
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
