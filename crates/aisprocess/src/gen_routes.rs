//! Depth-aware procedural sea-route generator.
//!
//! Replaces the manual AIS pipeline (multi-GB CSV dumps + hand-authored
//! `ports.json`) with a deterministic, offline generator grounded in **real
//! sea depth**. It needs no downloaded data at run time (the bundled EMODnet
//! bathymetry is produced once by `tools/fetch_bathymetry.py`) and no network.
//!
//! For each vessel class it plans routes with a cost-field weighted A*
//! ([`aisprocess::router`]) that keeps ships in water deep enough for their
//! draft + under-keel clearance, an offing off the coast, and — with a seeded
//! per-route jitter — deterministic-but-varied tracks. Output is
//! `data/ais_paths.json` + `data/ports.json` in the schema the engine consumes.
//!
//! Usage:
//! ```text
//! cargo run -p aisprocess --bin gen_routes -- \
//!     [--seed 42] [--grid-nm 1.0] [--clearance-nm 2.5] \
//!     [--w-lane 2.5] [--w-shallow 3.0] [--jitter 0.18] \
//!     [--n-cargo 35] [--n-passenger 25] [--n-tanker 20] \
//!     [--out data/ais_paths.json] [--ports-out data/ports.json]
//! ```

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::doc_markdown
)]

use aisprocess::bathymetry::Bathymetry;
use aisprocess::lanes::LaneField;
use aisprocess::ports::PORTS;
use aisprocess::router::{
    path_to_latlon, simplify_passable, Router, RouterParams, VesselClass, CLASSES,
};
use anyhow::{bail, Context, Result};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::json;
use simulation::ais::lat_lon_to_field;

