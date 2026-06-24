//! IWRAP Mk II external collision-frequency validation (offline).
//!
//! Anchors the model's relative KPIs to a regulator-recognised absolute scale by
//! computing the expected number of collisions on the waterway network,
//! `N_c = N_a · P_c`, where `N_a` is the geometric candidate count and `P_c` the
//! causation probability (Friis-Hansen IWRAP; Lusic & Ćorić 2015). Three
//! encounter types are summed: head-on, overtaking, and crossing.
//!
//! All formulas use consistent units — lengths/beams/width in nautical miles,
//! speeds in knots, flows in ships per hour, time in hours — so `N_a` is a pure
//! (dimensionless) expected count. To report collisions per year the analyzer
//! evaluates over a one-year horizon.
//!
//! Constants verified against the source PDF (`models-for-estimating…`, Table 1
//! and Eqs. 4, 5, 11, 12, 14). Note the IWRAP-default causation probabilities
//! are head-on `0.50e-4`, crossing `1.30e-4`, overtaking `1.10e-4`.

/// IWRAP-default causation probability for head-on encounters.
pub const PC_HEAD_ON: f64 = 0.50e-4;
/// IWRAP-default causation probability for crossing encounters.
pub const PC_CROSSING: f64 = 1.30e-4;
/// IWRAP-default causation probability for overtaking encounters.
pub const PC_OVERTAKING: f64 = 1.10e-4;

/// Hours in a (365-day) year, the horizon for per-year frequencies.
pub const HOURS_PER_YEAR: f64 = 8760.0;

/// One ship class on a traffic flow: flow rate, beam, length, speed.
#[derive(Debug, Clone, Copy)]
pub struct ShipFlow {
    /// Traffic flow (ships per hour).
    pub flow_per_h: f64,
    /// Beam (nm).
    pub beam_nm: f64,
    /// Length (nm).
    pub length_nm: f64,
    /// Speed (knots).
    pub speed_kn: f64,
}

// ── Candidate-count formulas (uniform lateral distribution) ──────────────────

/// Head-on candidate count on a leg (Eq. 4): two opposing flows of classes `i`,
/// `j` over a leg of sailing distance `d_nm` and fairway width `w_nm` in time
/// `t_h`.
#[must_use]
pub fn n_a_head_on(d_nm: f64, w_nm: f64, t_h: f64, flow1: &[ShipFlow], flow2: &[ShipFlow]) -> f64 {
    let mut sum = 0.0;
    for a in flow1 {
        for b in flow2 {
            if a.speed_kn <= 0.0 || b.speed_kn <= 0.0 {
                continue;
            }
            sum +=
                a.flow_per_h * b.flow_per_h * (a.beam_nm + b.beam_nm) * (a.speed_kn + b.speed_kn)
                    / (a.speed_kn * b.speed_kn);
        }
    }
    (d_nm * t_h / w_nm) * sum
}

/// Overtaking candidate count on a leg (Eq. 5): as head-on but with the speed
/// *difference* `|v_i − v_j|` (same-direction traffic).
#[must_use]
pub fn n_a_overtaking(
    d_nm: f64,
    w_nm: f64,
    t_h: f64,
    flow1: &[ShipFlow],
    flow2: &[ShipFlow],
) -> f64 {
    let mut sum = 0.0;
    for a in flow1 {
        for b in flow2 {
            if a.speed_kn <= 0.0 || b.speed_kn <= 0.0 {
                continue;
            }
            sum += a.flow_per_h
                * b.flow_per_h
                * (a.beam_nm + b.beam_nm)
                * (a.speed_kn - b.speed_kn).abs()
                / (a.speed_kn * b.speed_kn);
        }
    }
    (d_nm * t_h / w_nm) * sum
}

/// Relative speed of two crossing flows at course-crossing angle `theta` (Eq. 11).
#[must_use]
pub fn relative_speed(vi: f64, vj: f64, theta: f64) -> f64 {
    (vi * vi + vj * vj - 2.0 * vi * vj * theta.cos())
        .max(0.0)
        .sqrt()
}

/// Pedersen (1995) collision diameter for a crossing encounter (Eq. 12).
#[must_use]
pub fn collision_diameter(a: &ShipFlow, b: &ShipFlow, theta: f64, vij: f64) -> f64 {
    if vij <= 0.0 {
        return 0.0;
    }
    let s = theta.sin();
    let term1 = (a.length_nm * b.speed_kn + b.length_nm * a.speed_kn) * s / vij;
    let t2 = b.beam_nm * (1.0 - (a.speed_kn * s / vij).powi(2)).max(0.0).sqrt();
    let t3 = a.beam_nm * (1.0 - (b.speed_kn * s / vij).powi(2)).max(0.0).sqrt();
    term1 + t2 + t3
}

