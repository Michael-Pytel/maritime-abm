use crate::ais::{self, Port};
use crate::comms::{self, MsgKind, VesselMsg};
use crate::kpi::{KpiAccumulator, KpiSnapshot};
use crate::land::LandMask;
use crate::scenario::{Method, SimConfig};
use crate::simcol;
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
    /// Cumulative count of vessels lost (foundered, no survivors recovered).
    pub lost_cumulative: u64,
    /// Cumulative count of vessels whose survivors were rescued.
    pub rescued_cumulative: u64,
    /// Active rescue assets dispatched to foundered vessels.
    pub rescue_agents: Vec<crate::sar::RescueAgent>,
    /// Id counter for rescue assets.
    next_rescue_id: u64,
    /// Tick at which each currently-foundered vessel entered `Evac`
    /// (keyed by vessel id), used for liferaft survival timing.
    evac_since: std::collections::HashMap<u64, u64>,
    /// Rolling log of recent wreck (loss) locations `(tick, field_x, field_y)`.
    pub wrecks: VecDeque<(u64, f64, f64)>,
    /// Vessel pairs `(lo_id, hi_id)` currently within the hard-collision radius.
    /// A collision is counted once, on the tick a pair *enters* contact, rather
    /// than every tick it remains overlapping (e.g. while funnelling to a port).
    contacts: HashSet<(u64, u64)>,
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
            lost_cumulative: 0,
            rescued_cumulative: 0,
            rescue_agents: Vec::new(),
            next_rescue_id: 1,
            evac_since: std::collections::HashMap::new(),
            wrecks: VecDeque::with_capacity(COLLISION_EVENTS_CAPACITY + 1),
            contacts: HashSet::new(),
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
            anchored: false,
            follow_speed_cap: None,
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

    /// Nearest shore base (port position, field coords) to a datum, if any.
    fn nearest_base(&self, datum: (f64, f64)) -> Option<(f64, f64)> {
        self.ports.iter().map(|p| p.position).min_by(|a, b| {
            let da = dist2(*a, datum);
            let db = dist2(*b, datum);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Run one tick of the search-and-rescue subsystem.
    ///
    /// 1. Record the onset tick of any newly foundered (`Evac`) vessel.
    /// 2. Dispatch a rescue asset (helicopter / patrol) from the nearest shore
    ///    base to each foundered vessel not yet being serviced.
    /// 3. Advance each asset: mobilise, transit to the datum, then run a
    ///    Koopman random search over the drifting liferaft; a detection draw
    ///    yields `Rescued`. Record the rescue Time-to-Arrival KPI on arrival.
    /// 4. Time out any foundered vessel whose survival window has passed.
    #[allow(clippy::too_many_lines)]
    fn run_sar(&mut self, tick: u64) {
        let sar = self.config.sar;

        // (1) Onset bookkeeping for newly foundered vessels.
        for v in &self.vessels {
            if v.state == VesselState::Evac {
                self.evac_since.entry(v.id).or_insert(tick);
            }
        }

        // (1b) Liferaft drift: advect each foundered datum along the prevailing
        // current, which also grows the search-area uncertainty over time.
        if sar.drift_speed_nm_per_tick > 0.0 {
            let psi = sar.drift_bearing_deg.to_radians();
            let (dx, dy) = (
                psi.sin() * sar.drift_speed_nm_per_tick,
                psi.cos() * sar.drift_speed_nm_per_tick,
            );
            for v in &mut self.vessels {
                if v.state == VesselState::Evac {
                    v.position.0 += dx;
                    v.position.1 += dy;
                }
            }
        }

        // (2) Dispatch assets to un-serviced foundered vessels.
        let serviced: HashSet<u64> = self
            .rescue_agents
            .iter()
            .map(|r| r.target_vessel_id)
            .collect();
        let pending: Vec<(u64, (f64, f64))> = self
            .vessels
            .iter()
            .filter(|v| v.state == VesselState::Evac && !serviced.contains(&v.id))
            .map(|v| (v.id, v.position))
            .collect();
        for (vid, datum) in pending {
            if let Some(base) = self.nearest_base(datum) {
                let dist_nm = dist2(base, datum).sqrt();
                let kind = crate::sar::select_asset(dist_nm, &sar);
                let id = self.next_rescue_id;
                self.next_rescue_id = self.next_rescue_id.wrapping_add(1);
                self.rescue_agents.push(crate::sar::RescueAgent {
                    id,
                    kind,
                    position: base,
                    base,
                    target_vessel_id: vid,
                    phase: crate::sar::RescuePhase::Mobilising,
                    mobilising_ticks_left: sar.mobilisation_delay_ticks,
                    embark_ticks_left: 0,
                    dispatch_tick: tick,
                });
            }
            // With no shore base reachable the vessel waits and times out (4).
        }

        // (3) Advance assets. Ashrafi (2024) seasonal degradation: the calendar
        // month sets a month-group that scales transit/search speed and the
        // survivor-boarding rate, so rescues are slowest in winter.
        let group = crate::ashrafi::MonthGroup::from_month(self.config.sim_month);

        // Collect each datum's position and crew without holding a mutable
        // borrow on the vessel list.
        let datums: std::collections::HashMap<u64, ((f64, f64), u32)> = self
            .vessels
            .iter()
            .filter(|v| v.state == VesselState::Evac)
            .map(|v| (v.id, (v.position, v.n_crew)))
            .collect();
        let mut rescued_now: Vec<u64> = Vec::new(); // vessels recovered this tick
        let mut retire: Vec<u64> = Vec::new(); // rescue-agent ids to remove
        for agent in &mut self.rescue_agents {
            let Some(&(datum, crew)) = datums.get(&agent.target_vessel_id) else {
                retire.push(agent.id); // target no longer awaiting rescue
                continue;
            };
            let speed_kn = agent.kind.speed_kn(&sar) * group.speed_multiplier(agent.kind);
            match agent.phase {
                crate::sar::RescuePhase::Mobilising => {
                    agent.mobilising_ticks_left = agent.mobilising_ticks_left.saturating_sub(1);
                    if agent.mobilising_ticks_left == 0 {
                        agent.phase = crate::sar::RescuePhase::Transiting;
                    }
                }
                crate::sar::RescuePhase::Transiting => {
                    let step_nm = speed_kn * 0.25;
                    let dx = datum.0 - agent.position.0;
                    let dy = datum.1 - agent.position.1;
                    let d = (dx * dx + dy * dy).sqrt();
                    if d <= sar.arrival_radius_nm.max(step_nm) {
                        // On scene: record Time-to-Arrival and begin searching.
                        agent.position = datum;
                        agent.phase = crate::sar::RescuePhase::Searching;
                        self.kpi.record_rescue(tick - agent.dispatch_tick);
                    } else {
                        agent.position.0 += dx / d * step_nm;
                        agent.position.1 += dy / d * step_nm;
                    }
                }
                crate::sar::RescuePhase::Searching => {
                    // Follow the drifting datum and draw a Koopman detection at
                    // the season-degraded search speed.
                    agent.position = datum;
                    let elapsed = tick.saturating_sub(
                        *self
                            .evac_since
                            .get(&agent.target_vessel_id)
                            .unwrap_or(&tick),
                    );
                    let p = crate::sar::detection_prob_tick(&sar, speed_kn, elapsed);
                    if self.rng.gen::<f64>() < p {
                        // Located: embark the survivors at the season-degraded rate.
                        let board_min = f64::from(crew) * group.boarding_min_per_person(agent.kind);
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let board_ticks = (board_min / 15.0).ceil() as u32;
                        agent.embark_ticks_left = board_ticks.max(1);
                        agent.phase = crate::sar::RescuePhase::Embarking;
                    }
                }
                crate::sar::RescuePhase::Embarking => {
                    agent.position = datum;
                    agent.embark_ticks_left = agent.embark_ticks_left.saturating_sub(1);
                    if agent.embark_ticks_left == 0 {
                        rescued_now.push(agent.target_vessel_id);
                        retire.push(agent.id);
                    }
                }
            }
        }
        self.rescue_agents.retain(|a| !retire.contains(&a.id));

        // Apply detections: survivors recovered.
        for vid in rescued_now {
            if let Some(v) = self.vessels.iter_mut().find(|v| v.id == vid) {
                v.state = VesselState::Rescued;
            }
            self.evac_since.remove(&vid);
        }

        // (4) Time out foundered vessels whose survival window has elapsed.
        let survival_limit = u64::from(sar.liferaft_survival_ticks);
        let mut timed_out: Vec<u64> = Vec::new();
        for v in &mut self.vessels {
            if v.state == VesselState::Evac {
                let age = tick.saturating_sub(*self.evac_since.get(&v.id).unwrap_or(&tick));
                if age > survival_limit {
                    v.state = VesselState::Lost;
                    self.wrecks.push_back((tick, v.position.0, v.position.1));
                    timed_out.push(v.id);
                }
            }
        }
        for vid in timed_out {
            self.evac_since.remove(&vid);
        }
        while self.wrecks.len() > COLLISION_EVENTS_CAPACITY {
            self.wrecks.pop_front();
        }

        // Drop onset entries for vessels no longer foundered (e.g. reaped).
        let alive_evac: HashSet<u64> = self
            .vessels
            .iter()
            .filter(|v| v.state == VesselState::Evac)
            .map(|v| v.id)
            .collect();
        self.evac_since.retain(|id, _| alive_evac.contains(id));
    }

    /// True if `pos` lies within `radius_nm` of any port.
    fn near_any_port(&self, pos: (f64, f64), radius_nm: f64) -> bool {
        let r2 = radius_nm * radius_nm;
        self.ports.iter().any(|p| dist2(pos, p.position) <= r2)
    }

    /// Apply same-destination following: a trailing vessel bound for the same
    /// destination as a nearby leader matches the leader's speed (does not
    /// overtake) and, if it draws too close, holds station to keep distance.
    ///
    /// For each active vessel this finds the nearest *leader* — another active
    /// vessel that shares its destination (route endpoints within
    /// `same_destination_nm`), lies ahead (closer to that destination), and is
    /// within `follow_vicinity_nm`. The follower's speed is capped to the
    /// leader's (`follow_speed_cap`, honoured by `VesselAgent::step`), and it
    /// anchors when within `follow_keep_distance_nm`. On first acquiring a
    /// leader it is radioed to slow down, match speed, and keep distance.
    fn compute_following(&mut self, tick: u64) {
        let vicinity2 = self.config.follow_vicinity_nm.powi(2);
        let keep2 = self.config.follow_keep_distance_nm.powi(2);
        let samedest2 = self.config.same_destination_nm.powi(2);

        // (vessel_idx, position, destination, speed) for active, routed vessels.
        let active: Vec<FollowEntry> = self
            .vessels
            .iter()
            .enumerate()
            .filter(|(_, v)| v.state.is_active())
            .filter_map(|(i, v)| dest_of(v).map(|d| (i, v.position, d, v.speed_kn)))
            .collect();

        let mut leader_of: Vec<Option<usize>> = vec![None; active.len()];
        let mut hold: Vec<bool> = vec![false; active.len()];
        for (a, &(_, pos_a, dest_a, _)) in active.iter().enumerate() {
            let d_a = dist2(pos_a, dest_a); // follower's distance to its destination
            let mut best: Option<(usize, f64)> = None; // (vessel_idx, gap²)
            for (b, &(idx_b, pos_b, dest_b, _)) in active.iter().enumerate() {
                if a == b
                    || dist2(dest_a, dest_b) > samedest2 // not the same destination
                    || dist2(pos_b, dest_a) >= d_a
                // not ahead of us
                {
                    continue;
                }
                let gap = dist2(pos_a, pos_b);
                if gap <= vicinity2 && best.is_none_or(|(_, g)| gap < g) {
                    best = Some((idx_b, gap));
                }
            }
            if let Some((idx_b, gap)) = best {
                leader_of[a] = Some(idx_b);
                hold[a] = gap < keep2;
            }
        }

        // Apply caps / holds; collect newly-formed follows for messaging.
        let mut new_joins: Vec<(usize, usize)> = Vec::new(); // (leader_idx, follower_idx)
        for (a, &(i, ..)) in active.iter().enumerate() {
            if let Some(leader_idx) = leader_of[a] {
                if self.vessels[i].follow_speed_cap.is_none() {
                    new_joins.push((leader_idx, i));
                }
                self.vessels[i].follow_speed_cap = Some(self.vessels[leader_idx].speed_kn);
                self.vessels[i].anchored = hold[a];
            } else {
                self.vessels[i].follow_speed_cap = None;
                self.vessels[i].anchored = false;
            }
        }

        // The leader radios each newly-following vessel to slow and keep distance.
        for (leader_idx, follower_idx) in new_joins {
            let msg = VesselMsg {
                tick,
                from_id: self.vessels[leader_idx].id,
                from_name: self.vessels[leader_idx].name.clone(),
                to_id: self.vessels[follower_idx].id,
                to_name: self.vessels[follower_idx].name.clone(),
                kind: MsgKind::KeepDistance {
                    speed_kn: self.vessels[leader_idx].speed_kn,
                },
            };
            self.push_msg(msg);
        }
    }

    /// Remove vessels in a terminal SAR state (`Rescued` / `Lost`), tallying the
    /// cumulative counters. The fleet top-up in `post_tick` respawns
    /// replacements to maintain the configured fleet size.
    fn reap_terminal_vessels(&mut self) {
        let mut lost = 0u64;
        let mut rescued = 0u64;
        self.vessels.retain(|v| match v.state {
            VesselState::Lost => {
                lost += 1;
                false
            }
            VesselState::Rescued => {
                rescued += 1;
                false
            }
            _ => true,
        });
        self.lost_cumulative += lost;
        self.rescued_cumulative += rescued;
    }

    // ── run_tick (non-krabmaga path) ─────────────────────────────────────────

    /// Executes one full macro tick outside of krabmaga (tests / standalone).
    ///
    /// Navigates every vessel inline — the krabmaga path does this via the
    /// `VesselAgent` schedule at ordering 200 — then runs the shared
    /// post-navigation body in [`SimState::post_tick`], so the two execution
    /// paths cannot drift out of sync.
    pub fn run_tick(&mut self, ais_records: &[ais::AisRecord]) {
        let tick = self.step;
        let config = self.config.clone();
        let land_mask = self.land_mask.clone();
        let ports = self.ports.clone();

        // Agent step: navigate every vessel.
        for vessel in &mut self.vessels {
            vessel.step(tick, &config, &land_mask, &ports);
        }

        // Shared post-navigation body (weather, comms, physics, housekeeping).
        self.post_tick(ais_records);

        // Standalone path advances its own clock; the krabmaga path advances
        // `step` via `State::update`.
        self.step += 1;
    }

    /// Shared post-navigation tick body: weather update, grounding, fatigue &
    /// speed reduction, collision avoidance and consequence evaluation, KPI
    /// accumulation, fleet top-up, clock advance, snapshot and logging.
    ///
    /// Invoked by both [`SimState::run_tick`] (standalone) and
    /// `PostTickAgent::step` (krabmaga) so all per-tick physics lives in
    /// exactly one place.
    #[allow(clippy::too_many_lines)]
    pub fn post_tick(&mut self, ais_records: &[ais::AisRecord]) {
        let tick = self.step;
        let config = self.config.clone();

        // ── Step 1: advance weather field ─────────────────────────────────
        self.weather.update(&mut self.rng);

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

        // Same-destination following: trailing vessels match a leader's speed
        // and keep distance instead of drawing up side-by-side.
        self.compute_following(tick);

        // Hard-collision detection + SIMCOL consequence evaluation.
        //
        // A collision is counted once per *encounter*: only when a pair first
        // enters the hard-collision radius (rising edge), not every tick it
        // stays overlapping. Otherwise same-direction traffic funnelling to a
        // shared port endpoint — which the give-way rule does not separate
        // (overtaking has near-zero relative velocity) — would log a fresh
        // collision and re-draw SIMCOL fatalities every tick.
        let n = self.vessels.len();
        let mut contacts_now: HashSet<(u64, u64)> = HashSet::new();
        for i in 0..n {
            for j in (i + 1)..n {
                if !self.vessels[i].state.is_active() || !self.vessels[j].state.is_active() {
                    continue;
                }
                let dx = self.vessels[i].position.0 - self.vessels[j].position.0;
                let dy = self.vessels[i].position.1 - self.vessels[j].position.1;
                if (dx * dx + dy * dy).sqrt() > config.collision_trigger_field {
                    continue;
                }
                // Harbour-approach water is VTS-controlled: encounters within the
                // port exclusion radius are not counted as collisions.
                let excl = config.port_collision_exclusion_nm;
                if self.near_any_port(self.vessels[i].position, excl)
                    || self.near_any_port(self.vessels[j].position, excl)
                {
                    continue;
                }

                let key = {
                    let (a, b) = (self.vessels[i].id, self.vessels[j].id);
                    if a < b {
                        (a, b)
                    } else {
                        (b, a)
                    }
                };
                contacts_now.insert(key);
                // Already in contact last tick → ongoing overlap, not a new event.
                if self.contacts.contains(&key) {
                    continue;
                }

                self.kpi.record_collision();

                // Evaluate the consequence to each vessel as the struck ship,
                // using its type-derived principal particulars (mass/length/beam).
                let ship_i = collision_ship(&self.vessels[i]);
                let ship_j = collision_ship(&self.vessels[j]);
                let out_i = simcol::evaluate(ship_i, ship_j, &config.simcol);
                let out_j = simcol::evaluate(ship_j, ship_i, &config.simcol);

                let crew_i = self.vessels[i].n_crew;
                let crew_j = self.vessels[j].n_crew;
                self.kpi.record_struck_outcome(
                    crew_i,
                    out_i.survival_factor,
                    out_i.founders,
                    &mut self.rng,
                );
                self.kpi.record_struck_outcome(
                    crew_j,
                    out_j.survival_factor,
                    out_j.founders,
                    &mut self.rng,
                );

                // A foundering vessel enters the SAR chain (Evac).
                if out_i.founders {
                    self.vessels[i].state = VesselState::Evac;
                }
                if out_j.founders {
                    self.vessels[j].state = VesselState::Evac;
                }

                let mx = f64::midpoint(self.vessels[i].position.0, self.vessels[j].position.0);
                let my = f64::midpoint(self.vessels[i].position.1, self.vessels[j].position.1);
                self.collision_events.push_back((tick, mx, my));
                while self.collision_events.len() > COLLISION_EVENTS_CAPACITY {
                    self.collision_events.pop_front();
                }
            }
        }
        // Carry the contact set to the next tick for rising-edge detection.
        self.contacts = contacts_now;

        // Search-and-rescue: dispatch assets to foundered vessels, advance
        // in-flight rescues, and resolve Evac → Rescued / Lost.
        self.run_sar(tick);

        // Reap terminal vessels (Rescued / Lost); the fleet top-up below
        // respawns replacements to hold the fleet size.
        self.reap_terminal_vessels();

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
    }

    // ── Snapshot / log builders ───────────────────────────────────────────────

    #[must_use]
    #[allow(clippy::too_many_lines)]
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
                    "anchored": v.anchored,
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

        // Serialise dispatched rescue assets.
        let rescue_agents: Vec<Value> = self
            .rescue_agents
            .iter()
            .map(|r| {
                let (lat, lon) = ais::field_to_lat_lon(r.position.0, r.position.1);
                json!({
                    "id": r.id,
                    "kind": r.kind.label(),
                    "lat": lat,
                    "lon": lon,
                    "phase": r.phase.label(),
                    "target_id": r.target_vessel_id,
                })
            })
            .collect();

        // Shore SAR bases — the ports double as dispatch origins.
        let shore_stations: Vec<Value> = self
            .ports
            .iter()
            .map(|p| {
                let (lat, lon) = ais::field_to_lat_lon(p.position.0, p.position.1);
                json!({ "id": p.id, "name": p.name, "lat": lat, "lon": lon })
            })
            .collect();

        // Wreck (loss) markers.
        let wrecks: Vec<Value> = self
            .wrecks
            .iter()
            .map(|(t, fx, fy)| {
                let (lat, lon) = ais::field_to_lat_lon(*fx, *fy);
                json!({ "tick": t, "lat": lat, "lon": lon })
            })
            .collect();

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
            "rescue_agents": rescue_agents,
            "shore_stations": shore_stations,
            "wrecks": wrecks,
            "weather_channels": {},
            "weather_meta": null,
            "telemetry": {
                "active_count": self.vessels.iter().filter(|v| v.state == VesselState::Active).count(),
                "docked_count": self.vessels.iter().filter(|v| v.state == VesselState::Docked).count(),
                "avoiding_count": self.vessels.iter().filter(|v| v.avoiding).count(),
                "anchored_count": self.vessels.iter().filter(|v| v.anchored).count(),
                "evac_count": self.vessels.iter().filter(|v| v.state == VesselState::Evac).count(),
                "rescued_count": self.vessels.iter().filter(|v| v.state == VesselState::Rescued).count(),
                "lost_count": self.vessels.iter().filter(|v| v.state == VesselState::Lost).count(),
                "fatal_cumulative": self.kpi.fatal_crew,
                "lost_cumulative": self.lost_cumulative,
                "rescued_cumulative": self.rescued_cumulative,
                "rescue_agent_count": self.rescue_agents.len(),
                "wreck_count": self.wrecks.len(),
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

/// An active routed vessel as seen by the following step:
/// `(vessel_index, position, destination, speed_kn)`.
type FollowEntry = (usize, (f64, f64), (f64, f64), f64);

/// The destination endpoint a vessel is currently heading toward (the route
/// endpoint in its travel direction), or `None` if it has no usable route.
fn dest_of(v: &VesselAgent) -> Option<(f64, f64)> {
    if v.route.len() < 2 {
        return None;
    }
    if v.route_direction > 0 {
        v.route.last().copied()
    } else {
        v.route.first().copied()
    }
}

/// Squared Euclidean distance between two field-coordinate points.
fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

/// Build a SIMCOL ship descriptor from a vessel's kinematics and its
/// type-derived principal particulars (mass / length / beam).
fn collision_ship(v: &VesselAgent) -> simcol::CollisionShip {
    let d = crate::vessel::ship_dimensions(&v.vessel_type);
    simcol::CollisionShip {
        heading_deg: v.heading_deg,
        speed_kn: v.speed_kn,
        mass_tonnes: d.mass_tonnes,
        beam_m: d.beam_m,
        length_m: d.length_m,
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
    fn step(&mut self, state: &mut dyn State) {
        let wrapper = state
            .as_any_mut()
            .downcast_mut::<SimStateWrapper>()
            .unwrap();
        // Vessels have already navigated this tick via their own agents
        // (ordering 200); run the shared post-navigation body. `step` is
        // advanced by the krabmaga schedule through `State::update`.
        let ais = wrapper.ais_records.clone();
        wrapper.inner.post_tick(&ais);
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
            self.inner.lost_cumulative = 0;
            self.inner.rescued_cumulative = 0;
            self.inner.rescue_agents.clear();
            self.inner.next_rescue_id = 1;
            self.inner.evac_since.clear();
            self.inner.wrecks.clear();
            self.inner.contacts.clear();
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
        self.inner.lost_cumulative = 0;
        self.inner.rescued_cumulative = 0;
        self.inner.rescue_agents.clear();
        self.inner.next_rescue_id = 1;
        self.inner.evac_since.clear();
        self.inner.wrecks.clear();
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
    fn test_snapshot_surfaces_sar_states() {
        // An Evac / Rescued / Lost vessel must be reflected in the telemetry
        // counts, not silently hidden behind the Active/Docked-only view.
        let cfg = test_cfg();
        let mut wrapper = SimStateWrapper::new(cfg).unwrap();
        wrapper.do_init();
        assert!(!wrapper.inner.vessels.is_empty());

        wrapper.inner.vessels[0].state = VesselState::Evac;
        if wrapper.inner.vessels.len() > 1 {
            wrapper.inner.vessels[1].state = VesselState::Lost;
        }

        let snap = wrapper.inner.build_snapshot();
        let v: serde_json::Value = serde_json::from_str(&snap).unwrap();
        assert_eq!(v["telemetry"]["evac_count"], 1);
        if wrapper.inner.vessels.len() > 1 {
            assert_eq!(v["telemetry"]["lost_count"], 1);
        }
    }

    #[test]
    fn test_terminal_vessels_are_reaped_and_counted() {
        let cfg = test_cfg();
        let mut state = SimState::new(cfg).unwrap();
        let mk = |id: u64, st: VesselState| VesselAgent {
            id,
            name: format!("V{id}"),
            vessel_type: "cargo".into(),
            route: vec![],
            route_index: 0,
            route_direction: 1,
            position: (100.0, 100.0),
            last_valid_position: (100.0, 100.0),
            heading_deg: 0.0,
            speed_kn: 12.0,
            state: st,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            anchored: false,
            follow_speed_cap: None,
            fatigue: 0.0,
        };
        state.vessels.push(mk(1, VesselState::Lost));
        state.vessels.push(mk(2, VesselState::Rescued));
        state.vessels.push(mk(3, VesselState::Active));

        state.reap_terminal_vessels();

        assert_eq!(state.vessels.len(), 1, "only the Active vessel survives");
        assert_eq!(state.vessels[0].id, 3);
        assert_eq!(state.lost_cumulative, 1);
        assert_eq!(state.rescued_cumulative, 1);
    }

    #[test]
    fn test_simcol_founder_produces_fatalities_and_loss() {
        // Two heavy vessels overlap with perpendicular headings (a T-bone) and
        // empty routes, so they do not navigate away before the collision sweep.
        // SIMCOL should founder both, kill crew, and the reaper should respawn
        // the fleet while tallying the losses.
        let mut cfg = test_cfg();
        cfg.n_vessels = 2;
        let mut state = SimState::new(cfg).unwrap();
        let mk = |id: u64, hdg: f64| VesselAgent {
            id,
            name: format!("V{id}"),
            vessel_type: "cargo".into(),
            route: vec![], // empty → navigate() is a no-op, vessels stay overlapped
            route_index: 0,
            route_direction: 1,
            position: (300.0, 300.0),
            last_valid_position: (300.0, 300.0),
            heading_deg: hdg,
            speed_kn: 20.0,
            state: VesselState::Active,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            anchored: false,
            follow_speed_cap: None,
            fatigue: 0.0,
        };
        state.vessels.push(mk(1, 90.0)); // heading east
        state.vessels.push(mk(2, 180.0)); // heading south → strikes V1's side

        let records = crate::ais::load_ais(&state.config.ais_path).unwrap();
        state.run_tick(&records);

        assert!(
            state.kpi.collision_events >= 1,
            "a hard collision is recorded"
        );
        assert!(
            state.kpi.fatal_crew > 0,
            "foundering crew suffer fatalities"
        );
        assert!(state.kpi.evac_events >= 1, "a founder triggers evacuation");
        // The foundered vessel enters the SAR chain and a rescue asset launches.
        let evac = state
            .vessels
            .iter()
            .filter(|v| v.state == VesselState::Evac)
            .count();
        assert!(evac >= 1, "a foundered vessel is awaiting rescue");
        assert!(
            !state.rescue_agents.is_empty(),
            "a rescue asset is dispatched to the datum"
        );
    }

    /// A vessel agent bound for `dest` (route endpoint in travel direction).
    fn mk_bound(id: u64, pos: (f64, f64), dest: (f64, f64), speed: f64) -> VesselAgent {
        VesselAgent {
            id,
            name: format!("V{id}"),
            vessel_type: "cargo".into(),
            // route_direction +1 → destination is the last waypoint.
            route: vec![(pos.0 + 5.0, pos.1), dest],
            route_index: 0,
            route_direction: 1,
            position: pos,
            last_valid_position: pos,
            heading_deg: 270.0,
            speed_kn: speed,
            state: VesselState::Active,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            anchored: false,
            follow_speed_cap: None,
            fatigue: 0.0,
        }
    }

    #[test]
    fn test_following_caps_speed_and_radios() {
        // Two vessels bound for the same destination, far from any port: the
        // faster trailing one is capped to the leader's speed and, drawing
        // close, holds to keep distance — and is radioed to do so.
        let cfg = test_cfg();
        let mut state = SimState::new(cfg).unwrap();
        let dest = (700.0, 700.0); // open water, well away from ports
                                   // Leader nearer the destination (slower); follower behind (faster).
        let leader = mk_bound(1, (650.0, 700.0), dest, 10.0);
        let follower = mk_bound(2, (649.7, 700.0), dest, 18.0);
        state.vessels.push(leader);
        state.vessels.push(follower);

        state.compute_following(0);

        assert_eq!(
            state.vessels[1].follow_speed_cap,
            Some(10.0),
            "follower matches the leader's speed (no overtaking)"
        );
        assert!(
            state.vessels[0].follow_speed_cap.is_none(),
            "leader is unconstrained"
        );
        assert!(state.vessels[1].anchored, "follower holds to keep distance");
        let radioed = state.comms_log.iter().any(|m| {
            matches!(m.kind, MsgKind::KeepDistance { .. }) && m.from_id == 1 && m.to_id == 2
        });
        assert!(
            radioed,
            "leader radios the follower to slow and keep distance"
        );
    }

    #[test]
    fn test_no_collision_within_port_exclusion() {
        // Two overlapping vessels inside the 1 nm port exclusion log no collision.
        let cfg = test_cfg();
        let mut state = SimState::new(cfg).unwrap();
        let port = state.ports[0].position;
        let at = (port.0 + 0.2, port.1); // 0.2 nm from the port, overlapping
        let mut a = mk_bound(1, at, (port.0 + 50.0, port.1), 12.0);
        let mut b = mk_bound(2, at, (port.0 + 50.0, port.1), 12.0);
        a.route = vec![]; // empty route → no navigation, stay overlapped
        b.route = vec![];
        state.vessels.push(a);
        state.vessels.push(b);

        let records = crate::ais::load_ais(&state.config.ais_path).unwrap();
        state.run_tick(&records);
        assert_eq!(
            state.kpi.collision_events, 0,
            "encounters within the port exclusion are not collisions"
        );
    }

    #[test]
    fn test_sar_dispatch_resolves_to_rescue() {
        // A foundered vessel a short hop from a shore base is recovered, the
        // rescue TTA is recorded, and the vessel transitions Evac → Rescued.
        let cfg = test_cfg();
        let mut state = SimState::new(cfg).unwrap();
        assert!(!state.ports.is_empty(), "test fixture needs ports");
        let base = state.ports[0].position;
        let datum = (base.0 + 5.0, base.1); // ~5 nm offshore → patrol vessel

        state.vessels.push(VesselAgent {
            id: 1,
            name: "Foundered".into(),
            vessel_type: "cargo".into(),
            route: vec![],
            route_index: 0,
            route_direction: 1,
            position: datum,
            last_valid_position: datum,
            heading_deg: 0.0,
            speed_kn: 0.0,
            state: VesselState::Evac,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            anchored: false,
            follow_speed_cap: None,
            fatigue: 0.0,
        });

        // Advance the SAR subsystem until the case resolves.
        for t in 0..50 {
            state.run_sar(t);
            if state.vessels[0].state != VesselState::Evac {
                break;
            }
        }

        assert_eq!(
            state.vessels[0].state,
            VesselState::Rescued,
            "a nearby datum within the survival window is rescued"
        );
        assert_eq!(state.kpi.rescue_count, 1, "the rescue is counted");
        assert!(
            state.kpi.snapshot(0).avg_tta_hours > 0.0,
            "rescue TTA is positive"
        );
    }

    #[test]
    fn test_ashrafi_winter_rescue_is_slower_than_summer() {
        // The same datum is reached more slowly in January (Group D) than in
        // July (Group A): the Ashrafi seasonal degradation of transit speed.
        let arrival_tta = |month: u8| -> f64 {
            let mut cfg = test_cfg();
            cfg.sim_month = month;
            let mut state = SimState::new(cfg).unwrap();
            let base = state.ports[0].position;
            let datum = (base.0 + 30.0, base.1); // ~30 nm → patrol vessel
            state.vessels.push(VesselAgent {
                id: 1,
                name: "Foundered".into(),
                vessel_type: "cargo".into(),
                route: vec![],
                route_index: 0,
                route_direction: 1,
                position: datum,
                last_valid_position: datum,
                heading_deg: 0.0,
                speed_kn: 0.0,
                state: VesselState::Evac,
                n_crew: 10,
                dock_until_tick: None,
                last_port_id: None,
                avoidance_wps: vec![],
                avoidance_cooldown: 0,
                avoiding_vessel_id: None,
                avoided_vessel_id: None,
                avoiding: false,
                anchored: false,
                follow_speed_cap: None,
                fatigue: 0.0,
            });
            // Run until the asset reaches the datum (rescue count recorded).
            for t in 0..400 {
                state.run_sar(t);
                if state.kpi.rescue_count > 0 {
                    break;
                }
            }
            state.kpi.snapshot(0).avg_tta_hours
        };

        let summer = arrival_tta(7);
        let winter = arrival_tta(1);
        assert!(summer > 0.0 && winter > 0.0, "both seasons reach the datum");
        assert!(
            winter > summer * 2.0,
            "winter transit (Group D) is much slower: winter={winter}h summer={summer}h"
        );
    }

    #[test]
    fn test_sar_short_survival_window_is_lost() {
        // With a very short liferaft survival window the foundered vessel is
        // lost before it can be searched out — the latency → survival link.
        let mut cfg = test_cfg();
        cfg.sar.liferaft_survival_ticks = 1;
        cfg.sar.mobilisation_delay_ticks = 1;
        let mut state = SimState::new(cfg).unwrap();
        let base = state.ports[0].position;
        // Place the datum far offshore so the asset cannot arrive in time.
        let datum = (base.0 + 600.0, base.1);

        state.vessels.push(VesselAgent {
            id: 1,
            name: "Foundered".into(),
            vessel_type: "cargo".into(),
            route: vec![],
            route_index: 0,
            route_direction: 1,
            position: datum,
            last_valid_position: datum,
            heading_deg: 0.0,
            speed_kn: 0.0,
            state: VesselState::Evac,
            n_crew: 10,
            dock_until_tick: None,
            last_port_id: None,
            avoidance_wps: vec![],
            avoidance_cooldown: 0,
            avoiding_vessel_id: None,
            avoided_vessel_id: None,
            avoiding: false,
            anchored: false,
            follow_speed_cap: None,
            fatigue: 0.0,
        });

        for t in 0..20 {
            state.run_sar(t);
            if state.vessels[0].state != VesselState::Evac {
                break;
            }
        }

        assert_eq!(
            state.vessels[0].state,
            VesselState::Lost,
            "an unreachable datum past the survival window is lost"
        );
        assert!(!state.wrecks.is_empty(), "a wreck marker is recorded");
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
            anchored: false,
            follow_speed_cap: None,
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
