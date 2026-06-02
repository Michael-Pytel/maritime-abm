use crate::ais::{self, Port};
use crate::land::LandMask;
use crate::scenario::{Method, SimConfig};
use serde::Serialize;

// ── Fatigue constants ─────────────────────────────────────────────────────────

/// Base fatigue accumulation per tick (15 min) while actively sailing.
/// ~0.144 over 24 h in completely calm conditions.
const FATIGUE_BASE_RATE: f64 = 0.0015;

/// Additional fatigue per unit of weather hazard W, per tick.
/// At W=1 this adds 0.38 over 24 h on top of the base.
const FATIGUE_WEATHER_RATE: f64 = 0.0040;

/// Peak amplitude of the circadian fatigue bonus (at ~03:00 UTC).
/// Adds at most 0.077 over 24 h.
const FATIGUE_CIRCADIAN_PEAK: f64 = 0.0008;

/// Fatigue recovered per tick while docked.
/// Full recovery (0 → 1.0) in ~40 ticks ≈ 10 h in port.
const FATIGUE_RECOVERY_RATE: f64 = 0.025;

/// Fatigue level above which the crew comms rate starts to degrade.
const FATIGUE_COMMS_THRESHOLD: f64 = 0.40;

/// Maximum fractional comms rate reduction due to crew fatigue.
/// At fatigue = 1.0 the rate is multiplied by (1 − `FATIGUE_COMMS_PENALTY`).
const FATIGUE_COMMS_PENALTY: f64 = 0.65;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VesselState {
    Active,
    Docked,
}

#[derive(Debug, Clone, Serialize)]
pub struct VesselAgent {
    pub id: u64,
    pub name: String,
    pub vessel_type: String,
    pub route: Vec<(f64, f64)>,
    pub route_index: usize,
    pub route_direction: i8,
    pub position: (f64, f64),
    pub last_valid_position: (f64, f64),
    pub heading_deg: f64,
    pub speed_kn: f64,
    pub state: VesselState,
    pub n_crew: u32,
    pub dock_until_tick: Option<u64>,

    /// ID of the last port this vessel docked at — prevents immediately
    /// re-docking at the departure port before the vessel has moved away.
    #[serde(skip)]
    pub last_port_id: Option<u32>,

    // ── Collision-avoidance fields ─────────────────────────────────────────
    /// Temporary avoidance waypoints injected by `PostTickAgent`.  Consumed
    /// from the front before normal route waypoints are targeted.
    #[serde(skip)]
    pub avoidance_wps: Vec<(f64, f64)>,

    /// Ticks remaining before this vessel can accept a new avoidance manoeuvre.
    /// Counts down in `PostTickAgent` each tick.
    #[serde(skip)]
    pub avoidance_cooldown: u32,

    /// ID of the vessel this one is currently giving way to.  Set when an
    /// avoidance waypoint is injected; cleared when the manoeuvre completes.
    #[serde(skip)]
    pub avoiding_vessel_id: Option<u64>,

    /// Set (transiently) when the last avoidance waypoint has just been
    /// reached.  `PostTickAgent` reads this, logs a `ResumeRoute` message,
    /// then clears it.
    #[serde(skip)]
    pub avoided_vessel_id: Option<u64>,

    /// `true` while the vessel is executing an avoidance manoeuvre.  Included
    /// in the WebSocket snapshot so the frontend can highlight the ship.
    pub avoiding: bool,

    // ── Crew fatigue ──────────────────────────────────────────────────────
    /// Crew fatigue level ∈ [0, 1].
    /// 0 = fully rested; 1 = exhausted.
    /// Accumulates while sailing (faster in bad weather, worse at night).
    /// Recovers during port dwell.  Not serialised — purely internal.
    #[serde(skip)]
    pub fatigue: f64,
}

/// Returns a realistic cruising speed (knots) for the given vessel type.
#[must_use]
pub fn typical_speed_kn(vessel_type: &str, rand_frac: f64) -> f64 {
    match vessel_type.to_lowercase().as_str() {
        "passenger" | "ferry" | "ro-ro" | "ro-pax" => 15.0 + rand_frac * 8.0,
        "tanker" => 10.0 + rand_frac * 4.0,
        "cargo" | "container" | "bulk" | "general cargo" => 10.0 + rand_frac * 6.0,
        _ => 10.0 + rand_frac * 10.0,
    }
}

