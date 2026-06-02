//! AIS track extractor.
//! Reads all data/aisdk-*.csv files, reconstructs per-vessel tracks,
//! selects vessels with the most complete multi-day voyages, and writes
//! data/ais_paths.json in the format expected by the simulation.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

// Baltic Sea bounding box (same as simulation)
const LAT_MIN: f64 = 53.5;
const LAT_MAX: f64 = 66.0;
const LON_MIN: f64 = 9.5;
const LON_MAX: f64 = 30.5;

const MIN_SOG: f64 = 0.5;          // knots — ignore stationary pings
const BUCKET_MINUTES: u32 = 30;    // keep one ping per 30-min window per vessel
const MIN_POINTS: usize = 20;      // after subsampling, track must have ≥ 20 points
// Type quotas for the output routes
const QUOTA_CARGO: usize = 35;
const QUOTA_PASSENGER: usize = 25;
const QUOTA_TANKER: usize = 20;

#[derive(Debug, Clone)]
struct Ping {
    /// Minutes since 2026-01-01 00:00 (fits in u32 for ~8 years)
    time_min: u32,
    lat: f32,
    lon: f32,
}

#[derive(Debug, Default)]
struct VesselInfo {
    name: String,
    ship_type: String,
    /// time-bucketed pings, keyed by (time_min / BUCKET_MINUTES)
    buckets: HashMap<u32, Ping>,
    /// set of calendar days (YYYYMMDD) seen
    days: std::collections::HashSet<u32>,
}

fn main() -> Result<()> {
    let data_dir = PathBuf::from("data");
    let out_path = data_dir.join("ais_paths.json");

    // Collect all CSV files
    let mut csv_files: Vec<PathBuf> = fs::read_dir(&data_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("csv")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("aisdk-"))
                    .unwrap_or(false)
        })
        .collect();
    csv_files.sort();

    if csv_files.is_empty() {
        anyhow::bail!("No aisdk-*.csv files found in data/");
    }
    eprintln!("Found {} CSV file(s)", csv_files.len());

    let mut vessels: HashMap<u64, VesselInfo> = HashMap::new();
    let mut total_rows: u64 = 0;
    let mut kept_rows: u64 = 0;

    for csv_path in &csv_files {
        eprintln!("Processing {:?} …", csv_path.file_name().unwrap());
        let f = fs::File::open(csv_path)
            .with_context(|| format!("opening {csv_path:?}"))?;
        let reader = BufReader::with_capacity(4 * 1024 * 1024, f);
        let mut lines = reader.lines();

        // Skip header
        lines.next();

        for line in lines {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            total_rows += 1;

            let row = parse_row(&line);
            let row = match row {
                Some(r) => r,
                None => continue,
            };

            // Filter mobile type
            if row.mobile_type != "Class A" {
                continue;
            }

            // Filter ship type
            let st = row.ship_type.as_str();
            if !matches!(st, "Cargo" | "Passenger" | "Tanker") {
                continue;
            }

            // Filter geographic bounds
            let lat = row.lat as f64;
            let lon = row.lon as f64;
            if lat < LAT_MIN || lat > LAT_MAX || lon < LON_MIN || lon > LON_MAX {
                continue;
            }

            // Filter bad GPS
            if row.lat == 91.0 || (row.lat == 0.0 && row.lon == 0.0) {
                continue;
            }

            // Filter SOG
            if row.sog < MIN_SOG as f32 {
                continue;
            }

            kept_rows += 1;
            let bucket = row.time_min / BUCKET_MINUTES;
            let day = row.day_yyyymmdd;

            let entry = vessels.entry(row.mmsi).or_default();
            if entry.name.is_empty() && !row.name.is_empty() && row.name != "Unknown" {
                entry.name = row.name.clone();
            }
            if entry.ship_type.is_empty() {
                entry.ship_type = row.ship_type.clone();
            }
            entry.days.insert(day);
            entry.buckets.entry(bucket).or_insert(Ping {
                time_min: row.time_min,
                lat: row.lat,
                lon: row.lon,
            });
        }
    }

    eprintln!(
        "Total rows: {total_rows} | Kept: {kept_rows} | Unique MMSIs: {}",
        vessels.len()
    );

    // Filter: minimum track length (at least MIN_POINTS 30-min buckets)
    let mut by_type: HashMap<String, Vec<(u64, VesselInfo)>> = HashMap::new();
    for (mmsi, info) in vessels {
        if info.buckets.len() < MIN_POINTS {
            continue;
        }
        by_type
            .entry(info.ship_type.to_lowercase())
            .or_default()
            .push((mmsi, info));
    }

    // Sort each type bucket by (n_days DESC, n_points DESC)
    let sort_fn = |a: &(u64, VesselInfo), b: &(u64, VesselInfo)| {
        b.1.days
            .len()
            .cmp(&a.1.days.len())
            .then(b.1.buckets.len().cmp(&a.1.buckets.len()))
    };

    for bucket in by_type.values_mut() {
        bucket.sort_by(sort_fn);
    }

    eprintln!("Vessels per type (after min-points filter):");
    for (t, v) in &by_type {
        eprintln!("  {t}: {}", v.len());
    }

    let quota = [("cargo", QUOTA_CARGO), ("passenger", QUOTA_PASSENGER), ("tanker", QUOTA_TANKER)];
    let mut candidates: Vec<(u64, VesselInfo)> = Vec::new();
    for (type_name, max) in quota {
        if let Some(bucket) = by_type.remove(type_name) {
            candidates.extend(bucket.into_iter().take(max));
        }
    }
    eprintln!("Selected {} routes total", candidates.len());

    // Build output JSON
    let routes: Vec<Value> = candidates
        .into_iter()
        .map(|(mmsi, info)| {
            let mut pings: Vec<&Ping> = info.buckets.values().collect();
            pings.sort_by_key(|p| p.time_min);

            let waypoints: Vec<Value> = pings
                .iter()
                .map(|p| {
                    // Convert time_min back to an ISO-ish timestamp string
                    let ts = format_timestamp(p.time_min);
                    json!({ "lat": p.lat, "lon": p.lon, "timestamp_utc": ts })
                })
                .collect();

            let name = if info.name.is_empty() {
                format!("MMSI-{mmsi}")
            } else {
                info.name.clone()
            };

            json!({
                "mmsi": mmsi,
                "name": name,
                "vessel_type": info.ship_type.to_lowercase(),
                "waypoints": waypoints,
            })
        })
        .collect();

    eprintln!("Writing {} routes to {:?}", routes.len(), out_path);
    let json_str = serde_json::to_string_pretty(&routes)?;
    fs::write(&out_path, json_str)?;
    eprintln!("Done.");

    Ok(())
}

