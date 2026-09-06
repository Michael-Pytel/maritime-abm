#!/usr/bin/env python3
"""Opt-in live AIS refresh from Digitraffic (Finnish Baltic, keyless).

Fetches the latest vessel locations (+ vessel metadata), filters to the
maritime-abm Baltic–North Sea bbox, maps ship types onto cargo/passenger/tanker,
and synthesises short OD polylines (current position → nearest port) in the
engine's `ais_paths.json` schema.

The offline `gen_routes` depth+lane generator remains the calibration default.
This script never overwrites `data/ais_paths.json` unless you pass `--adopt`.

Usage
-----
    python tools/fetch_ais_live.py                  # → outputs/ais_live_paths.json
    python tools/fetch_ais_live.py --out /tmp/x.json
    python tools/fetch_ais_live.py --adopt          # also copies to data/ais_paths.json
    python tools/fetch_ais_live.py --limit 80

Requires: network access. Stdlib only (urllib).
"""
from __future__ import annotations

import argparse
import json
import math
import shutil
import ssl
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCATIONS_URL = "https://meri.digitraffic.fi/api/ais/v1/locations"
VESSELS_URL = "https://meri.digitraffic.fi/api/ais/v1/vessels"
USER_AGENT = "maritime-abm/1.0 (education; fetch_ais_live)"

# Engine bbox (ais.rs)
LAT_MIN, LAT_MAX = 50.5, 66.0
LON_MIN, LON_MAX = -5.0, 31.0

# Digitraffic shipType codes → engine vessel_type (coarse bins).
# https://en.wikipedia.org/wiki/Automatic_identification_system#Message_types
TYPE_MAP = {
    range(60, 70): "passenger",
    range(70, 80): "cargo",
    range(80, 90): "tanker",
}


def _get_json(url: str) -> dict | list:
    req = urllib.request.Request(
        url,
        headers={
            "Digitraffic-User": USER_AGENT,
            "Accept": "application/json",
            "Accept-Encoding": "gzip",
            "Connection": "close",
        },
    )
    ctx = ssl.create_default_context()
    with urllib.request.urlopen(req, context=ctx, timeout=60) as resp:
        raw = resp.read()
        if resp.headers.get("Content-Encoding", "").lower() == "gzip" or raw[:2] == b"\x1f\x8b":
            import gzip

            raw = gzip.decompress(raw)
        return json.loads(raw.decode())


def _vessel_type(ship_type: int | None) -> str | None:
    if ship_type is None:
        return None
    for band, name in TYPE_MAP.items():
        if ship_type in band:
            return name
    return None


def _load_ports(path: Path) -> list[dict]:
    raw = json.loads(path.read_text())
    ports = []
    if isinstance(raw, list):
        for p in raw:
            name = p.get("name") or p.get("id") or "port"
            poly = p.get("keypoints") or p.get("polygon") or p.get("vertices") or []
            if poly:
                lat = sum(float(v["lat"]) for v in poly) / len(poly)
                lon = sum(float(v["lon"]) for v in poly) / len(poly)
            else:
                lat = float(p.get("lat", 0))
                lon = float(p.get("lon", 0))
            ports.append({"name": str(name), "lat": lat, "lon": lon})
    return ports


def _haversine_nm(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    r = 3440.065  # Earth radius in nm
    p1, p2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlmb = math.radians(lon2 - lon1)
    a = math.sin(dphi / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dlmb / 2) ** 2
    return 2 * r * math.asin(min(1.0, math.sqrt(a)))


def _nearest_port(lat: float, lon: float, ports: list[dict]) -> dict | None:
    if not ports:
        return None
    return min(ports, key=lambda p: _haversine_nm(lat, lon, p["lat"], p["lon"]))


def build_routes(limit: int, ports_path: Path) -> list[dict]:
    print(f"Fetching {LOCATIONS_URL} …")
    locations = _get_json(LOCATIONS_URL)
    print(f"Fetching {VESSELS_URL} …")
    vessels = _get_json(VESSELS_URL)

    meta: dict[int, dict] = {}
    if isinstance(vessels, list):
        for v in vessels:
            mmsi = v.get("mmsi")
            if mmsi is not None:
                meta[int(mmsi)] = v

    ports = _load_ports(ports_path)
    features = locations.get("features", []) if isinstance(locations, dict) else []
    routes: list[dict] = []

    for feat in features:
        props = feat.get("properties") or {}
        geom = feat.get("geometry") or {}
        coords = geom.get("coordinates") or []
        if len(coords) < 2:
            continue
        lon, lat = float(coords[0]), float(coords[1])
        if not (LAT_MIN <= lat <= LAT_MAX and LON_MIN <= lon <= LON_MAX):
            continue
        mmsi = int(props.get("mmsi") or feat.get("mmsi") or 0)
        if mmsi <= 0:
            continue
        info = meta.get(mmsi, {})
        vtype = _vessel_type(info.get("shipType") or info.get("shipTypeCode"))
        if vtype is None:
            continue
        dest = _nearest_port(lat, lon, ports)
        if dest is None:
            continue
        # Skip already-in-port noise (< 2 nm).
        if _haversine_nm(lat, lon, dest["lat"], dest["lon"]) < 2.0:
            continue
        name = (info.get("name") or f"MMSI-{mmsi}").strip() or f"MMSI-{mmsi}"
        routes.append({
            "mmsi": mmsi,
            "name": f"{name}->{dest['name']}",
            "vessel_type": vtype,
            "waypoints": [
                {"lat": lat, "lon": lon},
                {"lat": dest["lat"], "lon": dest["lon"]},
            ],
        })
        if len(routes) >= limit:
            break

    return routes


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=str(ROOT / "outputs" / "ais_live_paths.json"),
                    help="scratch output path (default: outputs/ais_live_paths.json)")
    ap.add_argument("--ports", default=str(ROOT / "data" / "ports.json"))
    ap.add_argument("--limit", type=int, default=80, help="max routes to emit")
    ap.add_argument("--adopt", action="store_true",
                    help="also copy output over data/ais_paths.json (calibration baseline)")
    args = ap.parse_args()

    routes = build_routes(args.limit, Path(args.ports))
    if not routes:
        raise SystemExit("no live vessels mapped into engine bbox / types — try again later")

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(routes, indent=2))
    print(f"Wrote {len(routes)} routes → {out}")

    by = {}
    for r in routes:
        by[r["vessel_type"]] = by.get(r["vessel_type"], 0) + 1
    print("  mix:", by)
    print("  Point batch at this file via config_override: "
          f'{{"ais_path": "{out}"}}')

    if args.adopt:
        dest = ROOT / "data" / "ais_paths.json"
        shutil.copy2(out, dest)
        print(f"ADOPTED → {dest}  (offline gen_routes remains the recommended default)")


if __name__ == "__main__":
    main()
