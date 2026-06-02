use serde::Serialize;
use simulation::{
    kpi::KpiSnapshot,
    scenario::{Method, Scenario},
};

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub scenario: String,
    pub method: String,
    pub seed: u64,
    pub kpi: KpiSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchState {
    pub batch_id: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub running: bool,
    pub results: Vec<BatchResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HypothesisResult {
    pub kpi: String,
    pub scenario: String,
    pub method_a: String,
    pub method_b: String,
    pub mean_a: f64,
    pub mean_b: f64,
    pub u_stat: f64,
    pub z_score: f64,
    pub p_value: f64,
    pub p_corrected: f64,  // Holm-Bonferroni adjusted p-value
    pub cliffs_delta: f64, // signed Cliff's Δ = (U_a - U_b) / (n_a × n_b)
    pub effect_size: f64,  // |cliffs_delta| for backwards compat
    pub significant: bool, // p_corrected < 0.05
    pub confirmed: bool,   // significant AND |cliffs_delta| >= 0.2
}

/// Mann-Whitney U test (normal approximation, two-tailed).
/// Returns (`U_min`, Z, `p_value`, `cliffs_delta`).
#[allow(clippy::many_single_char_names, clippy::cast_precision_loss)]
fn mann_whitney(a: &[f64], b: &[f64]) -> (f64, f64, f64, f64) {
    let n1 = a.len();
    let n2 = b.len();
    if n1 == 0 || n2 == 0 {
        return (0.0, 0.0, 1.0, 0.0);
    }

    // Pool + rank with average-rank tie handling
    let mut combined: Vec<(f64, usize)> = a
        .iter()
        .map(|&v| (v, 0))
        .chain(b.iter().map(|&v| (v, 1)))
        .collect();
    combined.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

    let n = combined.len();
    let mut ranks = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && (combined[j].0 - combined[i].0).abs() < 1e-12 {
            j += 1;
        }
        let avg_rank = (i + j + 1) as f64 / 2.0; // 1-based average
        ranks[i..j].fill(avg_rank);
        i = j;
    }

    let r1: f64 = combined
        .iter()
        .zip(ranks.iter())
        .filter(|((_, g), _)| *g == 0)
        .map(|(_, r)| r)
        .sum();

    let u1 = r1 - (n1 * (n1 + 1)) as f64 / 2.0;
    let u2 = (n1 * n2) as f64 - u1;
    let u = u1.min(u2);

    let mean_u = (n1 * n2) as f64 / 2.0;
    let var_u = (n1 * n2 * (n1 + n2 + 1)) as f64 / 12.0;
    let z = (u - mean_u) / var_u.sqrt();
    let p = 2.0 * normal_cdf(-z.abs());

    // Signed Cliff's Δ: positive means a tends to be larger than b
    let cliffs_delta = (u1 - u2) / (n1 as f64 * n2 as f64);

    (u, z, p, cliffs_delta)
}

/// Approximation of Φ(x) — standard normal CDF (Abramowitz & Stegun 26.2.17).
fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.231_641_9 * x.abs());
    let poly = t
        * (0.319_381_530
            + t * (-0.356_563_782
                + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429))));
    let phi = 1.0 - (1.0 / (2.0 * std::f64::consts::PI).sqrt()) * (-x * x / 2.0).exp() * poly;
    if x >= 0.0 {
        phi
    } else {
        1.0 - phi
    }
}

fn extract_kpi(r: &BatchResult, kpi: &str) -> f64 {
    match kpi {
        "fatal_per_1k_hrs" => r.kpi.fatal_per_1k_hrs,
        "collision_per_1k_hrs" => r.kpi.collision_per_1k_hrs,
        "survival_ratio" => r.kpi.survival_ratio,
        "avg_tta_hours" => r.kpi.avg_tta_hours,
        "evac_activation_rate" => r.kpi.evac_activation_rate,
        "mean_p_prep" => r.kpi.mean_p_prep,
        _ => 0.0,
    }
}

/// Pairwise Mann-Whitney U tests for each KPI × scenario × method pair,
/// with Holm-Bonferroni correction applied across all tests.
#[allow(clippy::similar_names, clippy::many_single_char_names)]
pub fn compute_hypothesis_tests(results: &[BatchResult]) -> Vec<HypothesisResult> {
    let kpis = [
        "fatal_per_1k_hrs",
        "collision_per_1k_hrs",
        "survival_ratio",
        "avg_tta_hours",
        "evac_activation_rate",
        "mean_p_prep",
    ];
    let scenarios = [
        Scenario::CalmPassage,
        Scenario::StormCorridor,
        Scenario::BlindShore,
        Scenario::DeepWaterRescue,
    ];
    let method_pairs = [
        (Method::BaselineA, Method::ProposedSystem),
        (Method::BaselineB, Method::ProposedSystem),
        (Method::BaselineA, Method::BaselineB),
    ];

    let mut out: Vec<HypothesisResult> = Vec::new();
    for scenario in &scenarios {
        let sname = format!("{scenario:?}");
        for &(ma, mb) in &method_pairs {
            #[allow(clippy::similar_names)]
            let method_a_name = format!("{ma:?}");
            #[allow(clippy::similar_names)]
            let method_b_name = format!("{mb:?}");
            for kpi in &kpis {
                let a: Vec<f64> = results
                    .iter()
                    .filter(|r| r.scenario == sname && r.method == method_a_name)
                    .map(|r| extract_kpi(r, kpi))
                    .collect();
                let b: Vec<f64> = results
                    .iter()
                    .filter(|r| r.scenario == sname && r.method == method_b_name)
                    .map(|r| extract_kpi(r, kpi))
                    .collect();

                #[allow(clippy::cast_precision_loss)]
                let mean_a = if a.is_empty() {
                    0.0
                } else {
                    a.iter().sum::<f64>() / a.len() as f64
                };
                #[allow(clippy::cast_precision_loss)]
                let mean_b = if b.is_empty() {
                    0.0
                } else {
                    b.iter().sum::<f64>() / b.len() as f64
                };

                #[allow(clippy::many_single_char_names)]
                let (u, z, p, cliffs_delta) = mann_whitney(&a, &b);
                out.push(HypothesisResult {
                    kpi: kpi.to_string(),
                    scenario: sname.clone(),
                    method_a: method_a_name.clone(),
                    method_b: method_b_name.clone(),
                    mean_a,
                    mean_b,
                    u_stat: u,
                    z_score: z,
                    p_value: p,
                    p_corrected: p, // placeholder, filled below
                    cliffs_delta,
                    effect_size: cliffs_delta.abs(),
                    significant: false, // filled below
                    confirmed: false,   // filled below
                });
            }
        }
    }

    // Holm-Bonferroni step-down correction
    let m = out.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&i, &j| {
        out[i]
            .p_value
            .partial_cmp(&out[j].p_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut prev_corrected = 0.0f64;
    for (k, &idx) in order.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let factor = (m - k) as f64;
        let corrected = (out[idx].p_value * factor).min(1.0).max(prev_corrected);
        out[idx].p_corrected = corrected;
        out[idx].significant = corrected < 0.05;
        out[idx].confirmed = corrected < 0.05 && out[idx].cliffs_delta.abs() >= 0.2;
        prev_corrected = corrected;
    }

    out
}
