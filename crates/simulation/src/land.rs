//! Polygon-based land mask.
//!
//! Replaces the older `Vec<LandBoxField>` axis-aligned-bounding-box mask
//! with real coastline geometry sourced from Natural Earth 1:10m physical
//! land polygons (via `tools/build_coastline.py`), projected to the
//! simulation's field-nm coordinates.
//!
//! Why this exists: AABBs cannot represent the U-shaped channels between
//! adjacent landmasses (Øresund, the Belts, fjords, archipelago straits)
//! without spuriously flagging deep navigable water as "land". The
//! polygon representation can.
//!
//! Performance: queries are accelerated by an R-tree on per-polygon
//! axis-aligned bounding boxes. With ~5k polygons in the Baltic + North
//! Sea bbox (after Natural Earth simplification at ~0.005° tolerance),
//! a point-in-land query touches typically 1–3 polygons and runs in
//! single-digit microseconds.
//!
//! The mask is conceptually immutable — load once at simulation startup,
//! then share read-only across the whole tick pipeline. The `Clone` impl
//! is `Arc`-based so passing the mask into per-vessel code paths is
//! effectively free.

use crate::ais::lat_lon_to_field;
use anyhow::{Context, Result};
use geo::{BoundingRect, Contains, Coord, Intersects, Line as GeoLine, LineString, Polygon};
use geojson::{GeoJson, Geometry, Value};
use rstar::{RTree, RTreeObject, AABB};
use std::fs;
use std::sync::Arc;

/// Shared, read-only land mask. `Clone` is O(1) (Arc bump).
#[derive(Clone)]
pub struct LandMask {
    inner: Arc<LandMaskInner>,
}

struct LandMaskInner {
    polygons: Vec<Polygon<f64>>,
    rtree: RTree<RTreeEntry>,
}

#[derive(Clone, Debug)]
struct RTreeEntry {
    idx: usize,
    bbox: AABB<[f64; 2]>,
}

impl RTreeObject for RTreeEntry {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

impl LandMask {
    /// An empty mask. Useful for tests and as a safe placeholder.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(LandMaskInner {
                polygons: Vec::new(),
                rtree: RTree::new(),
            }),
        }
    }

    /// Build a mask directly from polygons already in field-nm coordinates.
    /// Primarily used by tests and by `from_geojson` after projection.
    #[must_use]
    pub fn from_polygons(polygons: Vec<Polygon<f64>>) -> Self {
        let entries: Vec<RTreeEntry> = polygons
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| {
                let rect = p.bounding_rect()?;
                let mn = rect.min();
                let mx = rect.max();
                Some(RTreeEntry {
                    idx,
                    bbox: AABB::from_corners([mn.x, mn.y], [mx.x, mx.y]),
                })
            })
            .collect();
        let rtree = RTree::bulk_load(entries);
        Self {
            inner: Arc::new(LandMaskInner { polygons, rtree }),
        }
    }

    /// Loads land polygons from a `GeoJSON` file. Coordinates in the file
    /// must be WGS84 lon/lat (the `GeoJSON` spec order); they are projected
    /// to field-nm at load time via `crate::ais::lat_lon_to_field`.
    ///
    /// Supports Polygon, `MultiPolygon`, and `GeometryCollection` geometries,
    /// at any depth of nesting inside a `FeatureCollection`. Holes (interior
    /// rings) are preserved.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the `GeoJSON` is malformed.
    pub fn from_geojson(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read coastline file: {path}"))?;
        let geojson: GeoJson = contents
            .parse()
            .with_context(|| format!("failed to parse coastline GeoJSON: {path}"))?;

        let mut polygons = Vec::new();
        match geojson {
            GeoJson::FeatureCollection(fc) => {
                for feat in fc.features {
                    if let Some(geom) = feat.geometry {
                        push_polygons_from_geometry(&geom, &mut polygons);
                    }
                }
            }
            GeoJson::Feature(feat) => {
                if let Some(geom) = feat.geometry {
                    push_polygons_from_geometry(&geom, &mut polygons);
                }
            }
            GeoJson::Geometry(geom) => {
                push_polygons_from_geometry(&geom, &mut polygons);
            }
        }

        Ok(Self::from_polygons(polygons))
    }

    /// Number of polygons in the mask (counting `MultiPolygon` parts separately).
    #[must_use]
    pub fn polygon_count(&self) -> usize {
        self.inner.polygons.len()
    }

    /// Returns true if `(x, y)` (field-nm) lies inside any land polygon.
    #[must_use]
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        let point = geo::Point::new(x, y);
        let envelope = AABB::from_point([x, y]);
        for entry in self.inner.rtree.locate_in_envelope_intersecting(&envelope) {
            if self.inner.polygons[entry.idx].contains(&point) {
                return true;
            }
        }
        false
    }

    /// Returns true if the line segment from `a` to `b` (field-nm)
    /// crosses or enters any land polygon. Used by sub-step grounding
    /// to catch the case where a full vessel step would skip over a thin
    /// land barrier that point-only tests miss.
    #[must_use]
    pub fn segment_crosses_land(&self, a: (f64, f64), b: (f64, f64)) -> bool {
        let line = GeoLine::new(Coord { x: a.0, y: a.1 }, Coord { x: b.0, y: b.1 });
        let (min_x, max_x) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
        let (min_y, max_y) = if a.1 <= b.1 { (a.1, b.1) } else { (b.1, a.1) };
        let envelope = AABB::from_corners([min_x, min_y], [max_x, max_y]);
        for entry in self.inner.rtree.locate_in_envelope_intersecting(&envelope) {
            let poly = &self.inner.polygons[entry.idx];
            if poly.intersects(&line)
                || poly.contains(&geo::Point::new(a.0, a.1))
                || poly.contains(&geo::Point::new(b.0, b.1))
            {
                return true;
            }
        }
        false
    }
}

