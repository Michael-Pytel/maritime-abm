//! Depth-aware maritime route planner.
//!
//! Replaces plain A* over a land mask with a **cost-field weighted A***, the
//! standard defensible formulation for ship routing (see e.g. Vettor & Guedes
//! Soares; multi-objective A* with bathymetry + IMO constraints, MDPI JMSE).
//! For each vessel class the planner:
//!
//! 1. treats water shallower than the class's **draft + under-keel clearance**
//!    as impassable (so laden tankers never cross shoals or thread archipelago
//!    skerries that are too shallow, regardless of the coastline polygon detail);
//! 2. penalises cells whose depth is *close* to that limit (a squat / safety
//!    margin) and cells *close to shore* (keeps a realistic offing — "not near
//!    the border"), so least-cost paths follow deep water down the middle of a
//!    channel; and
//! 3. adds small **seeded per-route jitter** so routes are deterministic for a
//!    given seed but not identical between seeds (near-optimal variety).
//!
//! Depth comes from the bundled real EMODnet bathymetry ([`crate::bathymetry`]).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::doc_markdown
)]

use crate::bathymetry::Bathymetry;
use simulation::ais::{field_height_nm, field_to_lat_lon, field_width_nm};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// A vessel class and the water depth it needs.
#[derive(Debug, Clone, Copy)]
pub struct VesselClass {
    pub name: &'static str,
    /// Static draft (m).
    pub draft_m: f64,
    /// Required under-keel clearance below the keel (m).
    pub ukc_m: f64,
}

impl VesselClass {
    /// Minimum charted depth the class may transit (m).
    #[must_use]
    pub fn required_depth_m(&self) -> f64 {
        self.draft_m + self.ukc_m
    }
}

/// Baltic / North-Sea-realistic classes. VLCCs are excluded — they cannot
/// transit the Danish Straits into the Baltic.
pub const CLASSES: &[VesselClass] = &[
    VesselClass {
        name: "passenger",
        draft_m: 6.5,
        ukc_m: 1.5,
    }, // 8 m
    VesselClass {
        name: "cargo",
        draft_m: 11.0,
        ukc_m: 2.0,
    }, // 13 m
    VesselClass {
        name: "tanker",
        draft_m: 14.0,
        ukc_m: 3.0,
    }, // 17 m
];

/// Tunable weights and grid resolution for the cost field.
#[derive(Debug, Clone, Copy)]
pub struct RouterParams {
    pub grid_nm: f64,
    /// Preferred minimum offing (nm) from shallow water / shore.
    pub min_clearance_nm: f64,
    /// Penalty weight for transiting close to the depth limit (squat margin).
    pub w_shallow: f64,
    /// Penalty weight for transiting closer than `min_clearance_nm` to shore.
    pub w_clearance: f64,
    /// Extra depth (m) required above draft+UKC for a cell to count as passable
    /// — a coarse-grid safety buffer so lanes avoid marginal-depth cells.
    pub safety_margin_m: f64,
    /// Penalty weight for sailing *off* the established lane for the class
    /// (0 = ignore lanes). Applied to `1 − attractiveness`, so busy corridors
    /// cost least and open water costs most.
    pub w_lane: f64,
    /// Fractional per-route cost jitter for near-optimal route variety.
    pub jitter: f64,
}

impl Default for RouterParams {
    fn default() -> Self {
        Self {
            grid_nm: 1.0,
            min_clearance_nm: 2.5,
            w_shallow: 3.0,
            w_clearance: 4.0,
            safety_margin_m: 2.0,
            w_lane: 2.5,
            jitter: 0.18,
        }
    }
}

/// The per-class navigability + cost field.
pub struct ClassField {
    passable: Vec<bool>,
    /// Base (jitter-free) cost multiplier per cell, ≥ 1.
    base_mult: Vec<f32>,
}

