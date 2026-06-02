#!/usr/bin/env python3
"""Build data/coastline.geojson from Natural Earth 1:10m physical land,
clipped to the maritime simulation's bounding box.

Run once after cloning the repo, and again whenever the bbox constants in
`crates/simulation/src/ais.rs` change.

Usage
-----
    python3 tools/build_coastline.py

Dependencies (one-time)
-----------------------
    pip install geopandas shapely pyogrio

The script downloads ~12 MB on first run.  The clipped + simplified output
file is typically 1–4 MB depending on simplification tolerance.

What the script does
--------------------
1. Streams Natural Earth `ne_10m_land.zip` directly from naciscdn.org.
2. Clips polygons to the bbox defined below (must match `ais.rs`).
3. Explodes MultiPolygons into one Feature per Polygon so the Rust loader
   does not need to walk multi-part geometries.
4. Simplifies each polygon at ~300 m tolerance (0.005°) — preserves every
   navigable strait the simulation cares about while keeping the vertex
   count tractable.
5. Writes `data/coastline.geojson` next to the existing `data/ais_paths.json`.
"""
from __future__ import annotations

import os
import sys


# ---------------------------------------------------------------------------
# Bounding box.  Must match the constants in
# crates/simulation/src/ais.rs (LAT_MIN, LAT_MAX, LON_MIN, LON_MAX).
# ---------------------------------------------------------------------------
LAT_MIN, LAT_MAX = 50.5, 66.0
LON_MIN, LON_MAX = -5.0, 31.0

# Simplification tolerance in degrees.  ~0.005° ≈ 300 m at this latitude.
# Increase (e.g. 0.01) for a coarser, smaller file; decrease (e.g. 0.002)
# for finer fjords and archipelagos at higher vertex count.
SIMPLIFY_TOLERANCE_DEG = 0.005

# Natural Earth 1:10m physical land.  Stable URL from naciscdn (the official
# Natural Earth CDN).  ~12 MB zipped.
NATURAL_EARTH_URL = (
    "https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip"
)

OUTPUT_PATH = "data/coastline.geojson"


def main() -> int:
    try:
        import geopandas as gpd  # type: ignore
        from shapely.geometry import box  # type: ignore
    except ImportError:
        sys.stderr.write(
            "Missing dependency.  Run:\n"
            "    pip install geopandas shapely pyogrio\n"
        )
        return 1

    print(f"Downloading Natural Earth 10m land from\n    {NATURAL_EARTH_URL}")
    gdf = gpd.read_file(NATURAL_EARTH_URL)
    print(f"  loaded {len(gdf)} features globally")

    bbox = box(LON_MIN, LAT_MIN, LON_MAX, LAT_MAX)
    print(
        f"Clipping to bbox lat[{LAT_MIN}..{LAT_MAX}] lon[{LON_MIN}..{LON_MAX}] ..."
    )
    clipped = gpd.clip(gdf, bbox)
    print(f"  {len(clipped)} features after clipping")

    # Drop empty geometries (clipping can produce some)
    clipped = clipped[~clipped.geometry.is_empty].copy()

    # Explode MultiPolygons so each Feature in the output is a single Polygon.
    exploded = clipped.explode(index_parts=False).reset_index(drop=True)
    print(f"  {len(exploded)} polygons after exploding multipolygons")

    # Simplify with topology preservation.  This is essential — without
    # preserve_topology=True, narrow straits can collapse into "land bridges".
    if SIMPLIFY_TOLERANCE_DEG > 0:
        exploded["geometry"] = exploded["geometry"].simplify(
            SIMPLIFY_TOLERANCE_DEG, preserve_topology=True
        )
        print(f"  simplified at tolerance {SIMPLIFY_TOLERANCE_DEG}°")

    # Strip everything but the geometry column — the Rust loader only needs polygons.
    out = exploded[["geometry"]].copy()

    # Make sure data/ exists, then write.
    os.makedirs("data", exist_ok=True)
    if os.path.exists(OUTPUT_PATH):
        os.remove(OUTPUT_PATH)
    out.to_file(OUTPUT_PATH, driver="GeoJSON")

    # Report vertex count for sanity.
    total_verts = 0
    for poly in exploded.geometry:
        if poly.is_empty:
            continue
        if poly.geom_type == "Polygon":
            total_verts += len(poly.exterior.coords)
            for hole in poly.interiors:
                total_verts += len(hole.coords)

    size_mb = os.path.getsize(OUTPUT_PATH) / 1024 / 1024
    print(
        f"\nWrote {OUTPUT_PATH}:\n"
        f"    {len(out):,} polygons\n"
        f"    {total_verts:,} vertices\n"
        f"    {size_mb:.2f} MB on disk"
    )

    return 0


if __name__ == "__main__":
    sys.exit(main())