impl std::fmt::Debug for LandMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LandMask")
            .field("polygons", &self.inner.polygons.len())
            .finish()
    }
}

fn push_polygons_from_geometry(geom: &Geometry, out: &mut Vec<Polygon<f64>>) {
    match &geom.value {
        Value::Polygon(rings) => {
            if let Some(poly) = polygon_from_rings(rings) {
                out.push(poly);
            }
        }
        Value::MultiPolygon(multi) => {
            for rings in multi {
                if let Some(poly) = polygon_from_rings(rings) {
                    out.push(poly);
                }
            }
        }
        Value::GeometryCollection(coll) => {
            for g in coll {
                push_polygons_from_geometry(g, out);
            }
        }
        _ => {} // points, lines, etc. are not part of a land mask
    }
}

/// Converts a `GeoJSON` polygon (outer ring + optional holes, each ring a
/// list of [lon, lat] positions) into a `geo::Polygon<f64>` in field-nm.
fn polygon_from_rings(rings: &[Vec<Vec<f64>>]) -> Option<Polygon<f64>> {
    let mut exterior_coords = Vec::new();
    let mut holes = Vec::new();

    for (i, ring) in rings.iter().enumerate() {
        let mut coords = Vec::with_capacity(ring.len());
        for pos in ring {
            if pos.len() >= 2 {
                // GeoJSON spec: [longitude, latitude]
                let lon = pos[0];
                let lat = pos[1];
                let (x, y) = lat_lon_to_field(lat, lon);
                coords.push(Coord { x, y });
            }
        }
        if coords.len() < 4 {
            // A valid ring needs at least 4 points (3 + repeated first).
            continue;
        }
        if i == 0 {
            exterior_coords = coords;
        } else {
            holes.push(LineString::new(coords));
        }
    }

    if exterior_coords.len() < 4 {
        return None;
    }
    Some(Polygon::new(LineString::new(exterior_coords), holes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> Polygon<f64> {
        let exterior = LineString::new(vec![
            Coord { x: x_min, y: y_min },
            Coord { x: x_max, y: y_min },
            Coord { x: x_max, y: y_max },
            Coord { x: x_min, y: y_max },
            Coord { x: x_min, y: y_min },
        ]);
        Polygon::new(exterior, vec![])
    }

    #[test]
    fn empty_mask_contains_nothing() {
        let m = LandMask::empty();
        assert!(!m.contains_point(0.0, 0.0));
        assert!(!m.segment_crosses_land((0.0, 0.0), (100.0, 100.0)));
        assert_eq!(m.polygon_count(), 0);
    }

    #[test]
    fn rect_polygon_contains_interior() {
        let m = LandMask::from_polygons(vec![rect(10.0, 10.0, 30.0, 30.0)]);
        assert!(m.contains_point(20.0, 20.0));
        assert!(!m.contains_point(5.0, 20.0));
        assert!(!m.contains_point(40.0, 20.0));
        assert!(!m.contains_point(20.0, 50.0));
    }

    #[test]
    fn segment_crosses_land_detects_traversal() {
        // Land at (10..30) × (10..30); segment from open water (0,20) to
        // open water (40,20) crosses straight through.
        let m = LandMask::from_polygons(vec![rect(10.0, 10.0, 30.0, 30.0)]);
        assert!(m.segment_crosses_land((0.0, 20.0), (40.0, 20.0)));
        // Segment fully in water below the rectangle.
        assert!(!m.segment_crosses_land((0.0, 0.0), (40.0, 5.0)));
        // Segment grazing the rectangle but staying in water.
        assert!(!m.segment_crosses_land((0.0, 9.99), (40.0, 9.99)));
    }

    #[test]
    fn segment_with_endpoint_in_land_is_caught() {
        let m = LandMask::from_polygons(vec![rect(10.0, 10.0, 30.0, 30.0)]);
        // Endpoint inside the rectangle counts as land entry.
        assert!(m.segment_crosses_land((0.0, 20.0), (15.0, 20.0)));
    }

    #[test]
    fn rtree_filters_far_polygons() {
        // Two polygons far apart; a query point near one should not need
        // to test the other. We can't observe filtering directly here,
        // but the result must remain correct.
        let m = LandMask::from_polygons(vec![
            rect(0.0, 0.0, 5.0, 5.0),
            rect(900.0, 900.0, 1000.0, 1000.0),
        ]);
        assert!(m.contains_point(2.0, 2.0));
        assert!(m.contains_point(950.0, 950.0));
        assert!(!m.contains_point(500.0, 500.0));
    }
}