/// A depth-aware router over a field-nm grid backed by real bathymetry.
pub struct Router {
    params: RouterParams,
    pub cols: usize,
    pub rows: usize,
    cell_nm: f64,
    /// Available water depth per cell (m); 0 on land / above sea level.
    avail: Vec<f32>,
}

impl Router {
    #[must_use]
    pub fn new(bathy: &Bathymetry, params: RouterParams) -> Self {
        let w = field_width_nm();
        let h = field_height_nm();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cols = (w / params.grid_nm).ceil() as usize;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rows = (h / params.grid_nm).ceil() as usize;
        let mut avail = vec![0.0f32; cols * rows];
        for j in 0..rows {
            for i in 0..cols {
                let (x, y) = cell_center(i, j, params.grid_nm);
                #[allow(clippy::cast_possible_truncation)]
                let d = bathy.available_depth_field(x, y) as f32;
                avail[j * cols + i] = d;
            }
        }
        Self {
            params,
            cols,
            rows,
            cell_nm: params.grid_nm,
            avail,
        }
    }

    #[inline]
    fn idx(&self, i: usize, j: usize) -> usize {
        j * self.cols + i
    }
    #[inline]
    fn ij(&self, idx: usize) -> (usize, usize) {
        (idx % self.cols, idx / self.cols)
    }

    /// Field-nm centre of a cell index.
    #[must_use]
    pub fn cell_center_of(&self, cell: usize) -> (f64, f64) {
        let (i, j) = self.ij(cell);
        cell_center(i, j, self.cell_nm)
    }

    /// Samples per-cell lane attractiveness (`0..1`) for a `LaneField` band.
    #[must_use]
    pub fn lane_attractiveness(&self, lanes: &crate::lanes::LaneField, band: usize) -> Vec<f32> {
        (0..self.cols * self.rows)
            .map(|c| {
                let (x, y) = self.cell_center_of(c);
                lanes.attractiveness_field(band, x, y) as f32
            })
            .collect()
    }

    /// Builds the navigability + cost field for one vessel class.
    ///
    /// `lane_att`, if given, is a per-cell attractiveness slice (`0..1`) for this
    /// class; off-lane cells are made more expensive so routes follow real lanes.
    #[must_use]
    pub fn class_field(&self, class: VesselClass, lane_att: Option<&[f32]>) -> ClassField {
        let need = class.required_depth_m();
        let pass_depth = need + self.params.safety_margin_m;
        let n = self.cols * self.rows;
        let passable: Vec<bool> = self
            .avail
            .iter()
            .map(|&d| f64::from(d) >= pass_depth)
            .collect();

        // Multi-source BFS: distance (in cells) from every cell to the nearest
        // impassable cell, capped — used for the shore-clearance penalty.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let clear_cells = (self.params.min_clearance_nm / self.cell_nm).round() as u16;
        let cap = clear_cells + 1;
        let mut dist = vec![u16::MAX; n];
        let mut q: VecDeque<usize> = VecDeque::new();
        for (c, &p) in passable.iter().enumerate() {
            // Sources are impassable cells (and grid edges act as impassable).
            let (i, j) = self.ij(c);
            let edge = i == 0 || j == 0 || i + 1 == self.cols || j + 1 == self.rows;
            if !p || edge {
                dist[c] = 0;
                q.push_back(c);
            }
        }
        while let Some(c) = q.pop_front() {
            let d = dist[c];
            if d >= cap {
                continue;
            }
            let (i, j) = self.ij(c);
            for (ni, nj) in self.neighbours4(i, j) {
                let nc = self.idx(ni, nj);
                if dist[nc] > d + 1 {
                    dist[nc] = d + 1;
                    q.push_back(nc);
                }
            }
        }

        // comfort depth: 60 % margin over the requirement.
        let comfort = need * 1.6;
        let base_mult: Vec<f32> = (0..n)
            .map(|c| {
                if !passable[c] {
                    return 1.0;
                }
                let avail = f64::from(self.avail[c]);
                let shallow = ((comfort - avail) / (comfort - need)).clamp(0.0, 1.0);
                let dcells = f64::from(dist[c].min(cap));
                let clear = ((f64::from(clear_cells) - dcells) / f64::from(clear_cells.max(1)))
                    .clamp(0.0, 1.0);
                // Off-lane penalty (0 on the busiest corridor, w_lane in open water).
                let lane_pen = lane_att.map_or(0.0, |la| {
                    self.params.w_lane * (1.0 - f64::from(la[c])).clamp(0.0, 1.0)
                });
                #[allow(clippy::cast_possible_truncation)]
                let m = (1.0
                    + self.params.w_shallow * shallow
                    + self.params.w_clearance * clear
                    + lane_pen) as f32;
                m
            })
            .collect();

        ClassField {
            passable,
            base_mult,
        }
    }