impl VesselAgent {
    pub fn step(&mut self, tick: u64, config: &SimConfig, land_mask: &LandMask, ports: &[Port]) {
        match self.state {
            VesselState::Docked => {
                if let Some(until) = self.dock_until_tick {
                    if tick >= until {
                        self.state = VesselState::Active;
                        self.dock_until_tick = None;
                    }
                }
            }
            VesselState::Active => {
                self.navigate(tick, config, land_mask, ports);
            }
        }
    }

    fn navigate(&mut self, tick: u64, config: &SimConfig, land_mask: &LandMask, ports: &[Port]) {
        if self.route.is_empty() {
            return;
        }

        // ── Choose current target ─────────────────────────────────────────
        // Avoidance waypoints take priority over the regular route.
        let (target, is_avoidance) = if let Some(&wp) = self.avoidance_wps.first() {
            (wp, true)
        } else {
            (self.route[self.route_index], false)
        };

        let dx = target.0 - self.position.0;
        let dy = target.1 - self.position.1;
        let dist = (dx * dx + dy * dy).sqrt();

        // ── Waypoint reached ──────────────────────────────────────────────
        if dist < 0.5 {
            if is_avoidance {
                // Consume the avoidance waypoint.
                self.avoidance_wps.remove(0);
                if self.avoidance_wps.is_empty() {
                    // Signal PostTickAgent to log the ResumeRoute message.
                    self.avoided_vessel_id = self.avoiding_vessel_id.take();
                    self.avoiding = false;
                }
                return;
            }

            // Normal route endpoint reached.
            let at_endpoint = match self.route_direction {
                d if d > 0 => self.route_index + 1 >= self.route.len(),
                _ => self.route_index == 0,
            };
            if at_endpoint {
                self.begin_dock(tick, config, true);
                return;
            }
            let next_opt = if self.route_direction > 0 {
                self.route_index.checked_add(1)
            } else {
                self.route_index.checked_sub(1)
            };
            if let Some(next) = next_opt.filter(|&n| n < self.route.len()) {
                self.route_index = next;
            }
            return;
        }

        // ── Move toward target ────────────────────────────────────────────
        // 1 tick = 15 min = 0.25 h
        let speed_nm_per_tick = self.speed_kn * 0.25;
        let full_step = speed_nm_per_tick.min(dist);

        // Sub-step halving against land mask (up to 5 halvings).
        let mut accepted: Option<(f64, f64)> = None;
        let mut frac = 1.0_f64;
        for _ in 0..5 {
            let step = full_step * frac;
            let nx = self.position.0 + dx / dist * step;
            let ny = self.position.1 + dy / dist * step;
            if !land_mask.segment_crosses_land(self.position, (nx, ny)) {
                accepted = Some((nx, ny));
                break;
            }
            frac *= 0.5;
        }

        if let Some((nx, ny)) = accepted {
            self.position = (nx, ny);
            self.last_valid_position = (nx, ny);
            self.heading_deg = (90.0 - dy.atan2(dx).to_degrees()).rem_euclid(360.0);

            // ── Port-polygon entry check ──────────────────────────────────
            // Only check when NOT executing an avoidance manoeuvre — we don't
            // want a temporary waypoint to accidentally trigger docking.
            if !is_avoidance {
                for port in ports {
                    if self.last_port_id == Some(port.id) {
                        continue; // still leaving departure port
                    }
                    if !port.polygon.is_empty() && ais::point_in_polygon(nx, ny, &port.polygon) {
                        self.last_port_id = Some(port.id);
                        self.begin_dock(tick, config, false);
                        return;
                    }
                }
            }
        } else {
            // Every sub-step blocked by land — skip to the next waypoint.
            if is_avoidance {
                // Give up on this avoidance waypoint rather than getting stuck.
                self.avoidance_wps.remove(0);
                if self.avoidance_wps.is_empty() {
                    self.avoided_vessel_id = self.avoiding_vessel_id.take();
                    self.avoiding = false;
                }
            } else {
                let next_opt = if self.route_direction > 0 {
                    self.route_index.checked_add(1)
                } else {
                    self.route_index.checked_sub(1)
                };
                if let Some(next) = next_opt.filter(|&n| n < self.route.len()) {
                    self.route_index = next;
                } else {
                    self.begin_dock(tick, config, true);
                }
            }
        }
    }

