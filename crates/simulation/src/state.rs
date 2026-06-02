use crate::ais::{self, Port};
use crate::comms::{self, MsgKind, VesselMsg};
use crate::kpi::{KpiAccumulator, KpiSnapshot};
use crate::land::LandMask;
use crate::scenario::{Method, SimConfig};
use crate::vessel::{VesselAgent, VesselState};
use crate::weather::WeatherField;

use anyhow::Result;
use krabmaga::engine::{schedule::Schedule, state::State};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::any::Any;
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{BufWriter, Write};

/// Maximum number of messages kept in the rolling comms log.
const COMMS_LOG_CAPACITY: usize = 60;
/// Maximum number of collision events kept (shown as fading markers on map).
const COLLISION_EVENTS_CAPACITY: usize = 30;

/// Central simulation state.
pub struct SimState {
    pub step: u64,
    pub vessels: Vec<VesselAgent>,
    pub kpi: KpiAccumulator,
    pub config: SimConfig,
    pub rng: SmallRng,
    pub land_mask: LandMask,
    pub utc_hour: f64,
    pub ports: Vec<Port>,
    /// Rolling log of the last `COMMS_LOG_CAPACITY` vessel messages —
    /// included in every WebSocket snapshot.
    pub comms_log: VecDeque<VesselMsg>,
    /// Rolling log of the last `COLLISION_EVENTS_CAPACITY` collision locations —
    /// each entry is `(tick, field_x, field_y)`.
    pub collision_events: VecDeque<(u64, f64, f64)>,
    /// Live 50×50 stochastic weather grid.
    pub weather: WeatherField,
    /// Current storm-centre in field coordinates `(x, y)`.
    /// `None` when `config.storm_enabled` is false.
    pub storm_pos: Option<(f64, f64)>,
    /// Index of the next waypoint in `config.storm_track` the storm is heading toward.
    pub storm_waypoint_idx: usize,
    pub snapshot_tx: Option<tokio::sync::broadcast::Sender<String>>,
    pub stop_rx: Option<tokio::sync::watch::Receiver<bool>>,
    pub log_writer: Option<BufWriter<File>>,
    next_vessel_id: u64,
}

impl SimState {
    /// # Errors
    /// Returns an error if AIS data, land mask, or port files cannot be loaded.
    pub fn new(config: SimConfig) -> Result<Self> {
        let mut rng = SmallRng::seed_from_u64(config.seed);
        let ais_records = ais::load_ais(&config.ais_path)?;
        let land_mask = LandMask::from_geojson(&config.land_mask_path)?;
        let ports = match ais::load_ports_from_file(&config.ports_path) {
            Ok(p) if !p.is_empty() => p,
            _ => ais::extract_ports(&ais_records, config.port_dedup_radius_nm),
        };

        let weather = WeatherField::new(config.weather_preset, &mut rng);

        let storm_pos = if config.storm_enabled {
            config
                .storm_track
                .first()
                .map(|wp| ais::lat_lon_to_field(wp[0], wp[1]))
        } else {
            None
        };

        Ok(Self {
            step: 0,
            vessels: Vec::new(),
            kpi: KpiAccumulator::new(),
            land_mask,
            utc_hour: 6.0,
            ports,
            comms_log: VecDeque::with_capacity(COMMS_LOG_CAPACITY + 1),
            collision_events: VecDeque::with_capacity(COLLISION_EVENTS_CAPACITY + 1),
            weather,
            storm_pos,
            storm_waypoint_idx: 1, // index 0 is spawn; we head toward index 1 first
            snapshot_tx: None,
            stop_rx: None,
            log_writer: None,
            next_vessel_id: 1,
            rng,
            config,
        })
    }

