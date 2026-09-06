use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

// =====================================================================
// Geographic projection
// ---------------------------------------------------------------------
// Bounding box covering both the Baltic Sea and the North Sea, plus a
// margin around the eastern UK coast and the Skagerrak / Kattegat hinge.
//
//   N edge: 66.0°  (mid-Norwegian coast / Bothnian Sea head)
//   S edge: 50.5°  (English Channel / Dutch coast)
//   W edge: -5.0°  (Scottish east coast / North Sea opening)
//   E edge: 31.0°  (Gulf of Finland / St. Petersburg approaches)
//
// Field coordinates are EQUIRECTANGULAR with cos(lat_mid) correction:
// one field unit equals exactly one nautical mile in both axes at the
// simulation midpoint, so the engine can work in nm directly without a
// scaling factor.  At the bbox edges the E-W distortion is at most ~6 %
// over the Baltic and ~10 % over the southern North Sea — small enough
// that ship dynamics, mesh radii, and storm geometry behave physically
// without distortion-correction terms scattered through the code.
// =====================================================================

/// Northern latitude bound of the simulation domain (degrees).
pub const LAT_MAX: f64 = 66.0;
/// Southern latitude bound of the simulation domain (degrees).
pub const LAT_MIN: f64 = 50.5;
/// Western longitude bound of the simulation domain (degrees).
pub const LON_MIN: f64 = -5.0;
/// Eastern longitude bound of the simulation domain (degrees).
pub const LON_MAX: f64 = 31.0;

/// Nautical miles per degree of latitude (rigorous: 60 nm).
pub const NM_PER_DEG_LAT: f64 = 60.0;

/// Returns the midpoint latitude of the domain, used as the reference
/// parallel for the equirectangular projection.
#[inline]
#[must_use]
pub fn lat_mid() -> f64 {
    f64::midpoint(LAT_MIN, LAT_MAX)
}

/// Returns the nautical-miles-per-degree-longitude scale factor at the
/// reference latitude, i.e. `60 · cos(φ_mid)`.
#[inline]
#[must_use]
pub fn nm_per_deg_lon() -> f64 {
    NM_PER_DEG_LAT * lat_mid().to_radians().cos()
}

/// Width of the simulation field in nautical miles.
/// Approximately 1136 nm for the default Baltic + North Sea bbox.
#[inline]
#[must_use]
pub fn field_width_nm() -> f64 {
    (LON_MAX - LON_MIN) * nm_per_deg_lon()
}

/// Height of the simulation field in nautical miles.
/// Exactly 15.5° × 60 nm = 930 nm for the default bbox.
#[inline]
#[must_use]
pub fn field_height_nm() -> f64 {
    (LAT_MAX - LAT_MIN) * NM_PER_DEG_LAT
}

/// Projects (lat, lon) onto field coordinates measured in nautical miles.
///
/// `x` increases eastward from `LON_MIN`; `y` increases northward from
/// `LAT_MIN`.  The map is equirectangular: one field-unit step in either
/// axis is exactly one nautical mile at the reference latitude.
#[must_use]
pub fn lat_lon_to_field(lat: f64, lon: f64) -> (f64, f64) {
    let x = (lon - LON_MIN) * nm_per_deg_lon();
    let y = (lat - LAT_MIN) * NM_PER_DEG_LAT;
    (x, y)
}

/// Inverse of `lat_lon_to_field`.
#[must_use]
pub fn field_to_lat_lon(x: f64, y: f64) -> (f64, f64) {
    let lon = x / nm_per_deg_lon() + LON_MIN;
    let lat = y / NM_PER_DEG_LAT + LAT_MIN;
    (lat, lon)
}

/// Returns `true` if a field coordinate lies inside the simulation domain.
#[inline]
#[must_use]
pub fn in_bounds(x: f64, y: f64) -> bool {
    x >= 0.0 && x <= field_width_nm() && y >= 0.0 && y <= field_height_nm()
}

// =====================================================================
// AIS data
// =====================================================================

/// A single AIS position report.  `timestamp_utc` is optional — the new
/// keypoint-only format omits it; the legacy timestamped format includes it.
#[derive(Debug, Clone, Deserialize)]
pub struct AisWaypoint {
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub timestamp_utc: Option<String>,
}

/// A full AIS vessel record with route waypoints.
#[derive(Debug, Clone, Deserialize)]
pub struct AisRecord {
    pub mmsi: u64,
    pub name: String,
    pub vessel_type: String,
    pub waypoints: Vec<AisWaypoint>,
}

// ─── Port definitions (from data/ports.json) ─────────────────────────────────

/// One lat/lon corner of a port polygon, as stored in `ports.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PortKeypoint {
    pub lat: f64,
    pub lon: f64,
}

/// Raw record from `ports.json` before projection.
#[derive(Debug, Clone, Deserialize)]
pub struct PortRecord {
    pub name: String,
    pub keypoints: Vec<PortKeypoint>,
}

/// A port — a bounding polygon (projected to field coordinates) at which
/// vessels automatically dock on entry.  Loaded from `data/ports.json`.
#[derive(Debug, Clone)]
pub struct Port {
    pub id: u32,
    pub name: String,
    /// Centroid of the polygon in field (nm) coordinates.
    pub position: (f64, f64),
    /// Polygon corners in field (nm) coordinates — used for point-in-polygon
    /// containment checks.
    pub polygon: Vec<(f64, f64)>,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Point-in-polygon test using the ray-casting algorithm.
/// `polygon` is a slice of (x, y) field-coordinate vertices (closed implicitly).
/// Returns `true` when `(px, py)` is strictly inside.
#[must_use]
pub fn point_in_polygon(px: f64, py: f64, polygon: &[(f64, f64)]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > py) != (yj > py) {
            let x_intersect = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Loads AIS records from a JSON file.
///
/// # Errors
/// Returns an error if the file cannot be read or the JSON is malformed.
pub fn load_ais(path: &str) -> Result<Vec<AisRecord>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read AIS file: {path}"))?;
    let records: Vec<AisRecord> = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse AIS JSON: {path}"))?;
    Ok(records)
}

/// Loads port definitions from a `ports.json` file.
///
/// # Errors
/// Returns an error if the file cannot be read or the JSON is malformed.
///
/// Each port's 4 keypoints define a quadrilateral bounding polygon.  The
/// centroid is computed automatically as the mean of the corners.
pub fn load_ports_from_file(path: &str) -> Result<Vec<Port>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read ports file: {path}"))?;
    let records: Vec<PortRecord> = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse ports JSON: {path}"))?;

