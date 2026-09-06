# Data and route generation

Runtime needs **no network**: the engine reads committed JSON/GeoJSON under [`data/`](../data/). Depth and lane grids feed **offline** `gen_routes` only.

## Bundled inputs

| File | Role |
|---|---|
| `ais_paths.json` | Vessel tracks (engine) |
| `ports.json` | Port polygons (engine) |
| `coastline.geojson` | Land mask |
| `bathymetry.bin.gz` + `.json` | EMODnet depth (router) |
| `lanes.bin.gz` + `.json` | EMODnet density (router) |

Treat `data/` as the **calibration baseline**.

## `crates/aisprocess`

| Piece | Role |
|---|---|
| [`bathymetry.rs`](../crates/aisprocess/src/bathymetry.rs) | Load depth grid |
| [`lanes.rs`](../crates/aisprocess/src/lanes.rs) | Load density field |
| [`ports.rs`](../crates/aisprocess/src/ports.rs) | Port gazetteer |
| [`router.rs`](../crates/aisprocess/src/router.rs) | Cost-field A\* (draft + UKC, clearance, lane bonus, jitter) |
| bin `gen_routes` | Emit `ais_paths.json` + `ports.json` |
| bin `aisprocess` | Legacy AIS CSV ETL |

```bash
cargo run -p aisprocess --bin gen_routes -- \
  --seed 42 --w-lane 2.5 --w-shallow 3.0 --jitter 0.18 \
  --out /tmp/ais_paths.json --ports-out /tmp/ports.json
```

> **Warning:** `gen_routes` defaults `--out` / `--ports-out` to committed `data/` paths. Always write to scratch until you intentionally adopt new routes.

## `tools/` (Python)

| Script | Output |
|---|---|
| [`fetch_bathymetry.py`](../tools/fetch_bathymetry.py) | `data/bathymetry.*` (EMODnet WCS) |
| [`fetch_lanes.py`](../tools/fetch_lanes.py) | `data/lanes.*` |
| [`build_coastline.py`](../tools/build_coastline.py) | `data/coastline.geojson` |
| [`fetch_ais_live.py`](../tools/fetch_ais_live.py) | Opt-in Digitraffic → `outputs/ais_live_paths.json` (`--adopt` overwrites baseline) |
| [`sweep.py`](../tools/sweep.py) | Parameter sweeps — see [sweeping.md](sweeping.md) |
| [`plot_results.py`](../tools/plot_results.py) | Report figures |
| [`waypoint_editor.html`](../tools/waypoint_editor.html) | Legacy manual waypoints |

Point a batch at a scratch network with:

```json
{ "ais_path": "/tmp/ais_paths.json", "ports_path": "/tmp/ports.json" }
```