    fn spawn_vessel(&mut self, ais_records: &[ais::AisRecord], is_initial: bool) {
        if ais_records.is_empty() {
            return;
        }
        let idx = self.rng.gen_range(0..ais_records.len());
        let rec = &ais_records[idx];
        let route: Vec<(f64, f64)> = rec
            .waypoints
            .iter()
            .map(|wp| ais::lat_lon_to_field(wp.lat, wp.lon))
            .collect();
        if route.len() < 2 {
            return;
        }

        let start_at_origin = self.rng.gen::<bool>();
        let (mut spawn_idx, start_dir): (usize, i8) = if start_at_origin {
            (0, 1)
        } else {
            (route.len() - 1, -1)
        };

        let mut walked = 0usize;
        while self
            .land_mask
            .contains_point(route[spawn_idx].0, route[spawn_idx].1)
        {
            let next_opt = if start_dir > 0 {
                spawn_idx.checked_add(1)
            } else {
                spawn_idx.checked_sub(1)
            };
            let Some(next) = next_opt.filter(|&n| n < route.len()) else {
                return;
            };
            spawn_idx = next;
            walked += 1;
            if walked >= route.len() {
                return;
            }
        }
        let start_pos = route[spawn_idx];
        let speed_kn = crate::vessel::typical_speed_kn(&rec.vessel_type, self.rng.gen::<f64>());

        let id = (rec.mmsi.saturating_mul(1_000)) + self.next_vessel_id;
        self.next_vessel_id = self.next_vessel_id.wrapping_add(1);

        let dwell_ticks: u32 = if is_initial {
            self.rng
                .gen_range(0..=self.config.initial_dwell_spread_ticks.max(1))
        } else {
            let span = self
                .config
                .port_dwell_max_ticks
                .saturating_sub(self.config.port_dwell_min_ticks)
                .max(1);
            self.config.port_dwell_min_ticks + self.rng.gen_range(0..span)
        };

        self.kpi.record_spawn();
        self.vessels.push(VesselAgent {
            id,
            name: rec.name.clone(),
            vessel_type: rec.vessel_type.clone(),
            route,
            route_index: spawn_idx,
            route_direction: start_dir,
            position: start_pos,
            last_valid_position: start_pos,
            heading_deg: 0.0,
            speed_kn,
            state: VesselState::Docked,
            n_crew: self.config.n_crew_per_vessel,
            dock_until_tick: Some(self.step + u64::from(dwell_ticks)),
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            fatigue: 0.0,
        });
    }

    /// Appends a message to the rolling comms log, evicting the oldest entry
    /// once capacity is exceeded.
    fn push_msg(&mut self, msg: VesselMsg) {
        self.comms_log.push_back(msg);
        while self.comms_log.len() > COMMS_LOG_CAPACITY {
            self.comms_log.pop_front();
        }
    }

    // ── Storm helpers ────────────────────────────────────────────────────────

    /// Advances the storm centre by one tick along its waypoint track.
    fn advance_storm(&mut self) {
        let Some(pos) = self.storm_pos.as_mut() else {
            return;
        };

        let track = &self.config.storm_track;
        if self.storm_waypoint_idx >= track.len() {
            return; // storm has reached its terminus — stay put
        }

        let target = ais::lat_lon_to_field(
            track[self.storm_waypoint_idx][0],
            track[self.storm_waypoint_idx][1],
        );

        let dx = target.0 - pos.0;
        let dy = target.1 - pos.1;
        let dist = (dx * dx + dy * dy).sqrt();
        let step = self.config.storm_drift_nm_per_tick;

        if dist <= step {
            // Snap to waypoint and advance to the next one.
            *pos = target;
            self.storm_waypoint_idx += 1;
        } else {
            pos.0 += dx / dist * step;
            pos.1 += dy / dist * step;
        }
    }

    // ── run_tick (non-krabmaga path) ─────────────────────────────────────────

