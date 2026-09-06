//! Tier-2 stochastic weather field (Section 3.3 of the paper).
//!
//! A 50×50 grid of scalar hazard values W ∈ [0, 1] evolves each tick through
//! seven ordered stages that mirror the paper's specification:
//!
//!   1. Regime switch (calm ↔ storm, Bernoulli draw)
//!   2. Storm-cell lifecycle  (spawn / drift / decay)
//!   3. AR(1) background evolution with advection  (Eq. 18)
//!   4. System overlay  (storm cells raise sea/wind, cut visibility)
//!   5. Cross-channel coupling  (Eqs. 19–21)
//!   6. 5-point neighbourhood smoothing + gradient limiting
//!   7. Hazard combination  W = weighted sum of channels
//!
//! The three physical channels — sea state `s`, visibility `v`, wind `u` —
//! are evolved independently and then fused into W.

use rand::Rng;
use serde::Serialize;

// ── Grid dimensions ────────────────────────────────────────────────────────

pub const GRID_CELLS: usize = 50;
const N: usize = GRID_CELLS * GRID_CELLS;

// ── AR(1) channel parameters ───────────────────────────────────────────────

// Per-tick AR(1) coefficients at Δt = 5 min, matched to the former 15-min
// persistence via ρ_5 = ρ_15^(1/3) so physical correlation time is unchanged.
const RHO_S: f64 = 0.983_047; // ≈ 0.95^(1/3)
const RHO_V: f64 = 0.965_489; // ≈ 0.90^(1/3)
const RHO_U: f64 = 0.976_054; // ≈ 0.93^(1/3)

// Calm-regime mean-reversion targets
const MU_S_CALM: f64 = 0.30; // moderate sea
const MU_V_CALM: f64 = 0.80; // mostly clear
const MU_U_CALM: f64 = 0.25; // light wind

// Storm-regime mean-reversion targets
const MU_S_STORM: f64 = 0.72; // heavy sea
const MU_V_STORM: f64 = 0.30; // poor visibility
const MU_U_STORM: f64 = 0.75; // strong wind

const SIGMA_S: f64 = 0.07;
const SIGMA_V: f64 = 0.06;
const SIGMA_U: f64 = 0.07;

// ── Regime transition probabilities ───────────────────────────────────────

// Default (mixed) preset — per 5-min tick, ≈⅓ of the former 15-min rates so
// expected regime sojourn times (hours) stay comparable.
const P_CS_DEFAULT: f64 = 0.01; // calm → storm per tick
const P_SC_DEFAULT: f64 = 0.027; // storm → calm per tick

// ── Storm cell parameters ──────────────────────────────────────────────────

const CELL_DRIFT_SPEED: f64 = 0.6; // grid-cells/tick
const DECAY_IN_STORM: f64 = 0.006;
const DECAY_IN_CALM: f64 = 0.020;
const MIN_INTENSITY: f64 = 0.12;
const P_SPAWN_BASE: f64 = 0.08;
const R_STORM: f64 = 1.35; // regime spawn multiplier
const R_CALM: f64 = 0.65;
const MAX_CELLS: usize = 8;
const CELL_RADIUS_BASE: f64 = 8.0; // grid-cells

// ── Hazard combination weights ─────────────────────────────────────────────

const W_SEA: f64 = 1.0;
const W_VIS: f64 = 0.5; // (1 − vis) contribution
const W_WIND: f64 = 0.8;
const W_TOTAL: f64 = W_SEA + W_VIS + W_WIND;

// ── Gradient limit ─────────────────────────────────────────────────────────

const MAX_GRADIENT: f64 = 0.12;

// ── Scenario preset ────────────────────────────────────────────────────────

/// Controls how the weather field is initialised and biased across the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherPreset {
    /// Predominantly calm — regime rarely flips to storm.
    Calm,
    /// Balanced calm/storm cycling.
    Mixed,
    /// Starts in storm regime; transitions back to calm slowly.
    Stormy,
}

// ── Internal types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Regime {
    Calm,
    Storm,
}

