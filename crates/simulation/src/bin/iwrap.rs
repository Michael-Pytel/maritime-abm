//! Offline IWRAP Mk II collision-frequency report.
//!
//! Loads the hand-authored route network (`data/ais_paths.json`), builds a
//! multi-class waterway network (each route carries its vessel type's beam,
//! length, and speed), and prints the expected collisions per year
//! `N_c = Σ N_a · P_c` for head-on, overtaking, and crossing encounters.
//!
//! Usage: `iwrap [FLEET] [SPEED_SCALE]`
//!   `FLEET`        total vessels on the network (default 25)
//!   `SPEED_SCALE`  multiplies nominal speeds, e.g. 0.78 to model the Proposed
//!                  system's weather-induced slow-down (default 1.0)

use anyhow::Result;
use simulation::ais;
use simulation::iwrap::{self, ShipFlow, WaterwayLeg};
use simulation::vessel::{ship_dimensions, typical_speed_kn};

const METERS_PER_NM: f64 = 1852.0;
const FAIRWAY_WIDTH_NM: f64 = 2.0;
const PERIOD_H: f64 = 720.0; // 30-day run horizon; analyzer scales to one year

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let fleet: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(25);
    let speed_scale: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let path = std::env::var("AIS_PATH").unwrap_or_else(|_| "data/ais_paths.json".to_string());
    let records = ais::load_ais(&path)?;
    let route_count = records
        .iter()
        .filter(|r| r.waypoints.len() >= 2)
        .count()
        .max(1);
    #[allow(clippy::cast_precision_loss)]
    let vessels_per_route = f64::from(fleet) / route_count as f64;

    let mut legs: Vec<WaterwayLeg> = Vec::new();
    for rec in &records {
        if rec.waypoints.len() < 2 {
            continue;
        }
        let dims = ship_dimensions(&rec.vessel_type);
        let beam_nm = dims.beam_m / METERS_PER_NM;
        let length_nm = dims.length_m / METERS_PER_NM;
        let speed_kn = typical_speed_kn(&rec.vessel_type, 0.5) * speed_scale;

        let pts: Vec<(f64, f64)> = rec
            .waypoints
            .iter()
            .map(|w| ais::lat_lon_to_field(w.lat, w.lon))
            .collect();
        let total_len: f64 = pts
            .windows(2)
            .map(|w| {
                let dx = w[1].0 - w[0].0;
                let dy = w[1].1 - w[0].1;
                (dx * dx + dy * dy).sqrt()
            })
            .sum();
        if total_len <= 0.0 || speed_kn <= 0.0 {
            continue;
        }
        let flow_per_h = vessels_per_route * speed_kn / (2.0 * total_len);
        let flow = ShipFlow {
            flow_per_h,
            beam_nm,
            length_nm,
            speed_kn,
        };
        for w in pts.windows(2) {
            legs.push(WaterwayLeg {
                a: w[0],
                b: w[1],
                width_nm: FAIRWAY_WIDTH_NM,
                flows: vec![flow],
            });
        }
    }

    let report = iwrap::analyze(&legs, PERIOD_H);
    println!("IWRAP Mk II collision-frequency report");
    println!("  routes={route_count}  fleet={fleet}  speed_scale={speed_scale}");
    println!("  legs={}", legs.len());
    println!(
        "  N_a (candidates/yr):  head-on={:.4}  overtaking={:.4}  crossing={:.4}",
        report.n_a_head_on, report.n_a_overtaking, report.n_a_crossing
    );
    println!("  N_c (collisions/yr):  {:.5}", report.n_c_per_year);
    let verdict = if report.n_c_per_year < 1.0 {
        "PASS (<1/yr)"
    } else {
        "ABOVE 1/yr"
    };
    println!("  acceptance (<1 collision/yr/fairway): {verdict}");
    Ok(())
}