    /// Executes one full macro tick (used outside of krabmaga).
    #[allow(clippy::too_many_lines)]
    pub fn run_tick(&mut self, ais_records: &[ais::AisRecord]) {
        let tick = self.step;
        let config = self.config.clone();
        let land_mask = self.land_mask.clone();

        // ── Step 1: advance weather field ─────────────────────────────────
        self.weather.update(&mut self.rng);

        let ports = self.ports.clone();
        for vessel in &mut self.vessels {
            vessel.step(tick, &config, &land_mask, &ports);
        }

        // Grounding: revert position only.
        for vessel in &mut self.vessels {
            if vessel.state == VesselState::Docked {
                continue;
            }
            if self
                .land_mask
                .contains_point(vessel.position.0, vessel.position.1)
            {
                vessel.position = vessel.last_valid_position;
            }
        }

        // Hazard-gated speed reduction (Baseline B + Proposed System only).
        // v_eff = v · max(0.30, 1 − 0.45·W)  →  position lerped back by that factor.
        // Also updates per-vessel crew fatigue using the local hazard and time-of-day.
        {
            let ww = config.world_width_nm;
            let wh = config.world_height_nm;
            let utc = self.utc_hour;
            let method = config.method;
            for vessel in &mut self.vessels {
                let w = if vessel.state == VesselState::Docked {
                    0.0 // docked vessels recover regardless of ambient weather
                } else {
                    self.weather
                        .hazard_at(vessel.position.0, vessel.position.1, ww, wh)
                };
                // Fatigue ticks for every vessel every tick.
                vessel.tick_fatigue(w, utc, method);

                if vessel.state == VesselState::Docked {
                    continue;
                }
                if config.method != Method::BaselineA {
                    let factor = (1.0 - 0.45 * w).max(0.30);
                    if factor < 1.0 {
                        let lv = vessel.last_valid_position;
                        vessel.position.0 = lv.0 + (vessel.position.0 - lv.0) * factor;
                        vessel.position.1 = lv.1 + (vessel.position.1 - lv.1) * factor;
                    }
                }
            }
        }

        // Storm speed dampening: vessels inside the storm zone move slower.
        // We do this by nudging their position back toward last_valid_position
        // proportional to (1 - speed_factor), which approximates a shorter step.
        if let Some(sp) = self.storm_pos {
            let r2 = config.storm_radius_nm * config.storm_radius_nm;
            let factor = config.storm_speed_factor;
            for vessel in &mut self.vessels {
                if vessel.state == VesselState::Docked {
                    continue;
                }
                let dx = vessel.position.0 - sp.0;
                let dy = vessel.position.1 - sp.1;
                if dx * dx + dy * dy <= r2 {
                    // Blend back: effective_pos = lv + (new - lv) * factor
                    let lv = vessel.last_valid_position;
                    vessel.position.0 = lv.0 + (vessel.position.0 - lv.0) * factor;
                    vessel.position.1 = lv.1 + (vessel.position.1 - lv.1) * factor;
                }
            }
        }

        // Log ResumeRoute for vessels that just finished an avoidance manoeuvre.
        let resume_pairs: Vec<(u64, String, u64)> = self
            .vessels
            .iter_mut()
            .filter_map(|v| {
                v.avoided_vessel_id
                    .take()
                    .map(|other_id| (v.id, v.name.clone(), other_id))
            })
            .collect();
        for (id, name, other_id) in resume_pairs {
            let other_name = self
                .vessels
                .iter()
                .find(|v| v.id == other_id)
                .map_or_else(|| other_id.to_string(), |v| v.name.clone());
            self.push_msg(VesselMsg {
                tick,
                from_id: id,
                from_name: name,
                to_id: other_id,
                to_name: other_name,
                kind: MsgKind::ResumeRoute,
            });
        }

        // CPA detection + avoidance waypoint injection.
        let storm_pos = self.storm_pos;
        run_collision_avoidance(
            tick,
            &config,
            &mut self.vessels,
            &mut self.comms_log,
            &mut self.kpi,
            &mut self.rng,
            storm_pos,
        );

        // Cooldown countdown.
        for v in &mut self.vessels {
            v.avoidance_cooldown = v.avoidance_cooldown.saturating_sub(1);
        }

        // Collision KPI.
        let n = self.vessels.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if self.vessels[i].state != VesselState::Active
                    || self.vessels[j].state != VesselState::Active
                {
                    continue;
                }
                let dx = self.vessels[i].position.0 - self.vessels[j].position.0;
                let dy = self.vessels[i].position.1 - self.vessels[j].position.1;
                if (dx * dx + dy * dy).sqrt() <= config.collision_trigger_field {
                    self.kpi.record_collision();
                    // Evaluate evac + fatality outcomes for this collision.
                    let w = self.weather.hazard_at(
                        f64::midpoint(self.vessels[i].position.0, self.vessels[j].position.0),
                        f64::midpoint(self.vessels[i].position.1, self.vessels[j].position.1),
                        config.world_width_nm,
                        config.world_height_nm,
                    );
                    let crew_i = self.vessels[i].n_crew;
                    let crew_j = self.vessels[j].n_crew;
                    let max_fatigue = self.vessels[i].fatigue.max(self.vessels[j].fatigue);
                    self.kpi.record_collision_outcome(
                        crew_i,
                        crew_j,
                        w,
                        max_fatigue,
                        config.method,
                        &mut self.rng,
                    );
                    let mx = f64::midpoint(self.vessels[i].position.0, self.vessels[j].position.0);
                    let my = f64::midpoint(self.vessels[i].position.1, self.vessels[j].position.1);
                    self.collision_events.push_back((tick, mx, my));
                    while self.collision_events.len() > COLLISION_EVENTS_CAPACITY {
                        self.collision_events.pop_front();
                    }
                }
            }
        }

        self.kpi.record_tick(&self.vessels);
        self.kpi.record_p_prep_tick(
            &self.vessels,
            &self.weather,
            config.world_width_nm,
            config.world_height_nm,
            config.method,
        );

        while self.vessels.len() < config.n_vessels as usize {
            self.spawn_vessel(ais_records, self.step == 0);
        }

        self.utc_hour = (self.utc_hour + 0.25) % 24.0;
        self.advance_storm();

        let snap_n = config.snapshot_every_n_ticks;
        if snap_n > 0 && tick.is_multiple_of(u64::from(snap_n)) {
            if let Some(tx) = &self.snapshot_tx {
                let snap = self.build_snapshot();
                let _ = tx.send(snap);
            }
        }

        let log_n = config.log_every_n_ticks;
        if log_n > 0 && tick.is_multiple_of(u64::from(log_n)) && self.log_writer.is_some() {
            let line = self.build_log_line();
            if let Some(w) = &mut self.log_writer {
                let _ = writeln!(w, "{line}");
            }
        }

        self.step += 1;
    }

    // ── Snapshot / log builders ───────────────────────────────────────────────

    #[must_use]
    pub fn build_snapshot(&self) -> String {
        use serde_json::{json, Value};

        let w_w = self.config.world_width_nm;
        let w_h = self.config.world_height_nm;

        let vessels: Vec<Value> = self
            .vessels
            .iter()
            .map(|v| {
                let (lat, lon) = ais::field_to_lat_lon(v.position.0, v.position.1);
                json!({
                    "id": v.id,
                    "name": v.name,
                    "lat": lat,
                    "lon": lon,
                    "heading_deg": v.heading_deg,
                    "state": format!("{:?}", v.state),
                    "n_crew": v.n_crew,
                    "dock_until_tick": v.dock_until_tick,
                    "avoiding": v.avoiding,
                })
            })
            .collect();

        let ports: Vec<Value> = self
            .ports
            .iter()
            .map(|p| {
                let (lat, lon) = ais::field_to_lat_lon(p.position.0, p.position.1);
                json!({ "id": p.id, "name": p.name, "lat": lat, "lon": lon })
            })
            .collect();

        let kpis = self.kpi.snapshot(self.step);

        // Serialise the comms log as a plain Vec for JSON.
        let comms_log: Vec<&VesselMsg> = self.comms_log.iter().collect();

        // Serialise collision events as [{tick, lat, lon}].
        let collision_events: Vec<serde_json::Value> = self
            .collision_events
            .iter()
            .map(|(t, fx, fy)| {
                let (lat, lon) = ais::field_to_lat_lon(*fx, *fy);
                json!({ "tick": t, "lat": lat, "lon": lon })
            })
            .collect();

        // Serialise storm zone for frontend rendering.
        let storm_json = self.storm_pos.map(|(fx, fy)| {
            let (lat, lon) = ais::field_to_lat_lon(fx, fy);
            json!({ "lat": lat, "lon": lon, "radius_nm": self.config.storm_radius_nm })
        });

        serde_json::to_string(&json!({
            "step": self.step,
            "vessels": vessels,
            "kpis": kpis,
            "bbox": {
                "lat_min": ais::LAT_MIN, "lat_max": ais::LAT_MAX,
                "lon_min": ais::LON_MIN, "lon_max": ais::LON_MAX,
            },
            "world_width_nm": w_w,
            "world_height_nm": w_h,
            "ports": ports,
            "comms_log": comms_log,
            "collision_events": collision_events,
            "storm": storm_json,
            "weather_grid": self.weather.hazard,
            "weather_grid_size": crate::weather::GRID_CELLS,
            "storm_centers": [],
            "rescue_agents": [],
            "shore_stations": [],
            "wrecks": [],
            "weather_channels": {},
            "weather_meta": null,
            "telemetry": {
                "active_count": self.vessels.iter().filter(|v| v.state == VesselState::Active).count(),
                "docked_count": self.vessels.iter().filter(|v| v.state == VesselState::Docked).count(),
                "avoiding_count": self.vessels.iter().filter(|v| v.avoiding).count(),
                "evac_count": 0,
                "fatal_cumulative": 0,
                "rescued_cumulative": 0,
                "rescue_agent_count": 0,
                "wreck_count": 0,
            },
        }))
        .unwrap_or_default()
    }

    fn build_log_line(&self) -> String {
        // Reuse the full snapshot so playback has every field the live view has.
        self.build_snapshot()
    }

    #[must_use]
    pub fn kpi_snapshot(&self) -> KpiSnapshot {
        self.kpi.snapshot(self.step)
    }

    #[must_use]
    pub fn shore_stations_json(&self) -> Vec<serde_json::Value> {
        vec![]
    }
}