#[derive(Debug, Clone)]
struct StormCell {
    cx: f64,        // centre column (grid-cell coords)
    cy: f64,        // centre row
    radius: f64,    // grid-cells
    intensity: f64, // [0, 1]
    dx: f64,        // drift per tick
    dy: f64,
    radius_phase: f64, // phase for sinusoidal radius oscillation
}

// ── Public struct ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WeatherField {
    pub preset: WeatherPreset,
    pub regime: Regime,
    /// Sea-state channel, row-major len = N.
    sea: Vec<f64>,
    /// Visibility channel (1 = perfect), len = N.
    vis: Vec<f64>,
    /// Wind channel, len = N.
    wind: Vec<f64>,
    /// Combined hazard W ∈ [0, 1], len = N.
    pub hazard: Vec<f64>,
    storm_cells: Vec<StormCell>,
    adv_drift_x: f64, // fractional advection accumulator
    adv_drift_y: f64,
    adv_x: i32, // integer cell offset (wraps)
    adv_y: i32,
    p_cs: f64, // calm → storm probability
    p_sc: f64, // storm → calm probability
    mu_s: f64, // current mean-reversion targets
    mu_v: f64,
    mu_u: f64,
}

impl WeatherField {
    // ── Constructor ────────────────────────────────────────────────────────

    pub fn new(preset: WeatherPreset, rng: &mut impl Rng) -> Self {
        let regime = match preset {
            WeatherPreset::Stormy => Regime::Storm,
            _ => Regime::Calm,
        };

        let (p_cs, p_sc) = match preset {
            WeatherPreset::Calm => (P_CS_DEFAULT * 0.35, P_SC_DEFAULT * 1.40),
            WeatherPreset::Mixed => (P_CS_DEFAULT, P_SC_DEFAULT),
            WeatherPreset::Stormy => (P_CS_DEFAULT * 2.00, P_SC_DEFAULT * 0.50),
        };

        let (mu_s, mu_v, mu_u) = targets_for(regime);

        // Initialise channels near their regime targets with small noise.
        let sea = init_channel(mu_s, rng);
        let vis = init_channel(mu_v, rng);
        let wind = init_channel(mu_u, rng);

        let mut f = Self {
            preset,
            regime,
            sea,
            vis,
            wind,
            hazard: vec![0.0; N],
            storm_cells: Vec::new(),
            adv_drift_x: 0.0,
            adv_drift_y: 0.0,
            adv_x: 0,
            adv_y: 0,
            p_cs,
            p_sc,
            mu_s,
            mu_v,
            mu_u,
        };
        f.combine_hazard();
        f
    }

    // ── Public interface ───────────────────────────────────────────────────

