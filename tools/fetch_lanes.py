#!/usr/bin/env python3
"""Build data/lanes.* — a per-vessel-class "lane attractiveness" grid from the
EMODnet Human Activities **vessel-density** maps (AIS-derived hours/km²).

This is the realism layer on top of bathymetry: the depth-aware router keeps
ships in deep water, and this makes least-cost paths prefer the corridors where
that class of ship *actually* sails (Great Belt, the Kiel approaches, the North
Sea lanes …) instead of arbitrary deep-water geodesics.

Per-class coverages: vesseldensity_08 = Passenger, _09 = Cargo, _10 = Tanker.
Data are EPSG:3857 (Web Mercator) with a yearly `time` axis; we fetch one year
and resample onto a regular lat/lon grid. numpy + requests only (no GDAL).

Run once (and again to refresh the year):

    python3 tools/fetch_lanes.py

Outputs (committed, small):
    data/lanes.bin.gz   gzipped uint8, 3 bands [passenger, cargo, tanker],
                        row-major (row 0 = south, col 0 = west); 0..255 = lane
                        attractiveness (255 = busiest lane for that class).
    data/lanes.json     grid header + band order + source/year.

Source: EMODnet Human Activities, Vessel Density Map
        https://emodnet.ec.europa.eu/en/human-activities
"""
from __future__ import annotations

import gzip
import json
import math
import re
import sys
from pathlib import Path

import numpy as np
import requests

# ── bbox: MUST match crates/simulation/src/ais.rs ────────────────────────────
LAT_MIN, LAT_MAX = 50.5, 66.0
LON_MIN, LON_MAX = -5.0, 31.0

# ── output lane grid (coarser than bathymetry — lanes are broad) ─────────────
DLAT = 0.02
DLON = 0.036

WCS = "https://ows.emodnet-humanactivities.eu/wcs"
YEAR = "2023-01-01T00:00:00.000Z"
BANDS = [("passenger", "emodnet__vesseldensity_08"),
         ("cargo", "emodnet__vesseldensity_09"),
         ("tanker", "emodnet__vesseldensity_10")]
SCALE_FACTOR = 0.5
TIMEOUT = 180
R = 6378137.0
OUT_DIR = Path(__file__).resolve().parent.parent / "data"


def merc_x(lon: float) -> float:
    return R * math.radians(lon)


def merc_y(lat: float) -> float:
    return R * math.log(math.tan(math.pi / 4 + math.radians(lat) / 2))


def fetch(coverage: str) -> str:
    params = {
        "service": "WCS", "version": "2.0.1", "request": "GetCoverage",
        "coverageId": coverage,
        "subset": [
            f"X({merc_x(LON_MIN)},{merc_x(LON_MAX)})",
            f"Y({merc_y(LAT_MIN)},{merc_y(LAT_MAX)})",
            f'time("{YEAR}")',
        ],
        "scaleFactor": SCALE_FACTOR,
        "format": "text/plain",
    }
    r = requests.get(WCS, params=params, timeout=TIMEOUT)
    if r.status_code != 200 or "ExceptionReport" in r.text[:400]:
        raise RuntimeError(f"{coverage}: HTTP {r.status_code} {r.text[:200]}")
    return r.text


def parse(text: str):
    """Returns (values[rows,cols] row0=south, x0, y0, x1, y1) in mercator."""
    dims = re.search(r"GridEnvelope2D\[(\d+)\.\.(\d+),\s*(\d+)\.\.(\d+)\]", text)
    bounds = re.search(
        r"GeneralBounds\[\(([-\d.eE]+),\s*([-\d.eE]+)\),\s*\(([-\d.eE]+),\s*([-\d.eE]+)\)\]",
        text,
    )
    cols = int(dims.group(2)) - int(dims.group(1)) + 1
    rows = int(dims.group(4)) - int(dims.group(3)) + 1
    x0, y0, x1, y1 = (float(bounds.group(i)) for i in (1, 2, 3, 4))
    nums = re.findall(r"-?\d+\.\d+(?:[eE][-+]?\d+)?|-?\d+", text)
    vals = np.asarray(nums[-rows * cols:], dtype=np.float64).reshape(rows, cols)
    return vals[::-1, :], x0, y0, x1, y1  # flip: row 0 = south


def normalise(band: np.ndarray) -> np.ndarray:
    """Skewed density -> 0..255 attractiveness (log, clipped at the 99th pct)."""
    valid = band > 0
    out = np.zeros_like(band, dtype=np.float64)
    if valid.any():
        lg = np.log1p(band[valid])
        p99 = np.percentile(lg, 99)
        if p99 > 0:
            out[valid] = np.clip(lg / p99, 0.0, 1.0)
    return np.round(out * 255).astype(np.uint8)


def main() -> int:
    nlat = round((LAT_MAX - LAT_MIN) / DLAT)
    nlon = round((LON_MAX - LON_MIN) / DLON)
    glat = LAT_MIN + (np.arange(nlat) + 0.5) * DLAT
    glon = LON_MIN + (np.arange(nlon) + 0.5) * DLON
    gx = np.array([merc_x(lo) for lo in glon])
    gy = np.array([merc_y(la) for la in glat])
    print(f"Lane grid: {nlat} x {nlon}", flush=True)

    stacked = np.zeros((len(BANDS), nlat, nlon), dtype=np.uint8)
    for b, (name, cov) in enumerate(BANDS):
        vals, x0, y0, x1, y1 = parse(fetch(cov))
        rows, cols = vals.shape
        # Nearest-neighbour resample from the mercator source onto lat/lon cells.
        ci = np.clip(((gx - x0) / (x1 - x0) * cols).astype(int), 0, cols - 1)
        ri = np.clip(((gy - y0) / (y1 - y0) * rows).astype(int), 0, rows - 1)
        block = vals[np.ix_(ri, ci)]
        block[block < 0] = 0.0  # -9999 nodata -> no traffic
        stacked[b] = normalise(block)
        busy = float((stacked[b] > 25).mean() * 100)
        print(f"  {name:9s} {cov}: source {rows}x{cols}, {busy:.1f}% cells have lanes", flush=True)

    OUT_DIR.mkdir(exist_ok=True)
    with gzip.open(OUT_DIR / "lanes.bin.gz", "wb") as f:
        f.write(stacked.tobytes(order="C"))
    meta = {
        "lat_min": LAT_MIN, "lon_min": LON_MIN,
        "dlat": DLAT, "dlon": DLON, "nlat": nlat, "nlon": nlon,
        "bands": [b[0] for b in BANDS],
        "order": "band-major then row-major, row0=south, col0=west; 0..255 attractiveness",
        "source": "EMODnet Human Activities vessel density, year 2023",
    }
    (OUT_DIR / "lanes.json").write_text(json.dumps(meta, indent=2))
    size = (OUT_DIR / "lanes.bin.gz").stat().st_size / 1e6
    print(f"Wrote lanes.bin.gz ({size:.2f} MB) + lanes.json", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