// ── Collision-avoidance logic (shared by run_tick and PostTickAgent) ──────────

/// Scans all active vessel pairs for predicted collisions.
/// For pairs that exceed the risk threshold and where the give-way vessel
/// has no active cooldown, injects an avoidance waypoint and logs the
/// three-message conversation (Warning → `GivingWay` → `MaintainingCourse`).
#[allow(clippy::too_many_lines)]
fn run_collision_avoidance(
    tick: u64,
    config: &SimConfig,
    vessels: &mut [VesselAgent],
    comms_log: &mut VecDeque<VesselMsg>,
    kpi: &mut crate::kpi::KpiAccumulator,
    rng: &mut impl rand::Rng,
    storm_pos: Option<(f64, f64)>,
) {
    /// Returns the comms-success-rate applicable to a vessel at `pos` given
    /// the storm zone (if any).
    fn vessel_comms_rate(
        pos: (f64, f64),
        config: &SimConfig,
        storm_pos: Option<(f64, f64)>,
    ) -> f64 {
        if let Some(sp) = storm_pos {
            let dx = pos.0 - sp.0;
            let dy = pos.1 - sp.1;
            if dx * dx + dy * dy <= config.storm_radius_nm * config.storm_radius_nm {
                return config.storm_comms_success_rate;
            }
        }
        config.comms_success_rate
    }
    let warn_r2 = config.collision_warn_radius_nm * config.collision_warn_radius_nm;
    let n = vessels.len();

    // Collect (give_way_idx, stand_on_idx, cpa_nm, tta_ticks) for all at-risk pairs.
    let mut manoeuvres: Vec<(usize, usize, f64, u64)> = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            if vessels[i].state != VesselState::Active || vessels[j].state != VesselState::Active {
                continue;
            }

            let dx = vessels[i].position.0 - vessels[j].position.0;
            let dy = vessels[i].position.1 - vessels[j].position.1;
            if dx * dx + dy * dy > warn_r2 {
                continue;
            }

            let vel_i = comms::vessel_velocity(vessels[i].heading_deg, vessels[i].speed_kn);
            let vel_j = comms::vessel_velocity(vessels[j].heading_deg, vessels[j].speed_kn);

            let (cpa, tta) =
                comms::compute_cpa(vessels[i].position, vel_i, vessels[j].position, vel_j);

            if cpa > config.collision_warn_cpa_nm {
                continue;
            }
            if tta <= 0.0 || tta > f64::from(config.collision_warn_tta_max_ticks) {
                continue;
            }

            // Lower ID is the give-way vessel.
            let (gw, so) = if vessels[i].id < vessels[j].id {
                (i, j)
            } else {
                (j, i)
            };

            if vessels[gw].avoidance_cooldown > 0 {
                continue;
            }

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            manoeuvres.push((gw, so, cpa, tta as u64));
        }
    }

    for (gw, so, cpa_nm, tta_ticks) in manoeuvres {
        // Comms success rate = worst of:
        //   (a) weather/storm comms quality at each vessel's position, AND
        //   (b) a fatigue penalty — tired watchkeepers miss or delay warnings.
        let weather_rate = vessel_comms_rate(vessels[gw].position, config, storm_pos)
            .min(vessel_comms_rate(vessels[so].position, config, storm_pos));
        let fatigue_factor = vessels[gw]
            .fatigue_comms_factor()
            .min(vessels[so].fatigue_comms_factor());
        let rate = weather_rate * fatigue_factor;
        if rate < 1.0 && rng.gen::<f64>() >= rate {
            continue;
        }

        // Inject avoidance waypoint into the give-way vessel.
        let wp = comms::avoidance_waypoint(
            vessels[gw].position,
            vessels[gw].heading_deg,
            config.avoidance_offset_nm,
            config.avoidance_forward_nm,
        );
        vessels[gw].avoidance_wps = vec![wp]; // replace any stale wp
        vessels[gw].avoidance_cooldown = config.avoidance_cooldown_ticks;
        vessels[gw].avoiding_vessel_id = Some(vessels[so].id);
        vessels[gw].avoiding = true;

        let gw_id = vessels[gw].id;
        let gw_name = vessels[gw].name.clone();
        let so_id = vessels[so].id;
        let so_name = vessels[so].name.clone();

        // Record TTA from this warning for avg_tta_hours KPI.
        kpi.record_collision_warning(tta_ticks);

        // 1. Stand-on warns give-way.
        push_capped(
            comms_log,
            VesselMsg {
                tick,
                from_id: so_id,
                from_name: so_name.clone(),
                to_id: gw_id,
                to_name: gw_name.clone(),
                kind: MsgKind::CollisionWarning { cpa_nm, tta_ticks },
            },
        );
        // 2. Give-way vessel acknowledges and turns starboard.
        push_capped(
            comms_log,
            VesselMsg {
                tick,
                from_id: gw_id,
                from_name: gw_name.clone(),
                to_id: so_id,
                to_name: so_name.clone(),
                kind: MsgKind::GivingWay,
            },
        );
        // 3. Stand-on vessel maintains course.
        push_capped(
            comms_log,
            VesselMsg {
                tick,
                from_id: so_id,
                from_name: so_name,
                to_id: gw_id,
                to_name: gw_name,
                kind: MsgKind::MaintainingCourse,
            },
        );
    }
}