    /// Returns W ∈ [0, 1] at a field-coordinate position `(fx, fy)`.
    ///
    /// `world_w` / `world_h` are the world extents in nautical miles.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn hazard_at(&self, fx: f64, fy: f64, world_w: f64, world_h: f64) -> f64 {
        let g = GRID_CELLS as f64;
        let col = ((fx / world_w) * g).clamp(0.0, g - 1.0) as usize;
        let row = ((fy / world_h) * g).clamp(0.0, g - 1.0) as usize;
        self.hazard[row * GRID_CELLS + col]
    }

    /// Advance the field by one simulation tick (seven stages).
    pub fn update(&mut self, rng: &mut impl Rng) {
        self.stage1_regime(rng);
        self.stage2_cells(rng);
        self.stage3_ar1(rng);
        self.stage4_overlay();
        self.stage5_coupling();
        self.stage6_smooth_limit();
        self.combine_hazard();
    }

    // ── Stage 1 — Regime switch ────────────────────────────────────────────

    fn stage1_regime(&mut self, rng: &mut impl Rng) {
        let flip = match self.regime {
            Regime::Calm => rng.gen::<f64>() < self.p_cs,
            Regime::Storm => rng.gen::<f64>() < self.p_sc,
        };
        if flip {
            self.regime = match self.regime {
                Regime::Calm => Regime::Storm,
                Regime::Storm => Regime::Calm,
            };
            let (mu_s, mu_v, mu_u) = targets_for(self.regime);
            self.mu_s = mu_s;
            self.mu_v = mu_v;
            self.mu_u = mu_u;
        }
    }

    // ── Stage 2 — Storm-cell lifecycle ─────────────────────────────────────

    fn stage2_cells(&mut self, rng: &mut impl Rng) {
        let decay = match self.regime {
            Regime::Storm => DECAY_IN_STORM,
            Regime::Calm => DECAY_IN_CALM,
        };
        let r_mult = match self.regime {
            Regime::Storm => R_STORM,
            Regime::Calm => R_CALM,
        };

        // Drift, oscillate radius, and decay.
        for cell in &mut self.storm_cells {
            cell.cx += cell.dx;
            cell.cy += cell.dy;
            cell.intensity -= decay;
            cell.radius_phase += 0.08;
            cell.radius =
                CELL_RADIUS_BASE * (1.0 + 0.15 * cell.radius_phase.sin()) * cell.intensity.sqrt();
            // cells shrink as they fade
        }

        // Remove exhausted or off-grid cells.
        #[allow(clippy::cast_precision_loss)]
        let g = GRID_CELLS as f64;
        self.storm_cells.retain(|c| {
            c.intensity >= MIN_INTENSITY
                && c.cx > -c.radius - 2.0
                && c.cx < g + c.radius + 2.0
                && c.cy > -c.radius - 2.0
                && c.cy < g + c.radius + 2.0
        });

        // Spawn a new cell.
        if self.storm_cells.len() < MAX_CELLS && rng.gen::<f64>() < P_SPAWN_BASE * r_mult {
            let bearing = rng.gen::<f64>() * std::f64::consts::TAU;
            let speed = CELL_DRIFT_SPEED * (0.5 + rng.gen::<f64>() * 0.5);
            let jitter = 0.3;
            self.storm_cells.push(StormCell {
                cx: rng.gen::<f64>() * g,
                cy: rng.gen::<f64>() * g,
                radius: CELL_RADIUS_BASE * (0.7 + rng.gen::<f64>() * 0.6),
                intensity: MIN_INTENSITY + rng.gen::<f64>() * (0.8 - MIN_INTENSITY),
                dx: bearing.sin() * speed + rng.gen_range(-jitter..jitter),
                dy: bearing.cos() * speed + rng.gen_range(-jitter..jitter),
                radius_phase: rng.gen::<f64>() * std::f64::consts::TAU,
            });
        }
    }

    // ── Stage 3 — AR(1) background with advection ─────────────────────────
    //
    // X_{t+1} = ρ·adv(X_t) + (1−ρ)·μ + σ·(0.3+u)·η   (Eq. 18)

    fn stage3_ar1(&mut self, rng: &mut impl Rng) {
        // Slowly drift advection offset (weather patterns advect eastward/NE).
        self.adv_drift_x += 0.018;
        self.adv_drift_y += 0.008;
        while self.adv_drift_x >= 1.0 {
            self.adv_drift_x -= 1.0;
            self.adv_x += 1;
        }
        while self.adv_drift_x <= -1.0 {
            self.adv_drift_x += 1.0;
            self.adv_x -= 1;
        }
        while self.adv_drift_y >= 1.0 {
            self.adv_drift_y -= 1.0;
            self.adv_y += 1;
        }
        while self.adv_drift_y <= -1.0 {
            self.adv_drift_y += 1.0;
            self.adv_y -= 1;
        }

        let u = 0.30_f64; // scenario unpredictability (default)

        self.sea = ar1_step(
            advect(&self.sea, self.adv_x, self.adv_y),
            RHO_S,
            self.mu_s,
            SIGMA_S,
            u,
            rng,
        );
        self.vis = ar1_step(
            advect(&self.vis, self.adv_x, self.adv_y),
            RHO_V,
            self.mu_v,
            SIGMA_V,
            u,
            rng,
        );
        self.wind = ar1_step(
            advect(&self.wind, self.adv_x, self.adv_y),
            RHO_U,
            self.mu_u,
            SIGMA_U,
            u,
            rng,
        );
    }

    // ── Stage 4 — System overlay ───────────────────────────────────────────
    // Each active storm cell adds intensity-weighted Gaussian blobs to sea
    // and wind, and subtracts from visibility.

    #[allow(clippy::cast_precision_loss)]
    fn stage4_overlay(&mut self) {
        for row in 0..GRID_CELLS {
            for col in 0..GRID_CELLS {
                let idx = row * GRID_CELLS + col;
                let cx = col as f64 + 0.5;
                let cy = row as f64 + 0.5;

                let mut add_s = 0.0_f64;
                let mut sub_v = 0.0_f64;
                let mut add_u = 0.0_f64;

                for cell in &self.storm_cells {
                    let dx = cx - cell.cx;
                    let dy = cy - cell.cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 < cell.radius * cell.radius {
                        let w = (1.0 - (d2 / (cell.radius * cell.radius)).sqrt()).powi(2)
                            * cell.intensity;
                        add_s += w;
                        sub_v += w * 0.85;
                        add_u += w;
                    }
                }

                self.sea[idx] = (self.sea[idx] + add_s).min(1.0);
                self.vis[idx] = (self.vis[idx] - sub_v).max(0.0);
                self.wind[idx] = (self.wind[idx] + add_u).min(1.0);
            }
        }
    }

    // ── Stage 5 — Cross-channel coupling ──────────────────────────────────
    // Eqs. 19–21:  s += 0.35·(u−0.5),  s −= 0.20·(v−0.5),  u −= 0.25·(v−0.5)

    fn stage5_coupling(&mut self) {
        for i in 0..N {
            let s = self.sea[i];
            let v = self.vis[i];
            let u = self.wind[i];
            self.sea[i] = (s + 0.35 * (u - 0.5) - 0.20 * (v - 0.5)).clamp(0.0, 1.0);
            self.wind[i] = (u - 0.25 * (v - 0.5)).clamp(0.0, 1.0);
        }
    }

    // ── Stage 6 — Smoothing + gradient limiting ────────────────────────────

    fn stage6_smooth_limit(&mut self) {
        smooth(&mut self.sea);
        smooth(&mut self.vis);
        smooth(&mut self.wind);
        limit_gradient(&mut self.sea);
        limit_gradient(&mut self.vis);
        limit_gradient(&mut self.wind);
    }

    // ── Stage 7 — Hazard combination ──────────────────────────────────────
    // W = (w_s·s + w_v·(1−v) + w_w·u) / (w_s + w_v + w_w)

    fn combine_hazard(&mut self) {
        for i in 0..N {
            self.hazard[i] =
                ((W_SEA * self.sea[i] + W_VIS * (1.0 - self.vis[i]) + W_WIND * self.wind[i])
                    / W_TOTAL)
                    .clamp(0.0, 1.0);
        }
    }
}

