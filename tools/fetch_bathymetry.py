#!/usr/bin/env python3
"""Build data/bathymetry.* from EMODnet Digital Bathymetry, clipped to the
maritime simulation's bounding box.

The route generator needs real sea depth so large ships are kept in water deep
enough for their draft (+ under-keel clearance) and away from shoals / the
coast. EMODnet Bathymetry (DTM 2024, ~115 m native grid, free) covers the whole
Baltic + North Sea and exposes an OGC WCS that can return a plain-text grid,
which we can parse with numpy alone (no GDAL/rasterio).

Run once after cloning (and again if the bbox in `crates/simulation/src/ais.rs`
changes):

    python3 tools/fetch_bathymetry.py

Dependencies (one-time): pip install numpy requests

Outputs (committed, ~1-3 MB gzipped):
    data/bathymetry.bin.gz  little-endian int16 depth grid, +down = below sea
                            level in metres; land/above-sea stored as +metres.
                            Row-major, row 0 = SOUTH (lat ascending), col 0 = WEST.
    data/bathymetry.json    grid header: bbox, dims, cell steps, nodata sentinel.

Source: EMODnet Bathymetry — https://emodnet.ec.europa.eu/en/bathymetry
"""
from __future__ import annotations

import gzip
import io
import json
import re
import sys
import time
from pathlib import Path

import numpy as np
import requests

# ── bbox: MUST match crates/simulation/src/ais.rs ────────────────────────────
LAT_MIN, LAT_MAX = 50.5, 66.0
LON_MIN, LON_MAX = -5.0, 31.0

# ── output grid resolution (degrees) ─────────────────────────────────────────
# ~0.6 nm in lat, ~0.55 nm in lon at 58°N — fine enough to resolve the Danish
# straits and archipelago channels the routing cares about.
DLAT = 0.010
DLON = 0.018

# ── EMODnet WCS ──────────────────────────────────────────────────────────────
WCS = "https://ows.emodnet-bathymetry.eu/wcs"
COVERAGE = "emodnet__mean"
SCALE_FACTOR = 0.12  # native ~0.00104° -> ~0.0087° per fetched cell
# The WCS caps each request's *native* read at ~98 MB regardless of scaling,
# so tiles must stay under ~14 deg^2 of native data. 3x4 deg = 12 deg^2 is safe.
TILE_LAT, TILE_LON = 3.0, 4.0
NODATA = -32768
TIMEOUT = 120
RETRIES = 3

OUT_DIR = Path(__file__).resolve().parent.parent / "data"


def fetch_tile(lat0: float, lat1: float, lon0: float, lon1: float) -> str:
    params = {
        "service": "WCS",
        "version": "2.0.1",
        "request": "GetCoverage",
        "coverageId": COVERAGE,
        "subset": [f"Lat({lat0},{lat1})", f"Long({lon0},{lon1})"],
        "scaleFactor": SCALE_FACTOR,
        "format": "text/plain",
    }
    last = None
    for attempt in range(RETRIES):
        try:
            r = requests.get(WCS, params=params, timeout=TIMEOUT)
            if r.status_code == 200 and "ExceptionReport" not in r.text[:400]:
                return r.text
            last = f"HTTP {r.status_code}: {r.text[:200]}"
        except requests.RequestException as e:  # noqa: PERF203
            last = str(e)
        time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"tile ({lat0},{lon0})-({lat1},{lon1}) failed: {last}")


def parse_tile(text: str):
    """Returns (values[rows,cols], lat_centers[rows asc], lon_centers[cols asc])."""
    dims = re.search(r"GridEnvelope2D\[0\.\.(\d+),\s*0\.\.(\d+)\]", text)
    bounds = re.search(
        r"GeneralBounds\[\(([-\d.]+),\s*([-\d.]+)\),\s*\(([-\d.]+),\s*([-\d.]+)\)\]",
        text,
    )
    if not dims or not bounds:
        raise ValueError("could not parse WCS text header")
    cols, rows = int(dims.group(1)) + 1, int(dims.group(2)) + 1
    lon0, lat0, lon1, lat1 = (float(bounds.group(i)) for i in (1, 2, 3, 4))

    nums = re.findall(r"-?\d+\.\d+(?:[eE][-+]?\d+)?|-?\d+", text)
    vals = np.asarray(nums[-rows * cols :], dtype=np.float64).reshape(rows, cols)
    # WCS image order: row 0 = NORTH. Flip so row 0 = SOUTH (lat ascending).
    vals = vals[::-1, :]

    # Cell centres.
    lat_c = lat0 + (np.arange(rows) + 0.5) * (lat1 - lat0) / rows
    lon_c = lon0 + (np.arange(cols) + 0.5) * (lon1 - lon0) / cols
    return vals, lat_c, lon_c