#[inline]
fn push_capped(log: &mut VecDeque<VesselMsg>, msg: VesselMsg) {
    log.push_back(msg);
    while log.len() > COMMS_LOG_CAPACITY {
        log.pop_front();
    }
}

// ── PostTickAgent ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct PostTickAgent;

impl krabmaga::engine::agent::Agent for PostTickAgent {
    #[allow(clippy::too_many_lines)]
    fn step(&mut self, state: &mut dyn State) {
        let wrapper = state
            .as_any_mut()
            .downcast_mut::<SimStateWrapper>()
            .unwrap();
        let tick = wrapper.inner.step;
        let config = wrapper.inner.config.clone();

        // ── Step 1: advance weather field ─────────────────────────────────
        wrapper.inner.weather.update(&mut wrapper.inner.rng);

        // Grounding: revert position only.
        for vessel in &mut wrapper.inner.vessels {
            if vessel.state == VesselState::Docked {
                continue;
            }
            if wrapper
                .inner
                .land_mask
                .contains_point(vessel.position.0, vessel.position.1)
            {
                vessel.position = vessel.last_valid_position;
            }
        }

        // Log ResumeRoute for vessels that completed an avoidance manoeuvre.
        let resume_pairs: Vec<(u64, String, u64)> = wrapper
            .inner
            .vessels
            .iter_mut()
            .filter_map(|v| {
                v.avoided_vessel_id
                    .take()
                    .map(|oid| (v.id, v.name.clone(), oid))
            })
            .collect();
        for (id, name, other_id) in resume_pairs {
            let other_name = wrapper
                .inner
                .vessels
                .iter()
                .find(|v| v.id == other_id)
                .map_or_else(|| other_id.to_string(), |v| v.name.clone());
            push_capped(
                &mut wrapper.inner.comms_log,
                VesselMsg {
                    tick,
                    from_id: id,
                    from_name: name,
                    to_id: other_id,
                    to_name: other_name,
                    kind: MsgKind::ResumeRoute,
                },
            );
        }

        // Hazard-gated speed reduction + per-vessel fatigue update.
        {
            let ww = config.world_width_nm;
            let wh = config.world_height_nm;
            let utc = wrapper.inner.utc_hour;
            let method = config.method;
            for vessel in &mut wrapper.inner.vessels {
                let w = if vessel.state == VesselState::Docked {
                    0.0
                } else {
                    wrapper
                        .inner
                        .weather
                        .hazard_at(vessel.position.0, vessel.position.1, ww, wh)
                };
                vessel.tick_fatigue(w, utc, method);

                if vessel.state == VesselState::Docked {
                    continue;
                }
                if config.method != Method::BaselineA {
                    let factor = (1.0 - 0.45 * w).max(0.30);
                    if factor < 1.0 {
                        let lv = vessel.last_valid_position;
                        vessel.position.0 = lv.0 + (vessel.position.0 - lv.0) * factor;
                        vessel.position.1 = lv.1 + (vessel.position.1 - lv.1) * factor;
                    }
                }
            }
        }

        // Storm speed dampening.
        if let Some(sp) = wrapper.inner.storm_pos {
            let r2 = config.storm_radius_nm * config.storm_radius_nm;
            let factor = config.storm_speed_factor;
            for vessel in &mut wrapper.inner.vessels {
                if vessel.state == VesselState::Docked {
                    continue;
                }
                let dx = vessel.position.0 - sp.0;
                let dy = vessel.position.1 - sp.1;
                if dx * dx + dy * dy <= r2 {
                    let lv = vessel.last_valid_position;
                    vessel.position.0 = lv.0 + (vessel.position.0 - lv.0) * factor;
                    vessel.position.1 = lv.1 + (vessel.position.1 - lv.1) * factor;
                }
            }
        }

        // CPA detection → avoidance waypoint injection + message log.
        let storm_pos = wrapper.inner.storm_pos;
        run_collision_avoidance(
            tick,
            &config,
            &mut wrapper.inner.vessels,
            &mut wrapper.inner.comms_log,
            &mut wrapper.inner.kpi,
            &mut wrapper.inner.rng,
            storm_pos,
        );

        // Cooldown countdown.
        for v in &mut wrapper.inner.vessels {
            v.avoidance_cooldown = v.avoidance_cooldown.saturating_sub(1);
        }

        // Collision KPI only — no damage, no state change.
        let n = wrapper.inner.vessels.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if wrapper.inner.vessels[i].state != VesselState::Active
                    || wrapper.inner.vessels[j].state != VesselState::Active
                {
                    continue;
                }
                let dx = wrapper.inner.vessels[i].position.0 - wrapper.inner.vessels[j].position.0;
                let dy = wrapper.inner.vessels[i].position.1 - wrapper.inner.vessels[j].position.1;
                if (dx * dx + dy * dy).sqrt() <= config.collision_trigger_field {
                    wrapper.inner.kpi.record_collision();
                    let w = wrapper.inner.weather.hazard_at(
                        f64::midpoint(
                            wrapper.inner.vessels[i].position.0,
                            wrapper.inner.vessels[j].position.0,
                        ),
                        f64::midpoint(
                            wrapper.inner.vessels[i].position.1,
                            wrapper.inner.vessels[j].position.1,
                        ),
                        config.world_width_nm,
                        config.world_height_nm,
                    );
                    let crew_i = wrapper.inner.vessels[i].n_crew;
                    let crew_j = wrapper.inner.vessels[j].n_crew;
                    let max_fatigue = wrapper.inner.vessels[i]
                        .fatigue
                        .max(wrapper.inner.vessels[j].fatigue);
                    wrapper.inner.kpi.record_collision_outcome(
                        crew_i,
                        crew_j,
                        w,
                        max_fatigue,
                        config.method,
                        &mut wrapper.inner.rng,
                    );
                    let mx = f64::midpoint(
                        wrapper.inner.vessels[i].position.0,
                        wrapper.inner.vessels[j].position.0,
                    );
                    let my = f64::midpoint(
                        wrapper.inner.vessels[i].position.1,
                        wrapper.inner.vessels[j].position.1,
                    );
                    wrapper.inner.collision_events.push_back((tick, mx, my));
                    while wrapper.inner.collision_events.len() > COLLISION_EVENTS_CAPACITY {
                        wrapper.inner.collision_events.pop_front();
                    }
                }
            }
        }

        wrapper.inner.kpi.record_tick(&wrapper.inner.vessels);
        wrapper.inner.kpi.record_p_prep_tick(
            &wrapper.inner.vessels,
            &wrapper.inner.weather,
            config.world_width_nm,
            config.world_height_nm,
            config.method,
        );

        // Top-up fleet.
        let ais = wrapper.ais_records.clone();
        while wrapper.inner.vessels.len() < config.n_vessels as usize {
            wrapper.inner.spawn_vessel(&ais, tick == 0);
        }

        wrapper.inner.utc_hour = (wrapper.inner.utc_hour + 0.25) % 24.0;
        wrapper.inner.advance_storm();

        let snap_n = config.snapshot_every_n_ticks;
        if snap_n > 0 && tick.is_multiple_of(u64::from(snap_n)) {
            if let Some(tx) = &wrapper.inner.snapshot_tx {
                let snap = wrapper.inner.build_snapshot();
                let _ = tx.send(snap);
            }
        }

        let log_n = config.log_every_n_ticks;
        if log_n > 0 && tick.is_multiple_of(u64::from(log_n)) && wrapper.inner.log_writer.is_some()
        {
            let line = wrapper.inner.build_log_line();
            if let Some(w) = &mut wrapper.inner.log_writer {
                let _ = writeln!(w, "{line}");
            }
        }
    }
}