    let ports = records
        .into_iter()
        .enumerate()
        .map(|(i, rec)| {
            let polygon: Vec<(f64, f64)> = rec
                .keypoints
                .iter()
                .map(|kp| lat_lon_to_field(kp.lat, kp.lon))
                .collect();
            #[allow(clippy::cast_precision_loss)]
            let n = polygon.len() as f64;
            let cx = polygon.iter().map(|p| p.0).sum::<f64>() / n;
            let cy = polygon.iter().map(|p| p.1).sum::<f64>() / n;
            #[allow(clippy::cast_possible_truncation)]
            let id = i as u32;
            Port {
                id,
                name: rec.name,
                position: (cx, cy),
                polygon,
            }
        })
        .collect();

    Ok(ports)
}

/// Derives a deduplicated list of ports from the endpoints of every AIS
/// route.  Two endpoints within `dedup_radius_nm` of each other collapse
/// into a single port, named after the first route to touch it.
///
/// Used when the simulation has no explicit `ports.json` — ports are
/// inferred from the start and end waypoints of each polyline.
/// # Panics
/// Panics if a record has waypoints but `first()` or `last()` returns `None` (can't happen).
#[must_use]
pub fn extract_ports(ais: &[AisRecord], dedup_radius_nm: f64) -> Vec<Port> {
    let mut ports: Vec<Port> = Vec::new();
    let r2 = dedup_radius_nm * dedup_radius_nm;
    let mut next_id: u32 = 0;

    for rec in ais {
        if rec.waypoints.is_empty() {
            continue;
        }
        let endpoints = [
            rec.waypoints.first().unwrap(),
            rec.waypoints.last().unwrap(),
        ];
        for (k, wp) in endpoints.iter().enumerate() {
            let pos = lat_lon_to_field(wp.lat, wp.lon);
            let already = ports.iter().any(|p| {
                let dx = p.position.0 - pos.0;
                let dy = p.position.1 - pos.1;
                dx * dx + dy * dy < r2
            });
            if !already {
                let suffix = if k == 0 { "Origin" } else { "Destination" };
                ports.push(Port {
                    id: next_id,
                    name: format!("{} ({})", rec.name, suffix),
                    position: pos,
                    polygon: vec![], // inferred ports have no polygon
                });
                next_id += 1;
            }
        }
    }
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ais_parses_sample_file() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/ais_paths.json");
        let records = load_ais(path).unwrap();
        assert!(!records.is_empty(), "expected at least one AIS record");
        for r in &records {
            assert!(
                r.waypoints.len() >= 2,
                "vessel {} has fewer than 2 waypoints",
                r.mmsi
            );
            for wp in &r.waypoints {
                assert!(
                    (-90.0..=90.0).contains(&wp.lat),
                    "lat out of range: {}",
                    wp.lat
                );
                assert!(
                    (-180.0..=180.0).contains(&wp.lon),
                    "lon out of range: {}",
                    wp.lon
                );
            }
        }
    }

    #[test]
    fn test_coord_conversion_round_trip() {
        let cases = [
            (59.334, 18.063), // Stockholm
            (54.352, 18.646), // Gdańsk
            (60.455, 22.264), // Turku
            (53.547, 9.984),  // Hamburg
            (52.378, 4.900),  // Amsterdam
            (60.392, 5.323),  // Bergen
        ];
        for (lat, lon) in cases {
            let (x, y) = lat_lon_to_field(lat, lon);
            let (lat2, lon2) = field_to_lat_lon(x, y);
            assert!(
                (lat - lat2).abs() < 1e-9,
                "lat round-trip failed: {lat} → {lat2}"
            );
            assert!(
                (lon - lon2).abs() < 1e-9,
                "lon round-trip failed: {lon} → {lon2}"
            );
        }
    }

    #[test]
    fn test_projection_field_dimensions() {
        // 15.5° × 60 nm = exactly 930 nm height
        assert!((field_height_nm() - 930.0).abs() < 1e-9);
        // 36° × 60 × cos(58.25°) ≈ 1136 nm width
        let expected_w = 36.0 * 60.0 * 58.25_f64.to_radians().cos();
        assert!((field_width_nm() - expected_w).abs() < 1e-6);
    }

    #[test]
    fn test_field_origin_at_sw_corner() {
        let (x, y) = lat_lon_to_field(LAT_MIN, LON_MIN);
        assert!(x.abs() < 1e-9 && y.abs() < 1e-9);
    }

    #[test]
    fn test_in_bounds_covers_known_ports() {
        for (lat, lon) in [
            (51.949, 4.063),  // Rotterdam approach
            (53.547, 9.984),  // Hamburg
            (59.334, 18.063), // Stockholm
            (60.171, 24.941), // Helsinki
        ] {
            let (x, y) = lat_lon_to_field(lat, lon);
            assert!(
                in_bounds(x, y),
                "({lat},{lon}) → ({x:.1},{y:.1}) out of domain"
            );
        }
    }
}