struct Config {
    seed: u64,
    grid_nm: f64,
    clearance_nm: f64,
    w_lane: f64,
    w_shallow: f64,
    jitter: f64,
    quotas: [(&'static str, usize); 3],
    bathy_bin: String,
    bathy_json: String,
    lanes_bin: String,
    lanes_json: String,
    use_lanes: bool,
    out: String,
    ports_out: String,
}

impl Default for Config {
    fn default() -> Self {
        let defaults = RouterParams::default();
        Self {
            seed: 42,
            grid_nm: defaults.grid_nm,
            clearance_nm: defaults.min_clearance_nm,
            w_lane: defaults.w_lane,
            w_shallow: defaults.w_shallow,
            jitter: defaults.jitter,
            quotas: [("cargo", 35), ("passenger", 25), ("tanker", 20)],
            bathy_bin: "data/bathymetry.bin.gz".into(),
            bathy_json: "data/bathymetry.json".into(),
            lanes_bin: "data/lanes.bin.gz".into(),
            lanes_json: "data/lanes.json".into(),
            use_lanes: true,
            out: "data/ais_paths.json".into(),
            ports_out: "data/ports.json".into(),
        }
    }
}

fn parse_args() -> Result<Config> {
    let mut cfg = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut next = || {
            args.next()
                .with_context(|| format!("missing value for {flag}"))
        };
        match flag.as_str() {
            "--seed" => cfg.seed = next()?.parse()?,
            "--grid-nm" => cfg.grid_nm = next()?.parse()?,
            "--clearance-nm" => cfg.clearance_nm = next()?.parse()?,
            "--w-lane" => cfg.w_lane = next()?.parse()?,
            "--w-shallow" => cfg.w_shallow = next()?.parse()?,
            "--jitter" => cfg.jitter = next()?.parse()?,
            "--n-cargo" => cfg.quotas[0].1 = next()?.parse()?,
            "--n-passenger" => cfg.quotas[1].1 = next()?.parse()?,
            "--n-tanker" => cfg.quotas[2].1 = next()?.parse()?,
            "--bathy-bin" => cfg.bathy_bin = next()?,
            "--bathy-json" => cfg.bathy_json = next()?,
            "--lanes-bin" => cfg.lanes_bin = next()?,
            "--lanes-json" => cfg.lanes_json = next()?,
            "--no-lanes" => cfg.use_lanes = false,
            "--out" => cfg.out = next()?,
            "--ports-out" => cfg.ports_out = next()?,
            "--source" => {
                let s = next()?;
                if s != "procedural" {
                    bail!("--source '{s}' not implemented; only 'procedural' is available");
                }
            }
            "-h" | "--help" => {
                eprintln!(
                    "gen_routes --seed --grid-nm --clearance-nm --w-lane --w-shallow --jitter --n-* --out --ports-out"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(cfg)
}

fn class_by_name(name: &str) -> VesselClass {
    *CLASSES
        .iter()
        .find(|c| c.name == name)
        .expect("quota names match CLASSES")
}

/// A port anchored to real navigable water: canonical (shallowest-class) sea
/// position, plus its snapped cell in whichever class field is being routed.
struct CanonPort {
    name: String,
    lat: f64,
    lon: f64,
}

fn main() -> Result<()> {
    let cfg = parse_args()?;

    eprintln!("Loading bathymetry {} …", cfg.bathy_bin);
    let bathy = Bathymetry::load(&cfg.bathy_bin, &cfg.bathy_json)
        .with_context(|| "failed to load bundled bathymetry (run tools/fetch_bathymetry.py?)")?;

    let params = RouterParams {
        grid_nm: cfg.grid_nm,
        min_clearance_nm: cfg.clearance_nm,
        w_lane: cfg.w_lane,
        w_shallow: cfg.w_shallow,
        jitter: cfg.jitter,
        ..RouterParams::default()
    };
    eprintln!(
        "Building {}-nm router grid (clearance {} nm, w_lane={}, w_shallow={}, jitter={}) …",
        cfg.grid_nm, cfg.clearance_nm, cfg.w_lane, cfg.w_shallow, cfg.jitter
    );
    let router = Router::new(&bathy, params);
    eprintln!("  {}×{} cells", router.cols, router.rows);

    // Optional real-traffic lane prior (EMODnet vessel density). If missing,
    // routing falls back to depth + shore-clearance only.
    let lanes = if cfg.use_lanes {
        match LaneField::load(&cfg.lanes_bin, &cfg.lanes_json) {
            Ok(l) => {
                eprintln!("Loaded lane prior from {}", cfg.lanes_bin);
                Some(l)
            }
            Err(e) => {
                eprintln!("No lane prior ({e}); depth + clearance only");
                None
            }
        }
    } else {
        None
    };

    // Canonical port positions from the shallowest class (reaches the most
    // ports); a port unreachable even by the shallowest class is dropped.
    let shallow = class_by_name("passenger");
    let shallow_field = router.class_field(shallow, None);
    let max_ring = (60.0 / cfg.grid_nm).ceil() as usize;
    let mut ports: Vec<CanonPort> = Vec::new();
    for (name, lat, lon) in PORTS {
        if let Some(cell) = router.snap(*lat, *lon, &shallow_field, max_ring) {
            let (cx, cy) = router.cell_center_of(cell);
            let (slat, slon) = simulation::ais::field_to_lat_lon(cx, cy);
            ports.push(CanonPort {
                name: (*name).to_string(),
                lat: slat,
                lon: slon,
            });
        } else {
            eprintln!("  ! {name} unreachable — dropped");
        }
    }
    eprintln!(
        "Anchored {} / {} ports to navigable water",
        ports.len(),
        PORTS.len()
    );
    if ports.len() < 2 {
        bail!("need at least 2 reachable ports");
    }
    write_ports(&cfg.ports_out, &ports)?;
    eprintln!("Wrote {} ports → {}", ports.len(), cfg.ports_out);

    let mut rng = StdRng::seed_from_u64(cfg.seed);
    let mut routes: Vec<serde_json::Value> = Vec::new();
    let mut mmsi: u64 = 900_000_001;
    let mut total_wp = 0usize;

    for (vtype, count) in cfg.quotas {
        let class = class_by_name(vtype);
        // Per-class lane attractiveness (cargo follows cargo lanes, etc.).
        let lane_att = lanes.as_ref().and_then(|l| {
            l.band_index(vtype)
                .map(|b| router.lane_attractiveness(l, b))
        });
        let field = router.class_field(class, lane_att.as_deref());
        // Which canonical ports this class can actually reach (deep enough,
        // and its approach cell is within ~12 nm of the port).
        let usable: Vec<(usize, usize)> = ports // (port_index, cell)
            .iter()
            .enumerate()
            .filter_map(|(pi, p)| {
                router.snap(p.lat, p.lon, &field, max_ring).and_then(|c| {
                    let (cx, cy) = router.cell_center_of(c);
                    let (px, py) = lat_lon_to_field(p.lat, p.lon);
                    ((cx - px).hypot(cy - py) <= 12.0).then_some((pi, c))
                })
            })
            .collect();
        if usable.len() < 2 {
            eprintln!(
                "  ! {vtype}: fewer than 2 reachable ports (draft {} m) — skipped",
                class.required_depth_m()
            );
            continue;
        }

        let mut made = 0;
        let mut attempts = 0;
        let mut rejected = 0;
        let epsilon = 2.0 * cfg.grid_nm;
        while made < count && attempts < count * 60 + 300 {
            attempts += 1;
            let (opi, ocell) = usable[rng.gen_range(0..usable.len())];
            let (dpi, dcell) = usable[rng.gen_range(0..usable.len())];
            if opi == dpi {
                continue;
            }
            let (ox, oy) = router.cell_center_of(ocell);
            let (dx, dy) = router.cell_center_of(dcell);
            if (ox - dx).hypot(oy - dy) < 80.0 {
                continue;
            }
            // Seeded per-route jitter → deterministic but varied tracks.
            let salt = cfg.seed ^ mmsi.wrapping_mul(0x9E37_79B1);
            let Some(cells) = router.astar(&field, ocell, dcell, salt) else {
                continue;
            };
            let pts: Vec<(f64, f64)> = cells.iter().map(|&c| router.cell_center_of(c)).collect();
            let simplified = simplify_passable(&router, &field, &pts, epsilon);
            // Final guarantee against real bathymetry (finer than the router
            // grid): reject if the polyline ever dips below the class draft+UKC.
            if simplified.len() < 2
                || !route_deep_enough(&bathy, &simplified, class.required_depth_m())
            {
                rejected += 1;
                continue;
            }
            let waypoints: Vec<serde_json::Value> = path_to_latlon(&simplified)
                .into_iter()
                .map(|(lat, lon)| json!({ "lat": lat, "lon": lon }))
                .collect();
            total_wp += simplified.len();
            routes.push(json!({
                "mmsi": mmsi,
                "name": format!("{}-{}", ports[opi].name, ports[dpi].name),
                "vessel_type": vtype,
                "waypoints": waypoints,
            }));
            mmsi += 1;
            made += 1;
        }
        eprintln!(
            "  {vtype}: {made}/{count} routes  ({} usable ports, {attempts} attempts, {rejected} rejected)",
            usable.len()
        );
    }

    if routes.is_empty() {
        bail!("generated 0 routes");
    }
    let mean_wp = total_wp as f64 / routes.len() as f64;
    eprintln!(
        "Route quality: {} routes, mean {mean_wp:.1} waypoints, 0 cross shallow water",
        routes.len()
    );
    std::fs::write(&cfg.out, serde_json::to_string_pretty(&routes)?)
        .with_context(|| format!("writing {}", cfg.out))?;
    eprintln!("Wrote {} routes → {}\nDone.", routes.len(), cfg.out);
    Ok(())
}

/// Samples the simplified field-nm polyline against the raw bathymetry
/// (~0.25 nm steps) and returns true only if every point has at least `need`
/// metres of water — a resolution-independent safety check on the final route.
fn route_deep_enough(bathy: &Bathymetry, points: &[(f64, f64)], need: f64) -> bool {
    points.windows(2).all(|w| {
        let (a, b) = (w[0], w[1]);
        let dist = (b.0 - a.0).hypot(b.1 - a.1);
        let steps = (dist / 0.15).ceil().max(1.0) as usize;
        (0..=steps).all(|s| {
            let t = s as f64 / steps as f64;
            let x = a.0 + (b.0 - a.0) * t;
            let y = a.1 + (b.1 - a.1) * t;
            bathy.available_depth_field(x, y) >= need
        })
    })
}

/// Writes `ports.json`: a small box (±~0.07°) around each anchored sea position.
fn write_ports(path: &str, ports: &[CanonPort]) -> Result<()> {
    const H_LAT: f64 = 0.06;
    const H_LON: f64 = 0.10;
    let recs: Vec<serde_json::Value> = ports
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "keypoints": [
                    { "lat": p.lat + H_LAT, "lon": p.lon - H_LON },
                    { "lat": p.lat + H_LAT, "lon": p.lon + H_LON },
                    { "lat": p.lat - H_LAT, "lon": p.lon + H_LON },
                    { "lat": p.lat - H_LAT, "lon": p.lon - H_LON },
                ],
            })
        })
        .collect();
    std::fs::write(path, serde_json::to_string_pretty(&recs)?)
        .with_context(|| format!("writing {path}"))?;
    Ok(())
}