impl std::fmt::Display for PostTickAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PostTickAgent")
    }
}

// ── SimStateWrapper ───────────────────────────────────────────────────────────

pub struct SimStateWrapper {
    pub inner: SimState,
    pub ais_records: Vec<ais::AisRecord>,
    pub scheduled_vessel_ids: HashSet<u64>,
}

impl SimStateWrapper {
    /// # Errors
    /// Returns an error if AIS data, land mask, or port files cannot be loaded,
    /// or if the log file cannot be created.
    pub fn new(config: SimConfig) -> Result<Self> {
        let ais_records = ais::load_ais(&config.ais_path)?;
        // Open log file before moving config into SimState.
        let log_writer: Option<BufWriter<File>> = config.log_path.as_deref().and_then(|p| {
            // Create parent dirs if needed.
            if let Some(parent) = std::path::Path::new(p).parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::File::create(p).ok().map(BufWriter::new)
        });
        let mut inner = SimState::new(config)?;
        inner.log_writer = log_writer;
        Ok(Self {
            inner,
            ais_records,
            scheduled_vessel_ids: HashSet::new(),
        })
    }

    pub fn do_init(&mut self) {
        let n = self.inner.config.n_vessels as usize;
        for _ in 0..n {
            self.inner.spawn_vessel(&self.ais_records, true);
        }
    }