/// Crossing candidate count for two flows meeting at angle `theta` (Eq. 14).
/// `theta` should lie in (10°, 170°) for the formula to be valid.
#[must_use]
pub fn n_a_crossing(t_h: f64, theta: f64, flow1: &[ShipFlow], flow2: &[ShipFlow]) -> f64 {
    let s = theta.sin();
    if s.abs() < 1e-9 {
        return 0.0;
    }
    let mut sum = 0.0;
    for a in flow1 {
        for b in flow2 {
            if a.speed_kn <= 0.0 || b.speed_kn <= 0.0 {
                continue;
            }
            let vij = relative_speed(a.speed_kn, b.speed_kn, theta);
            if vij < 1e-9 {
                continue;
            }
            let dij = collision_diameter(a, b, theta, vij);
            sum += a.flow_per_h * b.flow_per_h / (a.speed_kn * b.speed_kn) * vij * dij;
        }
    }
    (t_h / s) * sum
}

/// Expected collisions from a candidate count and causation probability (Eq. 1).
#[must_use]
pub fn n_c(n_a: f64, p_c: f64) -> f64 {
    n_a * p_c
}

// ── Waterway network model ───────────────────────────────────────────────────

/// A straight waterway leg with two opposing flows of equal composition
/// (vessels reverse at route ends), plus its geometry.
#[derive(Debug, Clone)]
pub struct WaterwayLeg {
    /// Endpoints in field coordinates (nm).
    pub a: (f64, f64),
    pub b: (f64, f64),
    /// Fairway width (nm).
    pub width_nm: f64,
    /// Per-class flows on the leg (one direction; the opposing flow mirrors it).
    pub flows: Vec<ShipFlow>,
}

impl WaterwayLeg {
    #[must_use]
    pub fn length_nm(&self) -> f64 {
        let dx = self.b.0 - self.a.0;
        let dy = self.b.1 - self.a.1;
        (dx * dx + dy * dy).sqrt()
    }

    fn direction(&self) -> (f64, f64) {
        let len = self.length_nm().max(1e-9);
        ((self.b.0 - self.a.0) / len, (self.b.1 - self.a.1) / len)
    }
}

/// Aggregated IWRAP result over a network.
#[derive(Debug, Clone, Copy, Default)]
pub struct IwrapReport {
    pub n_a_head_on: f64,
    pub n_a_overtaking: f64,
    pub n_a_crossing: f64,
    /// Expected collisions per year, `Σ N_a · P_c`.
    pub n_c_per_year: f64,
}

/// Crossing angle (radians, in `(0, π/2]`) between two leg directions.
fn crossing_angle(d1: (f64, f64), d2: (f64, f64)) -> f64 {
    let dot = (d1.0 * d2.0 + d1.1 * d2.1).clamp(-1.0, 1.0);
    let mut theta = dot.acos();
    if theta > std::f64::consts::FRAC_PI_2 {
        theta = std::f64::consts::PI - theta; // fold to the acute crossing angle
    }
    theta
}

/// Do two segments properly intersect? (used to find crossing leg pairs).
fn segments_cross(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    let d = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (b.0 - a.0) * (o.1 - a.1) - (b.1 - a.1) * (o.0 - a.0)
    };
    let d1 = d(p1, p3, p4);
    let d2 = d(p2, p3, p4);
    let d3 = d(p3, p1, p2);
    let d4 = d(p4, p1, p2);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Analyse a waterway network over a horizon of `period_h` hours.
///
/// Head-on and overtaking candidates are summed per leg (opposing/co-directional
/// flows); crossing candidates are summed over every pair of legs that intersect
/// at a valid crossing angle (10°–170°). `N_c` is reported per year.
#[must_use]
pub fn analyze(legs: &[WaterwayLeg], period_h: f64) -> IwrapReport {
    let mut n_head = 0.0;
    let mut n_over = 0.0;
    let mut n_cross = 0.0;

    for leg in legs {
        let d = leg.length_nm();
        if d <= 0.0 || leg.width_nm <= 0.0 {
            continue;
        }
        // Opposing / co-directional flows have the same composition.
        n_head += n_a_head_on(d, leg.width_nm, period_h, &leg.flows, &leg.flows);
        n_over += n_a_overtaking(d, leg.width_nm, period_h, &leg.flows, &leg.flows);
    }

    let lo = 10.0_f64.to_radians();
    let hi = 170.0_f64.to_radians();
    for i in 0..legs.len() {
        for j in (i + 1)..legs.len() {
            if !segments_cross(legs[i].a, legs[i].b, legs[j].a, legs[j].b) {
                continue;
            }
            let theta = crossing_angle(legs[i].direction(), legs[j].direction());
            if theta < lo || theta > hi {
                continue;
            }
            n_cross += n_a_crossing(period_h, theta, &legs[i].flows, &legs[j].flows);
        }
    }

    // Scale candidate counts to a one-year horizon, then apply P_c.
    let year_scale = HOURS_PER_YEAR / period_h.max(1e-9);
    let n_head_y = n_head * year_scale;
    let n_over_y = n_over * year_scale;
    let n_cross_y = n_cross * year_scale;
    IwrapReport {
        n_a_head_on: n_head_y,
        n_a_overtaking: n_over_y,
        n_a_crossing: n_cross_y,
        n_c_per_year: n_c(n_head_y, PC_HEAD_ON)
            + n_c(n_over_y, PC_OVERTAKING)
            + n_c(n_cross_y, PC_CROSSING),
    }
}