    /// Transition the vessel to `Docked` and set a dwell timer.
    ///
    /// `snap_to_waypoint`: snap position to the current route waypoint
    /// (used for fallback endpoint-docking).  Pass `false` when docking was
    /// triggered by polygon entry — position is already valid.
    fn begin_dock(&mut self, tick: u64, config: &SimConfig, snap_to_waypoint: bool) {
        if snap_to_waypoint {
            self.position = self.route[self.route_index];
            self.last_valid_position = self.position;
        }
        self.state = VesselState::Docked;

        // Clear any avoidance state so the return voyage starts clean.
        self.avoidance_wps.clear();
        self.avoidance_cooldown = 0;
        self.avoiding_vessel_id = None;
        self.avoided_vessel_id = None;
        self.avoiding = false;

        let span = config
            .port_dwell_max_ticks
            .saturating_sub(config.port_dwell_min_ticks)
            .max(1);
        let r = pseudo_rand(self.id.wrapping_add(0xD0C0), tick);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let dwell = config.port_dwell_min_ticks + (r * f64::from(span)) as u32;
        self.dock_until_tick = Some(tick + u64::from(dwell));

        // Reverse travel direction for the return voyage.
        self.route_direction = -self.route_direction;
        let next_opt = if self.route_direction > 0 {
            self.route_index.checked_add(1)
        } else {
            self.route_index.checked_sub(1)
        };
        if let Some(next) = next_opt.filter(|&n| n < self.route.len()) {
            self.route_index = next;
        }
    }

    // ── Fatigue ───────────────────────────────────────────────────────────────

    /// Update crew fatigue for one tick.
    ///
    /// * `w`         — weather hazard at the vessel's current position ∈ [0, 1].
    /// * `utc_hour`  — current simulation UTC hour (0–24).
    /// * `method`    — determines whether crew scheduling mitigates build-up.
    pub fn tick_fatigue(&mut self, w: f64, utc_hour: f64, method: Method) {
        match self.state {
            VesselState::Docked => {
                // Rest and recover in port.
                self.fatigue = (self.fatigue - FATIGUE_RECOVERY_RATE).max(0.0);
            }
            VesselState::Active => {
                // Circadian factor: peaks at ~03:00 UTC (trough at 15:00).
                // f(t) = 0.5 * (1 − cos(2π(t − 3) / 24))  →  [0, 1]
                let phase = 2.0 * std::f64::consts::PI * (utc_hour - 3.0) / 24.0;
                let circadian = 0.5 * (1.0 - phase.cos());

                let build = FATIGUE_BASE_RATE
                    + FATIGUE_WEATHER_RATE * w
                    + FATIGUE_CIRCADIAN_PEAK * circadian;

                // Methods with crew-management systems slow fatigue build-up.
                let method_factor = match method {
                    Method::BaselineA => 1.00,      // unmanaged
                    Method::BaselineB => 0.82,      // shore rest alerts
                    Method::ProposedSystem => 0.65, // smart watch scheduling
                };

                self.fatigue = (self.fatigue + build * method_factor).min(1.0);
            }
        }
    }

    /// Fractional comms-rate multiplier due to crew fatigue.
    ///
    /// Returns 1.0 until fatigue exceeds `FATIGUE_COMMS_THRESHOLD`, then
    /// linearly decays to `(1 − FATIGUE_COMMS_PENALTY)` at fatigue = 1.0.
    #[must_use]
    pub fn fatigue_comms_factor(&self) -> f64 {
        if self.fatigue <= FATIGUE_COMMS_THRESHOLD {
            return 1.0;
        }
        let excess = (self.fatigue - FATIGUE_COMMS_THRESHOLD) / (1.0 - FATIGUE_COMMS_THRESHOLD);
        (1.0 - FATIGUE_COMMS_PENALTY * excess).max(1.0 - FATIGUE_COMMS_PENALTY)
    }
}

impl krabmaga::engine::agent::Agent for VesselAgent {
    fn step(&mut self, state: &mut dyn krabmaga::engine::state::State) {
        let wrapper = state
            .as_any_mut()
            .downcast_mut::<crate::state::SimStateWrapper>()
            .unwrap();
        let tick = wrapper.inner.step;
        let Some(idx) = wrapper.inner.vessels.iter().position(|v| v.id == self.id) else {
            return;
        };
        let config = wrapper.inner.config.clone();
        let land_mask = wrapper.inner.land_mask.clone();
        let ports = wrapper.inner.ports.clone();
        wrapper.inner.vessels[idx].step(tick, &config, &land_mask, &ports);
        *self = wrapper.inner.vessels[idx].clone();
    }

    fn is_stopped(&mut self, state: &mut dyn krabmaga::engine::state::State) -> bool {
        let wrapper = state
            .as_any_mut()
            .downcast_mut::<crate::state::SimStateWrapper>()
            .unwrap();
        !wrapper.inner.vessels.iter().any(|v| v.id == self.id)
    }
}