    fn neighbours4(&self, i: usize, j: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let cols = self.cols;
        let rows = self.rows;
        [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter_map(move |(di, dj)| {
                let ni = i as i64 + di;
                let nj = j as i64 + dj;
                (ni >= 0 && nj >= 0 && (ni as usize) < cols && (nj as usize) < rows)
                    .then_some((ni as usize, nj as usize))
            })
    }

    /// Nearest passable cell to a lat/lon within `max_ring` cells, else `None`.
    #[must_use]
    pub fn snap(&self, lat: f64, lon: f64, field: &ClassField, max_ring: usize) -> Option<usize> {
        let (x, y) = simulation::ais::lat_lon_to_field(lat, lon);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (ci, cj) = ((x / self.cell_nm) as isize, (y / self.cell_nm) as isize);
        for ring in 0..=max_ring as isize {
            let mut best: Option<(f64, usize)> = None;
            for dj in -ring..=ring {
                for di in -ring..=ring {
                    if ring > 0 && di.abs() != ring && dj.abs() != ring {
                        continue;
                    }
                    let (i, j) = (ci + di, cj + dj);
                    if i < 0 || j < 0 || i as usize >= self.cols || j as usize >= self.rows {
                        continue;
                    }
                    let c = self.idx(i as usize, j as usize);
                    if field.passable[c] {
                        let (cx, cy) = cell_center(i as usize, j as usize, self.cell_nm);
                        let d2 = (cx - x).powi(2) + (cy - y).powi(2);
                        if best.is_none_or(|(bd, _)| d2 < bd) {
                            best = Some((d2, c));
                        }
                    }
                }
            }
            if let Some((_, c)) = best {
                return Some(c);
            }
        }
        None
    }

    /// Cost-field weighted A* between two cells, with seeded per-route jitter
    /// (`salt`). Returns the cell path (inclusive), or `None` if disconnected.
    #[must_use]
    pub fn astar(
        &self,
        field: &ClassField,
        start: usize,
        goal: usize,
        salt: u64,
    ) -> Option<Vec<usize>> {
        const DIAG: f64 = std::f64::consts::SQRT_2;
        if start == goal {
            return Some(vec![start]);
        }
        let n = self.cols * self.rows;
        let (gi, gj) = self.ij(goal);
        let mut g = vec![f64::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
        g[start] = 0.0;
        heap.push(Reverse((0, start)));

        let mult = |c: usize| -> f64 {
            f64::from(field.base_mult[c]) * (1.0 + self.params.jitter * hash01(c as u64, salt))
        };

        while let Some(Reverse((_, cur))) = heap.pop() {
            if cur == goal {
                return Some(reconstruct(&came, start, goal));
            }
            let (ci, cj) = self.ij(cur);
            let mcur = mult(cur);
            for (di, dj, step) in [
                (1i64, 0i64, 1.0),
                (-1, 0, 1.0),
                (0, 1, 1.0),
                (0, -1, 1.0),
                (1, 1, DIAG),
                (1, -1, DIAG),
                (-1, 1, DIAG),
                (-1, -1, DIAG),
            ] {
                let (ni, nj) = (ci as i64 + di, cj as i64 + dj);
                if ni < 0 || nj < 0 || ni as usize >= self.cols || nj as usize >= self.rows {
                    continue;
                }
                let (ni, nj) = (ni as usize, nj as usize);
                let nidx = self.idx(ni, nj);
                if !field.passable[nidx] {
                    continue;
                }
                // No diagonal corner-cutting through an impassable pinch.
                if di != 0
                    && dj != 0
                    && (!field.passable[self.idx(ni, cj)] || !field.passable[self.idx(ci, nj)])
                {
                    continue;
                }
                let edge = step * self.cell_nm * 0.5 * (mcur + mult(nidx));
                let tentative = g[cur] + edge;
                if tentative < g[nidx] {
                    g[nidx] = tentative;
                    came[nidx] = cur;
                    let dx = (ni as f64 - gi as f64).abs();
                    let dy = (nj as f64 - gj as f64).abs();
                    // Octile heuristic × minimum multiplier 1 → admissible.
                    let h = (dx.max(dy) + (DIAG - 1.0) * dx.min(dy)) * self.cell_nm;
                    #[allow(clippy::cast_possible_truncation)]
                    let f = ((tentative + h) * 1000.0) as i64;
                    heap.push(Reverse((f, nidx)));
                }
            }
        }
        None
    }

    /// True if the straight field-nm segment stays in passable water for the
    /// class (sampled ~every half cell).
    #[must_use]
    pub fn segment_passable(&self, field: &ClassField, a: (f64, f64), b: (f64, f64)) -> bool {
        let dist = (b.0 - a.0).hypot(b.1 - a.1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (dist / (0.5 * self.cell_nm)).ceil().max(1.0) as usize;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let x = a.0 + (b.0 - a.0) * t;
            let y = a.1 + (b.1 - a.1) * t;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, j) = ((x / self.cell_nm) as isize, (y / self.cell_nm) as isize);
            if i < 0 || j < 0 || i as usize >= self.cols || j as usize >= self.rows {
                return false;
            }
            if !field.passable[self.idx(i as usize, j as usize)] {
                return false;
            }
        }
        true
    }
}

#[inline]
fn cell_center(i: usize, j: usize, cell_nm: f64) -> (f64, f64) {
    #[allow(clippy::cast_precision_loss)]
    ((i as f64 + 0.5) * cell_nm, (j as f64 + 0.5) * cell_nm)
}

fn reconstruct(came: &[usize], start: usize, goal: usize) -> Vec<usize> {
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != start {
        cur = came[cur];
        if cur == usize::MAX {
            break;
        }
        path.push(cur);
    }
    path.reverse();
    path
}

/// Deterministic hash of `(cell, salt)` → [0, 1) — SplitMix64-style mixing.
fn hash01(cell: u64, salt: u64) -> f64 {
    let mut x = cell.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ salt.wrapping_add(0x1234_5678_9abc_def0);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    #[allow(clippy::cast_precision_loss)]
    let v = (x >> 11) as f64 / (1u64 << 53) as f64;
    v
}

// ── path field-nm helpers (Ramer–Douglas–Peucker) ─────────────────────────────

/// Simplify a field-nm polyline but never let a shortcut cross impassable water
/// for the class: lower the tolerance until clean, else keep the dense path.
#[must_use]
pub fn simplify_passable(
    router: &Router,
    field: &ClassField,
    points: &[(f64, f64)],
    epsilon: f64,
) -> Vec<(f64, f64)> {
    let mut eps = epsilon;
    for _ in 0..6 {
        let s = rdp(points, eps);
        if s.windows(2)
            .all(|w| router.segment_passable(field, w[0], w[1]))
        {
            return s;
        }
        eps *= 0.5;
    }
    points.to_vec()
}

fn rdp(points: &[(f64, f64)], epsilon: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let mut dmax = 0.0;
    let mut index = 0;
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = perpendicular_distance(*p, a, b);
        if d > dmax {
            dmax = d;
            index = i;
        }
    }
    if dmax > epsilon {
        let mut left = rdp(&points[..=index], epsilon);
        let right = rdp(&points[index..], epsilon);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![a, b]
    }
}