/// Build a single-class waterway network from hand-authored routes.
///
/// Each route polyline becomes a chain of legs. The fleet is shared evenly over
/// the routes; a vessel of mean speed `speed_kn` traverses its route in
/// `route_length / speed` hours, so the per-direction flow on each of the
/// route's legs is `(fleet / routes) · speed / (2 · route_length)` ships/hour.
#[must_use]
pub fn legs_from_routes(
    routes: &[Vec<(f64, f64)>],
    fleet: u32,
    speed_kn: f64,
    beam_nm: f64,
    length_nm: f64,
    width_nm: f64,
) -> Vec<WaterwayLeg> {
    let route_count = routes.iter().filter(|r| r.len() >= 2).count().max(1);
    #[allow(clippy::cast_precision_loss)]
    let vessels_per_route = f64::from(fleet) / route_count as f64;

    let mut legs = Vec::new();
    for route in routes {
        if route.len() < 2 {
            continue;
        }
        let total_len: f64 = route
            .windows(2)
            .map(|w| {
                let dx = w[1].0 - w[0].0;
                let dy = w[1].1 - w[0].1;
                (dx * dx + dy * dy).sqrt()
            })
            .sum();
        if total_len <= 0.0 {
            continue;
        }
        // Per-direction flow (ships/hour) on this route's legs.
        let flow_per_h = vessels_per_route * speed_kn / (2.0 * total_len);
        let flow = ShipFlow {
            flow_per_h,
            beam_nm,
            length_nm,
            speed_kn,
        };
        for w in route.windows(2) {
            legs.push(WaterwayLeg {
                a: w[0],
                b: w[1],
                width_nm,
                flows: vec![flow],
            });
        }
    }
    legs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(q: f64, v: f64) -> ShipFlow {
        ShipFlow {
            flow_per_h: q,
            beam_nm: 0.02,   // ~37 m
            length_nm: 0.10, // ~185 m
            speed_kn: v,
        }
    }

    #[test]
    fn head_on_matches_hand_worked_value() {
        // Single class both flows: D=10, W=2, t=1, Q=0.5/h, B=0.02, v=12.
        // N_a = (D t / W)·Q²·(2B)·(2v)/v² = 5·0.25·0.04·24/144 = 5·0.25·0.04·0.1667
        //     = 0.0083333…
        let f = vec![flow(0.5, 12.0)];
        let got = n_a_head_on(10.0, 2.0, 1.0, &f, &f);
        let want = 5.0 * 0.25 * 0.04 * (24.0 / 144.0);
        assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
    }

    #[test]
    fn overtaking_zero_for_equal_speeds_positive_for_unequal() {
        let same = vec![flow(0.5, 12.0)];
        assert!(n_a_overtaking(10.0, 2.0, 1.0, &same, &same).abs() < 1e-12);
        let fast = vec![flow(0.5, 18.0)];
        let slow = vec![flow(0.5, 10.0)];
        assert!(n_a_overtaking(10.0, 2.0, 1.0, &fast, &slow) > 0.0);
    }

    #[test]
    fn crossing_is_largest_near_right_angle() {
        let f = vec![flow(0.5, 12.0)];
        let perp = n_a_crossing(1.0, std::f64::consts::FRAC_PI_2, &f, &f);
        let shallow = n_a_crossing(1.0, 20.0_f64.to_radians(), &f, &f);
        assert!(perp > 0.0 && shallow > 0.0);
        assert!(perp > shallow, "near-perpendicular crossings expose more");
    }

    #[test]
    fn n_c_applies_causation_probability() {
        assert!((n_c(2.0, PC_HEAD_ON) - 1.0e-4).abs() < 1e-12);
        // Crossing is the most dangerous encounter type per IWRAP defaults.
        let pcs = [PC_HEAD_ON, PC_OVERTAKING, PC_CROSSING];
        assert!(pcs.iter().copied().fold(f64::MIN, f64::max) - PC_CROSSING < 1e-12);
    }

    #[test]
    fn analyze_two_crossing_routes_is_finite_and_positive() {
        // Two routes crossing at right angles through the origin.
        let routes = vec![
            vec![(-50.0, 0.0), (50.0, 0.0)],
            vec![(0.0, -50.0), (0.0, 50.0)],
        ];
        let legs = legs_from_routes(&routes, 20, 12.0, 0.02, 0.10, 2.0);
        let rep = analyze(&legs, 720.0);
        assert!(rep.n_c_per_year.is_finite() && rep.n_c_per_year > 0.0);
        assert!(rep.n_a_crossing > 0.0, "the routes cross");
    }
}