struct Row {
    mobile_type: String,
    mmsi: u64,
    lat: f32,
    lon: f32,
    sog: f32,
    name: String,
    ship_type: String,
    /// Minutes since 2026-01-01 00:00:00 UTC (approximate — ignores leap seconds)
    time_min: u32,
    /// YYYYMMDD as u32 for day-dedup
    day_yyyymmdd: u32,
}

/// Fast field splitter: split on comma, respecting the file's simple CSV (no quoting in practice).
fn parse_row(line: &str) -> Option<Row> {
    // Fields: 0=Timestamp, 1=MobileType, 2=MMSI, 3=Lat, 4=Lon, 5=NavStatus,
    //         6=ROT, 7=SOG, 8=COG, 9=Heading, 10=IMO, 11=Callsign, 12=Name,
    //         13=ShipType, ...
    let mut fields = line.splitn(25, ',');
    let timestamp = fields.next()?;            // 0
    let mobile_type = fields.next()?.trim();   // 1
    let mmsi_str = fields.next()?.trim();      // 2
    let lat_str = fields.next()?.trim();       // 3
    let lon_str = fields.next()?.trim();       // 4
    let _nav = fields.next();                  // 5
    let _rot = fields.next();                  // 6
    let sog_str = fields.next()?.trim();       // 7
    let _cog = fields.next();                  // 8
    let _hdg = fields.next();                  // 9
    let _imo = fields.next();                  // 10
    let _call = fields.next();                 // 11
    let name = fields.next()?.trim();          // 12
    let ship_type = fields.next()?.trim();     // 13

    let mmsi: u64 = mmsi_str.parse().ok()?;
    let lat: f32 = lat_str.parse().ok()?;
    let lon: f32 = lon_str.parse().ok()?;
    let sog: f32 = sog_str.parse().unwrap_or(0.0);

    let (time_min, day) = parse_timestamp(timestamp)?;

    Some(Row {
        mobile_type: mobile_type.to_string(),
        mmsi,
        lat,
        lon,
        sog,
        name: name.to_string(),
        ship_type: ship_type.to_string(),
        time_min,
        day_yyyymmdd: day,
    })
}

/// Parses "DD/MM/YYYY HH:MM:SS" → (minutes_since_2026-01-01, YYYYMMDD)
fn parse_timestamp(s: &str) -> Option<(u32, u32)> {
    // Expected: "03/05/2026 00:00:00"
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let day: u32 = s[0..2].parse().ok()?;
    let month: u32 = s[3..5].parse().ok()?;
    let year: u32 = s[6..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;

    // Days since 2026-01-01 (approximate, ignores leap years for simplicity)
    let days_since_epoch = days_from_2026(year, month, day);
    let total_min = days_since_epoch * 24 * 60 + hour * 60 + minute;
    let yyyymmdd = year * 10000 + month * 100 + day;

    Some((total_min, yyyymmdd))
}

fn days_from_2026(year: u32, month: u32, day: u32) -> u32 {
    // Days per month (non-leap year), good enough for 2026
    const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let year_days = (year.saturating_sub(2026)) * 365;
    let month_days: u32 = DAYS_IN_MONTH[..((month as usize).saturating_sub(1)).min(12)]
        .iter()
        .sum();
    year_days + month_days + day.saturating_sub(1)
}

fn format_timestamp(time_min: u32) -> String {
    let total_min = time_min;
    let minute = total_min % 60;
    let total_hr = total_min / 60;
    let hour = total_hr % 24;
    let total_days = total_hr / 24;

    const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut remaining = total_days;
    let mut year = 2026u32;
    let mut month = 1u32;
    loop {
        let days_in_year = 365;
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    loop {
        let dim = DAYS_IN_MONTH[(month as usize) - 1];
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
        if month > 12 {
            break;
        }
    }
    let day = remaining + 1;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z")
}
