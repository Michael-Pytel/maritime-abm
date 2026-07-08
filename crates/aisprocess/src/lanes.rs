//! Per-vessel-class shipping-lane attractiveness grid, loaded from the bundled
//! EMODnet vessel-density maps (`data/lanes.bin.gz` + `data/lanes.json`, built
//! by `tools/fetch_lanes.py`).
//!
//! Three bands — passenger / cargo / tanker — hold `0..1` attractiveness (1 =
//! the busiest lane for that class). The router turns *low* attractiveness into
//! a small extra cost, so least-cost paths gravitate to the corridors that
//! class of ship actually uses.

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
    bands: Vec<String>,
}

/// A stack of per-class lane-attractiveness grids.
pub struct LaneField {
    meta: Meta,
    /// Band-major then row-major `u8` (row 0 = south, col 0 = west).
    data: Vec<u8>,
}

impl LaneField {
    /// Loads the gzipped `u8` band stack and its JSON header.
    ///
    /// # Errors
    /// Fails if either file is missing/malformed or their sizes disagree.
    pub fn load(bin_gz: &str, json: &str) -> Result<Self> {
        let meta: Meta = serde_json::from_str(
            &fs::read_to_string(json).with_context(|| format!("reading {json}"))?,
        )
        .with_context(|| format!("parsing {json}"))?;
        let file = fs::File::open(bin_gz).with_context(|| format!("opening {bin_gz}"))?;
        let mut data = Vec::new();
        GzDecoder::new(file)
            .read_to_end(&mut data)
            .with_context(|| format!("decompressing {bin_gz}"))?;
        let expected = meta.bands.len() * meta.nlat * meta.nlon;
        ensure!(
            data.len() == expected,
            "lanes size mismatch: {} bytes for {} bands × {}×{}",
            data.len(),
            meta.bands.len(),
            meta.nlat,
            meta.nlon
        );
        Ok(Self { meta, data })
    }

    /// Band index for a class name (`"passenger"`, `"cargo"`, `"tanker"`).
    #[must_use]
    pub fn band_index(&self, name: &str) -> Option<usize> {
        self.meta.bands.iter().position(|b| b == name)
    }

    /// Attractiveness in `[0, 1]` for a band at a lat/lon (0 outside the grid).
    #[must_use]
    pub fn attractiveness(&self, band: usize, lat: f64, lon: f64) -> f64 {
        let fi = (lat - self.meta.lat_min) / self.meta.dlat;
        let fj = (lon - self.meta.lon_min) / self.meta.dlon;
        if fi < 0.0 || fj < 0.0 || band >= self.meta.bands.len() {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (fi as usize, fj as usize);
        if i >= self.meta.nlat || j >= self.meta.nlon {
            return 0.0;
        }
        let idx = band * self.meta.nlat * self.meta.nlon + i * self.meta.nlon + j;
        f64::from(self.data[idx]) / 255.0
    }

    /// Attractiveness in `[0, 1]` for a band at a field-nm coordinate.
    #[must_use]
    pub fn attractiveness_field(&self, band: usize, x: f64, y: f64) -> f64 {
        let (lat, lon) = field_to_lat_lon(x, y);
        self.attractiveness(band, lat, lon)
    }
}