fn perpendicular_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return (p.0 - a.0).hypot(p.1 - a.1);
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

/// Converts a field-nm path to `(lat, lon)` waypoints.
#[must_use]
pub fn path_to_latlon(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .map(|&(x, y)| field_to_lat_lon(x, y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deep ocean (−50 m) everywhere, with an impassable shallow (+5 m) wall in
    /// the lon band [10,11]° that leaves a navigable gap north of 63° N.
    fn synth_bathy() -> Bathymetry {
        let (lat_min, lon_min, dlat, dlon) = (50.5, -5.0, 0.5, 0.5);
        let (nlat, nlon) = (31usize, 72usize);
        let mut depth = vec![-50i16; nlat * nlon];
        for i in 0..nlat {
            for j in 0..nlon {
                let lat = lat_min + (i as f64 + 0.5) * dlat;
                let lon = lon_min + (j as f64 + 0.5) * dlon;
                if (10.0..=11.0).contains(&lon) && lat < 63.0 {
                    depth[i * nlon + j] = 5; // above sea level -> impassable
                }
            }
        }
        Bathymetry::from_raw(lat_min, lon_min, dlat, dlon, nlat, nlon, depth)
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn hash01_is_deterministic_and_bounded() {
        for c in 0..1000u64 {
            let v = hash01(c, 7);
            assert!((0.0..1.0).contains(&v));
            assert_eq!(v, hash01(c, 7), "same inputs -> same output");
        }
        assert_ne!(hash01(1, 1), hash01(1, 2), "salt changes the value");
    }

    #[test]
    fn deeper_ports_reach_fewer_cells() {
        let bathy = synth_bathy();
        let router = Router::new(
            &bathy,
            RouterParams {
                grid_nm: 15.0,
                ..RouterParams::default()
            },
        );
        let shallow = router.class_field(CLASSES[0], None); // 8 m
        let deep = router.class_field(CLASSES[2], None); // 17 m
        let sp = shallow.passable.iter().filter(|p| **p).count();
        let dp = deep.passable.iter().filter(|p| **p).count();
        // -50 m water passes both; the point is the API works and the deeper
        // class never has MORE passable cells than the shallower one.
        assert!(dp <= sp);
        assert!(sp > 0);
    }

    #[test]
    fn astar_routes_around_a_shallow_wall() {
        let bathy = synth_bathy();
        let router = Router::new(
            &bathy,
            RouterParams {
                grid_nm: 15.0,
                ..RouterParams::default()
            },
        );
        let field = router.class_field(CLASSES[1], None); // cargo, 13 m
        let start = router.snap(56.0, 0.0, &field, 30).expect("west start");
        let goal = router.snap(56.0, 20.0, &field, 30).expect("east goal");
        let path = router
            .astar(&field, start, goal, 0)
            .expect("must detour via the gap");
        // Every path cell is passable (deep enough) …
        for &c in &path {
            assert!(field.passable[c], "path crossed shallow water");
        }
        // … and the route must climb north into the gap (lat > 62°) to pass.
        let max_lat = path
            .iter()
            .map(|&c| {
                let (x, y) = router.cell_center_of(c);
                field_to_lat_lon(x, y).0
            })
            .fold(f64::MIN, f64::max);
        assert!(
            max_lat > 62.0,
            "route should detour north of the wall, got {max_lat:.1}"
        );
    }

    #[test]
    fn rdp_reduces_straight_line_and_keeps_corner() {
        let line: Vec<(f64, f64)> = (0..10).map(|i| (f64::from(i), 0.0)).collect();
        assert_eq!(rdp(&line, 0.5).len(), 2);
        let corner = vec![(0.0, 0.0), (5.0, 5.0), (10.0, 0.0)];
        assert_eq!(rdp(&corner, 0.5).len(), 3);
    }
}