    pub fn run_blocking(&mut self) {
        let mut loop_num: u64 = 0;
        loop {
            self.inner.vessels.clear();
            self.scheduled_vessel_ids.clear();

            let mut schedule = Schedule::new();
            State::init(self, &mut schedule);

            loop {
                if let Some(rx) = &self.inner.stop_rx {
                    if *rx.borrow() {
                        return;
                    }
                }
                schedule.step(self.as_state_mut());
                if State::end_condition(self, &mut schedule) {
                    break;
                }
            }

            if let Some(ref mut w) = self.inner.log_writer {
                let _ = w.flush();
            }

            if !self.inner.config.loop_mode {
                break;
            }

            loop_num += 1;
            let new_seed = self.inner.config.seed.wrapping_add(loop_num);
            self.inner.rng = SmallRng::seed_from_u64(new_seed);
            self.inner.vessels.clear();
            self.inner.kpi = KpiAccumulator::new();
            self.inner.step = 0;
            self.inner.utc_hour = 6.0;
            self.inner.next_vessel_id = 1;
            self.inner.comms_log.clear();
            self.inner.collision_events.clear();
            self.inner.weather =
                WeatherField::new(self.inner.config.weather_preset, &mut self.inner.rng);
            self.inner.storm_pos = if self.inner.config.storm_enabled {
                self.inner
                    .config
                    .storm_track
                    .first()
                    .map(|wp| ais::lat_lon_to_field(wp[0], wp[1]))
            } else {
                None
            };
            self.inner.storm_waypoint_idx = 1;
            self.scheduled_vessel_ids.clear();
        }
    }
}