// ── Pure helper functions ──────────────────────────────────────────────────

fn targets_for(regime: Regime) -> (f64, f64, f64) {
    match regime {
        Regime::Storm => (MU_S_STORM, MU_V_STORM, MU_U_STORM),
        Regime::Calm => (MU_S_CALM, MU_V_CALM, MU_U_CALM),
    }
}

fn init_channel(mu: f64, rng: &mut impl Rng) -> Vec<f64> {
    (0..N)
        .map(|_| (mu + rng.gen::<f64>() * 0.12 - 0.06).clamp(0.0, 1.0))
        .collect()
}

/// Circular shift of a 50×50 grid by `(ox, oy)` integer cell offsets.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn advect(grid: &[f64], ox: i32, oy: i32) -> Vec<f64> {
    let g = GRID_CELLS as i32;
    let mut out = vec![0.0; N];
    for row in 0..g {
        for col in 0..g {
            let src_col = ((col - ox).rem_euclid(g)) as usize;
            let src_row = ((row - oy).rem_euclid(g)) as usize;
            out[(row as usize) * GRID_CELLS + (col as usize)] =
                grid[src_row * GRID_CELLS + src_col];
        }
    }
    out
}

/// AR(1) update with additive Gaussian-like noise.
fn ar1_step(grid: Vec<f64>, rho: f64, mu: f64, sigma: f64, u: f64, rng: &mut impl Rng) -> Vec<f64> {
    grid.into_iter()
        .map(|x| {
            let eta: f64 = rng.gen::<f64>() * 2.0 - 1.0;
            (rho * x + (1.0 - rho) * mu + sigma * (0.3 + u) * eta).clamp(0.0, 1.0)
        })
        .collect()
}

