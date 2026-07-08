//! Real sea-depth grid loaded from the bundled EMODnet bathymetry
//! (`data/bathymetry.bin.gz` + `data/bathymetry.json`, produced by
//! `tools/fetch_bathymetry.py`).
//!
//! Depths are metres with **negative = below sea level**; land / above-sea
//! cells are non-negative. The grid is a regular lat/lon raster with row 0 at
//! the south edge (lat ascending) and column 0 at the west edge (lon ascending).

use anyhow::{ensure, Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use simulation::ais::field_to_lat_lon;
use std::fs;
use std::io::Read;

#[derive(Debug, Deserialize)]
struct Meta {
    lat_min: f64,
    lon_min: f64,
    dlat: f64,
    dlon: f64,
    nlat: usize,
    nlon: usize,
}

/// A bundled bathymetric depth grid over the simulation bbox.
pub struct Bathymetry {
    meta: Meta,
    /// Row-major (`nlat` × `nlon`) depth in metres; negative = below sea level.
    depth: Vec<i16>,
}

impl Bathymetry {
    /// Loads the gzipped int16 depth grid and its JSON header.
    ///
    /// # Errors
    /// Fails if either file is missing/malformed or their sizes disagree.
    pub fn load(bin_gz: &str, json: &str) -> Result<Self> {
        let meta: Meta = serde_json::from_str(
            &fs::read_to_string(json).with_context(|| format!("reading {json}"))?,
        )
        .with_context(|| format!("parsing {json}"))?;

        let file = fs::File::open(bin_gz).with_context(|| format!("opening {bin_gz}"))?;
        let mut bytes = Vec::new();
        GzDecoder::new(file)
            .read_to_end(&mut bytes)
            .with_context(|| format!("decompressing {bin_gz}"))?;

        let expected = meta.nlat * meta.nlon;
        ensure!(
            bytes.len() == expected * 2,
            "bathymetry size mismatch: {} bytes for {}×{} grid",
            bytes.len(),
            meta.nlat,
            meta.nlon
        );
        let depth = bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(Self { meta, depth })
    }

    /// Builds a grid directly from parts — for tests and synthetic fixtures.
    #[doc(hidden)]
    #[must_use]
    pub fn from_raw(
        lat_min: f64,
        lon_min: f64,
        dlat: f64,
        dlon: f64,
        nlat: usize,
        nlon: usize,
        depth: Vec<i16>,
    ) -> Self {
        assert_eq!(depth.len(), nlat * nlon, "depth length must be nlat*nlon");
        Self {
            meta: Meta {
                lat_min,
                lon_min,
                dlat,
                dlon,
                nlat,
                nlon,
            },
            depth,
        }
    }

    /// Depth in metres at a lat/lon (negative = below sea level). Points outside
    /// the grid return `0.0` (treated as land — safe, i.e. non-navigable).
    #[must_use]
    pub fn depth_m(&self, lat: f64, lon: f64) -> f64 {
        let fi = (lat - self.meta.lat_min) / self.meta.dlat;
        let fj = (lon - self.meta.lon_min) / self.meta.dlon;
        if fi < 0.0 || fj < 0.0 {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (fi as usize, fj as usize);
        if i >= self.meta.nlat || j >= self.meta.nlon {
            return 0.0;
        }
        f64::from(self.depth[i * self.meta.nlon + j])
    }

    /// Available water depth (metres, positive) at a field-nm coordinate;
    /// `0.0` on land / above sea level.
    #[must_use]
    pub fn available_depth_field(&self, x: f64, y: f64) -> f64 {
        let (lat, lon) = field_to_lat_lon(x, y);
        (-self.depth_m(lat, lon)).max(0.0)
    }
}