impl State for SimStateWrapper {
    fn init(&mut self, schedule: &mut Schedule) {
        let n = self.inner.config.n_vessels as usize;
        for _ in 0..n {
            self.inner.spawn_vessel(&self.ais_records, true);
        }

        schedule.schedule_repeating(Box::new(PostTickAgent), 0.0, 1000);

        for v in &self.inner.vessels {
            self.scheduled_vessel_ids.insert(v.id);
            schedule.schedule_repeating(Box::new(v.clone()), 0.0, 200);
        }
    }

    fn after_step(&mut self, schedule: &mut Schedule) {
        let t = schedule.time;
        for v in &self.inner.vessels {
            if self.scheduled_vessel_ids.insert(v.id) {
                schedule.schedule_repeating(Box::new(v.clone()), t + 1.0, 200);
            }
        }
    }

    fn update(&mut self, step: u64) {
        self.inner.step = step;
    }

    fn reset(&mut self) {
        self.inner.vessels.clear();
        self.inner.kpi = KpiAccumulator::new();
        self.inner.step = 0;
        self.inner.utc_hour = 6.0;
        self.inner.rng = SmallRng::seed_from_u64(self.inner.config.seed);
        self.inner.next_vessel_id = 1;
        self.inner.comms_log.clear();
        self.inner.collision_events.clear();
        self.inner.weather =
            WeatherField::new(self.inner.config.weather_preset, &mut self.inner.rng);
        self.inner.storm_pos = if self.inner.config.storm_enabled {
            self.inner
                .config
                .storm_track
                .first()
                .map(|wp| ais::lat_lon_to_field(wp[0], wp[1]))
        } else {
            None
        };
        self.inner.storm_waypoint_idx = 1;
        self.scheduled_vessel_ids.clear();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn as_state(&self) -> &dyn State {
        self
    }
    fn as_state_mut(&mut self) -> &mut dyn State {
        self
    }

    fn end_condition(&mut self, _schedule: &mut Schedule) -> bool {
        self.inner.step >= u64::from(self.inner.config.n_ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::SimConfig;

    fn test_cfg() -> SimConfig {
        SimConfig {
            ais_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ais_paths.json").into(),
            ports_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ports.json").into(),
            land_mask_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/coastline.geojson")
                .into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_init_schedules_all_vessels() {
        let cfg = test_cfg();
        let mut wrapper = SimStateWrapper::new(cfg).unwrap();
        let mut schedule = Schedule::new();
        wrapper.init(&mut schedule);
        assert_eq!(
            wrapper.inner.vessels.len(),
            wrapper.inner.config.n_vessels as usize
        );
        for v in &wrapper.inner.vessels {
            assert_eq!(v.state, VesselState::Docked);
        }
    }

    #[test]
    fn test_fleet_invariant_holds_across_ticks() {
        let mut cfg = test_cfg();
        cfg.n_ticks = 200;
        cfg.n_vessels = 12;
        let mut wrapper = SimStateWrapper::new(cfg).unwrap();
        wrapper.do_init();
        for _ in 0..200 {
            let records = wrapper.ais_records.clone();
            wrapper.inner.run_tick(&records);
            assert_eq!(
                wrapper.inner.vessels.len(),
                wrapper.inner.config.n_vessels as usize
            );
        }
    }

    #[test]
    fn test_comms_log_populated_on_potential_collision() {
        // Two vessels head-on: A heads north, B heads south, 5 nm apart.
        // With warn_radius=15 nm and cpa_nm=2.0, they should trigger a warning.
        use crate::ais::field_width_nm;
        let cfg = test_cfg();
        let cx = field_width_nm() / 2.0; // somewhere in open water

        let mut state = SimState::new(cfg.clone()).unwrap();

        let route = vec![(cx, 400.0), (cx, 500.0)];
        let make = |id: u64, pos: (f64, f64), hdg: f64| VesselAgent {
            id,
            name: format!("V{id}"),
            vessel_type: "cargo".into(),
            route: route.clone(),
            route_index: 0,
            route_direction: 1,
            position: pos,
            last_valid_position: pos,
            heading_deg: hdg,
            speed_kn: 15.0,
            state: VesselState::Active,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            fatigue: 0.0,
        };

        state.vessels.push(make(1, (cx, 450.0), 0.0)); // heading north
        state.vessels.push(make(2, (cx, 455.0), 180.0)); // heading south

        run_collision_avoidance(
            0,
            &cfg,
            &mut state.vessels,
            &mut state.comms_log,
            &mut state.kpi,
            &mut state.rng,
            None,
        );

        assert!(
            !state.comms_log.is_empty(),
            "comms log should have messages for a head-on encounter"
        );
        assert!(
            state.vessels[0].avoiding || state.vessels[1].avoiding,
            "at least one vessel should be in avoidance mode"
        );
    }
}