/// 5-point neighbourhood smoother (centre weight = 2).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn smooth(grid: &mut [f64]) {
    let src = grid.to_vec();
    let g = GRID_CELLS as i32;
    for row in 0..g {
        for col in 0..g {
            let mut sum = src[(row * g + col) as usize] * 2.0;
            let mut cnt = 2.0_f64;
            for (dr, dc) in [(-1_i32, 0_i32), (1, 0), (0, -1), (0, 1)] {
                let r2 = row + dr;
                let c2 = col + dc;
                if r2 >= 0 && r2 < g && c2 >= 0 && c2 < g {
                    sum += src[(r2 * g + c2) as usize];
                    cnt += 1.0;
                }
            }
            grid[(row * g + col) as usize] = sum / cnt;
        }
    }
}

/// Single-pass gradient limiter: caps cell-to-cell change to `MAX_GRADIENT`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn limit_gradient(grid: &mut [f64]) {
    let src = grid.to_vec();
    let g = GRID_CELLS as i32;
    for row in 0..g {
        for col in 0..g {
            let idx = (row * g + col) as usize;
            let mut v = src[idx];
            for (dr, dc) in [(-1_i32, 0_i32), (1, 0), (0, -1), (0, 1)] {
                let r2 = row + dr;
                let c2 = col + dc;
                if r2 >= 0 && r2 < g && c2 >= 0 && c2 < g {
                    let n = src[(r2 * g + c2) as usize];
                    if (v - n).abs() > MAX_GRADIENT {
                        v = n + MAX_GRADIENT * (v - n).signum();
                    }
                }
            }
            grid[idx] = v;
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    #[test]
    fn hazard_stays_in_unit_interval() {
        let mut rng = SmallRng::seed_from_u64(42);
        let mut field = WeatherField::new(WeatherPreset::Stormy, &mut rng);
        for _ in 0..100 {
            field.update(&mut rng);
        }
        for &w in &field.hazard {
            assert!((0.0..=1.0).contains(&w), "hazard out of range: {w}");
        }
    }

    #[test]
    fn stormy_preset_higher_mean_than_calm() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut stormy = WeatherField::new(WeatherPreset::Stormy, &mut rng);
        let mut calm = WeatherField::new(WeatherPreset::Calm, &mut rng);
        for _ in 0..200 {
            stormy.update(&mut rng);
            calm.update(&mut rng);
        }
        #[allow(clippy::cast_precision_loss)]
        let mean_s: f64 = stormy.hazard.iter().sum::<f64>() / stormy.hazard.len() as f64;
        #[allow(clippy::cast_precision_loss)]
        let mean_c: f64 = calm.hazard.iter().sum::<f64>() / calm.hazard.len() as f64;
        assert!(
            mean_s > mean_c,
            "stormy mean {mean_s:.3} not > calm mean {mean_c:.3}"
        );
    }

    #[test]
    fn hazard_at_maps_field_coords() {
        let mut rng = SmallRng::seed_from_u64(1);
        let field = WeatherField::new(WeatherPreset::Mixed, &mut rng);
        // Should not panic and should return a value in [0,1].
        let w = field.hazard_at(500.0, 400.0, 1000.0, 900.0);
        assert!((0.0..=1.0).contains(&w));
    }
}