#[allow(clippy::cast_precision_loss)]
fn pseudo_rand(id: u64, tick: u64) -> f64 {
    let mut x = id
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(tick.wrapping_mul(1_442_695_040_888_963_407));
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 31;
    (x as f64) / (u64::MAX as f64)
}

impl std::fmt::Display for VesselAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vessel({})", self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::SimConfig;

    fn make_vessel(id: u64, route: Vec<(f64, f64)>) -> VesselAgent {
        let pos = route.first().copied().unwrap_or((0.0, 0.0));
        VesselAgent {
            id,
            name: format!("V{id}"),
            vessel_type: "cargo".into(),
            route,
            route_index: 0,
            route_direction: 1,
            position: pos,
            last_valid_position: pos,
            heading_deg: 0.0,
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
        }
    }

    #[test]
    fn advances_toward_waypoint() {
        let cfg = SimConfig::default();
        let mut v = make_vessel(1, vec![(0.0, 0.0), (100.0, 0.0)]);
        v.route_index = 1;
        v.step(0, &cfg, &LandMask::empty(), &[]);
        assert!(v.position.0 > 0.0 && v.position.0 < 100.0);
    }

    #[test]
    fn docks_at_endpoint() {
        let cfg = SimConfig::default();
        let mut v = make_vessel(2, vec![(0.0, 0.0), (0.3, 0.0)]);
        v.route_index = 1;
        v.position = (0.1, 0.0);
        v.step(0, &cfg, &LandMask::empty(), &[]);
        assert_eq!(v.state, VesselState::Docked);
        assert!(v.dock_until_tick.is_some());
    }

    #[test]
    fn undocks_when_dwell_expires() {
        let cfg = SimConfig::default();
        let mut v = make_vessel(3, vec![(0.0, 0.0), (10.0, 0.0)]);
        v.state = VesselState::Docked;
        v.dock_until_tick = Some(5);
        v.step(3, &cfg, &LandMask::empty(), &[]);
        assert_eq!(v.state, VesselState::Docked);
        v.step(5, &cfg, &LandMask::empty(), &[]);
        assert_eq!(v.state, VesselState::Active);
    }

    #[test]
    fn docks_on_port_polygon_entry() {
        use crate::ais::Port;
        let cfg = SimConfig::default();
        let route = vec![(0.0, 0.0), (100.0, 0.0)];
        let mut v = make_vessel(4, route);
        v.position = (49.0, 0.0);
        v.route_index = 1;
        let port = Port {
            id: 77,
            name: "TestPort".into(),
            position: (52.0, 0.0),
            polygon: vec![(49.0, -1.0), (49.0, 1.0), (55.0, 1.0), (55.0, -1.0)],
        };
        v.step(0, &cfg, &LandMask::empty(), &[port]);
        assert_eq!(v.state, VesselState::Docked, "should dock on polygon entry");
        assert_eq!(v.last_port_id, Some(77));
    }

    #[test]
    fn avoidance_wp_consumed_before_route() {
        // Vessel heading toward (100, 0) but with an avoidance WP at (5, 2).
        // It should head to (5, 2) first, not (100, 0).
        let cfg = SimConfig::default();
        let mut v = make_vessel(5, vec![(0.0, 0.0), (100.0, 0.0)]);
        v.route_index = 1;
        v.position = (0.0, 0.0);
        v.avoidance_wps = vec![(5.0, 2.0)];
        v.avoiding = true;

        v.step(0, &cfg, &LandMask::empty(), &[]);

        // Vessel should have moved toward (5, 2), not directly toward (100, 0)
        assert!(
            v.position.1 > 0.0,
            "vessel should have moved north (toward avoidance WP y=2), got y={}",
            v.position.1
        );
    }

    #[test]
    fn avoidance_wp_consumed_clears_avoiding_flag() {
        let cfg = SimConfig::default();
        let mut v = make_vessel(6, vec![(0.0, 0.0), (100.0, 0.0)]);
        v.route_index = 1;
        // Place avoidance WP very close so it's reached this tick.
        v.position = (0.0, 0.0);
        v.avoidance_wps = vec![(0.1, 0.0)];
        v.avoiding = true;
        v.avoiding_vessel_id = Some(999);

        v.step(0, &cfg, &LandMask::empty(), &[]);

        assert!(
            !v.avoiding,
            "avoiding flag should be cleared after WP consumed"
        );
        assert!(
            v.avoided_vessel_id == Some(999),
            "avoided_vessel_id should capture the other vessel"
        );
        assert!(v.avoiding_vessel_id.is_none());
    }
}