def main() -> int:
    nlat = round((LAT_MAX - LAT_MIN) / DLAT)
    nlon = round((LON_MAX - LON_MIN) / DLON)
    print(f"Output grid: {nlat} lat x {nlon} lon = {nlat * nlon:,} cells", flush=True)

    grid = np.full((nlat, nlon), np.nan, dtype=np.float64)
    glat = LAT_MIN + (np.arange(nlat) + 0.5) * DLAT
    glon = LON_MIN + (np.arange(nlon) + 0.5) * DLON

    lat_edges = np.arange(LAT_MIN, LAT_MAX + 1e-9, TILE_LAT)
    lon_edges = np.arange(LON_MIN, LON_MAX + 1e-9, TILE_LON)
    tiles = [
        (a, min(a + TILE_LAT, LAT_MAX), o, min(o + TILE_LON, LON_MAX))
        for a in lat_edges
        if a < LAT_MAX
        for o in lon_edges
        if o < LON_MAX
    ]
    print(f"Fetching {len(tiles)} tiles from EMODnet WCS …", flush=True)

    for i, (la0, la1, lo0, lo1) in enumerate(tiles, 1):
        t0 = time.time()
        vals, lat_c, lon_c = parse_tile(fetch_tile(la0, la1, lo0, lo1))
        # Nearest-neighbour resample onto the global cells that fall in the tile.
        rmask = (glat >= lat_c[0]) & (glat <= lat_c[-1])
        cmask = (glon >= lon_c[0]) & (glon <= lon_c[-1])
        if rmask.any() and cmask.any():
            ri = np.searchsorted(lat_c, glat[rmask]).clip(0, len(lat_c) - 1)
            ci = np.searchsorted(lon_c, glon[cmask]).clip(0, len(lon_c) - 1)
            block = vals[np.ix_(ri, ci)]
            grid[np.ix_(np.where(rmask)[0], np.where(cmask)[0])] = block
        sea = np.mean(vals < 0) * 100
        print(
            f"  [{i}/{len(tiles)}] Lat({la0:.0f},{la1:.0f}) Lon({lo0:.0f},{lo1:.0f}) "
            f"{vals.shape[0]}x{vals.shape[1]}  {sea:.0f}% sea  {time.time() - t0:.1f}s",
            flush=True,
        )

    print(f"Filled {np.isfinite(grid).mean() * 100:.1f}% of the global grid", flush=True)
    # Fill thin tile-seam gaps from finite neighbours so routing sees no
    # spurious land barriers along tile boundaries.
    for _ in range(8):
        holes = ~np.isfinite(grid)
        if not holes.any():
            break
        vals = np.where(np.isfinite(grid), grid, 0.0)
        cnt = np.isfinite(grid).astype(np.float64)
        acc = np.zeros_like(grid)
        nb = np.zeros_like(grid)
        for dr, dc in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            acc += np.roll(vals, (dr, dc), (0, 1))
            nb += np.roll(cnt, (dr, dc), (0, 1))
        fillable = holes & (nb > 0)
        grid[fillable] = acc[fillable] / nb[fillable]
    grid[~np.isfinite(grid)] = 0.0  # any isolated leftover -> land (safe)

    depth_i16 = np.clip(np.round(grid), NODATA + 1, 32767).astype("<i2")
    OUT_DIR.mkdir(exist_ok=True)
    bin_path = OUT_DIR / "bathymetry.bin.gz"
    with gzip.open(bin_path, "wb") as f:
        f.write(depth_i16.tobytes(order="C"))
    meta = {
        "lat_min": LAT_MIN,
        "lon_min": LON_MIN,
        "dlat": DLAT,
        "dlon": DLON,
        "nlat": nlat,
        "nlon": nlon,
        "nodata": NODATA,
        "order": "row-major, row0=south (lat asc), col0=west (lon asc)",
        "units": "metres, negative = below sea level",
        "source": "EMODnet Digital Bathymetry (DTM), https://emodnet.ec.europa.eu/en/bathymetry",
    }
    (OUT_DIR / "bathymetry.json").write_text(json.dumps(meta, indent=2))

    sea = grid < 0
    print(
        f"Wrote {bin_path.name} ({bin_path.stat().st_size / 1e6:.1f} MB) + bathymetry.json\n"
        f"  sea cells: {sea.mean() * 100:.0f}%   "
        f"depth range: {grid[sea].min():.0f}..{grid[sea].max():.0f} m",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
