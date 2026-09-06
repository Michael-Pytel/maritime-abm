//! Offline IWRAP Mk II collision-frequency report.
//!
//! Loads the route network (`data/ais_paths.json`), builds a multi-class
//! waterway network, and prints the expected collisions per year
//! `N_c = Σ N_a · P_c` for head-on, overtaking, and crossing encounters.
//!
//! Usage: `iwrap [FLEET] [SPEED_SCALE]`
//!   `FLEET`        total vessels on the network (default 25)
//!   `SPEED_SCALE`  multiplies nominal speeds, e.g. 0.78 to model the Proposed
//!                  system's weather-induced slow-down (default 1.0)

use anyhow::Result;
use simulation::iwrap;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let fleet: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(25);
    let speed_scale: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let path = std::env::var("AIS_PATH").unwrap_or_else(|_| "data/ais_paths.json".to_string());
    let n_c = iwrap::estimate_nc_from_ais_path(&path, fleet, speed_scale)?;

    println!("IWRAP Mk II collision-frequency report");
    println!("  ais_path={path}  fleet={fleet}  speed_scale={speed_scale}");
    println!("  N_c (collisions/yr):  {n_c:.5}");
    let verdict = if n_c < 1.0 {
        "PASS (<1/yr)"
    } else {
        "ABOVE 1/yr"
    };
    println!("  acceptance (<1 collision/yr/fairway): {verdict}");
    Ok(())
}
