# Maritime ABM — Developer Manual & Handoff Guide

A tick-based **agent-based model of maritime traffic safety** on the real Baltic and
North Sea, built to quantify a causal chain from *environmental + human degradation*
to *collision and survival outcomes*. Vessels sail depth- and lane-constrained routes,
resolve encounters with a CPA/COLREGs give-way rule whose success is gated by weather
severity × crew fatigue, and — when things go wrong — feed an accredited collision-damage
model (DTU SIMCOL) and a search-and-rescue chain (IAMSAR/MASSIM + Ashrafi seasonal
degradation). Results are compared across scenarios and methods with non-parametric
hypothesis tests.

The engine is **Rust** (on the [krABMaga](https://krabmaga.github.io/) ABM framework) for
deterministic, data-parallel batch execution; the UI is a **React 19 + deck.gl** playback
workbench; the academic write-up lives in [report/](report/).

> **New here? Read in this order:** [What this actually simulates](#1-what-this-actually-simulates)
> → [The tick pipeline](#43-the-tick-pipeline-the-heart-of-the-engine) → [Getting started](#9-getting-started)
> → [Hyperparameter sweeping](#6-hyperparameter-sweeping) → [TODO / roadmap](#12-todo--roadmap-start-here). The roadmap is where the open work is.

---

## Table of contents

1. [What this actually simulates](#1-what-this-actually-simulates)
2. [Repository layout](#2-repository-layout)
3. [Architecture & data flow](#3-architecture--data-flow)
4. [The simulation engine (`crates/simulation`)](#4-the-simulation-engine-cratessimulation)
5. [The API server (`crates/api`)](#5-the-api-server-cratesapi)
6. [Hyperparameter sweeping — **find better parameters from the JSONs**](#6-hyperparameter-sweeping)
7. [Route generation & data tooling (`crates/aisprocess`, `tools/`, `data/`)](#7-route-generation--data-tooling)
8. [The frontend (`frontend/`)](#8-the-frontend-frontend)
9. [Getting started](#9-getting-started)
10. [Command cheat-sheet](#10-command-cheat-sheet)
11. [Conventions & gotchas](#11-conventions--gotchas-read-before-you-touch-things)
12. [TODO / roadmap — **start here**](#12-todo--roadmap-start-here)
13. [Glossary & scientific references](#13-glossary--scientific-references)

---

## 1. What this actually simulates

The experiment is a **factorial design**: 4 scenarios × 3 methods × N seeds, each run for
`n_ticks` ticks where **1 tick = 15 minutes** (default 2880 ticks = **30 simulated days**).

**Scenarios** (defined in [crates/simulation/src/scenario.rs](crates/simulation/src/scenario.rs), `SimConfig::for_scenario`):

| Scenario | Vessels | The stressor |
|---|---|---|
| `CalmPassage` | 20 | Baseline — calm weather, no storm. The calibration reference. |
| `StormCorridor` | 25 | A **moving storm zone** tracks Channel → North Sea → Skagerrak → Gdańsk; inside it comms drop to ~8 % success and speed to 45 %. |
| `BlindShore` | 25 | Fleet-wide degraded comms (0.55 success) + tighter avoidance berth — patchy VHF/AIS away from dense lanes. |
| `DeepWaterRescue` | 15 | Sparse fleet, distant SAR bases — slow assets + longer mobilisation stress the rescue chain against the fixed survival window. |

**Methods** (the thing under test):

| Method | What it is |
|---|---|
| `BaselineA` | Classical COLREGs. No weather *awareness* → no behavioural mitigation (does **not** slow in the storm). Still suffers environmental comms loss. |
| `BaselineB` | Weather-aware but shore-only comms (no offshore mesh) → blind far from shore. |
| `ProposedSystem` | Full system: fatigue-aware watch scheduling + coordinated avoidance + functioning rescue chain. |

**Hypotheses** (from the report abstract / [report.tex](report/report.tex) §Experiment):

- **H1** — Fatigue-aware scheduling reduces the fatal-event rate by **≥20 %** vs Baseline A (Mann–Whitney U, p<0.05, |δ|≥0.2).
- **H2** — Better avoidance + working rescue improves post-incident survival by **≥15 %** vs Baseline A.
- **H3** — Proposed beats Baseline B on the fatal-event rate **under the storm corridor**, where comms degradation is worst.

The core scientific claim is the **coupling**: the success probability of every
collision-warning exchange is `environmental_factor × crew_fatigue_factor`, so a tired
watch-keeper in heavy weather misses the warning and collides. Everything else
(consequence model, SAR, KPIs) exists to turn that into measurable survival outcomes.

---

## 2. Repository layout

```
maritime-abm/
├── crates/                       # Rust workspace (see Cargo.toml)
│   ├── simulation/               #   the ABM engine — all the physics & science
│   ├── api/                      #   axum HTTP + WebSocket server, batch runner, stats
│   └── aisprocess/               #   route-data tooling (depth/lane router + gen_routes)
├── frontend/                     # React 19 + Vite + deck.gl playback workbench
├── data/                         # bundled, COMMITTED inputs the engine reads
│   ├── ais_paths.json            #   generated vessel routes  (engine input)
│   ├── ports.json                #   generated ports          (engine input)
│   ├── bathymetry.bin.gz + .json #   EMODnet sea-depth grid   (router input)
│   ├── lanes.bin.gz + .json      #   EMODnet vessel-density   (router input)
│   └── coastline.geojson         #   land mask (grounding)
├── tools/                        # Python: data fetchers + sweep driver
├── report/                       # LaTeX academic paper (+ build.ps1)
├── outputs/                      # batch artifacts — GITIGNORED, latest-batch-only
├── citings/                      # source PDFs for accredited models — GITIGNORED
├── Cargo.toml / Cargo.lock       # workspace manifest + shared deps
├── Makefile                      # dev / build / check / fmt shortcuts
└── .github/workflows/ci.yml      # CI: fmt + clippy + frontend build/lint
```

**Committed vs generated:** `data/` is committed and is the calibration baseline —
treat it as source of truth. `outputs/` and `citings/` are gitignored. Rust `target/`,
`node_modules/`, and `frontend/dist/` are gitignored build output.

---

## 3. Architecture & data flow

```
                 ┌─────────────────────────────────────────────────────────┐
   tools/*.py    │  aisprocess: gen_routes (depth+lane A*)                   │
  (EMODnet WCS)  │    bathymetry.bin.gz + lanes.bin.gz  ─►  ais_paths.json   │
       │         │                                          ports.json       │
       ▼         └───────────────────────────────┬─────────────────────────┘
   data/*.bin.gz                                 │  (engine inputs)
                                                 ▼
        ┌────────────────────────────────────────────────────────────────┐
        │  crates/simulation : SimStateWrapper.run_blocking()             │
        │    per-tick pipeline → KpiAccumulator → snapshots               │
        └───────────────┬───────────────────────────────┬────────────────┘
                        │ (live, per-run)                │ (batch, ×rayon)
                        ▼                                 ▼
        crates/api                            crates/api : spawn_batch
        WebSocket /ws  (live snapshots)         outputs/<run>.jsonl   (playback, latest only)
        REST /sim/*    (control + KPI)          outputs/<run>_manifest.json
                        │                        outputs/results.db    (SQLite, RETAINED)
                        │                        outputs/summary.csv
                        ▼                                 │
        ┌────────────────────────────────────────────────▼───────────────┐
        │  frontend (deck.gl) :  batch launcher → run grid → scrub .jsonl │
        │    /sim/batch/stats → Mann–Whitney + Holm–Bonferroni + Cliff's δ │
        └─────────────────────────────────────────────────────────────────┘
```

Two execution paths share **one** tick body (`SimState::post_tick`):

- **Live** — `POST /sim/start` spawns a single run; snapshots stream over WebSocket `/ws`.
  *Note: the current frontend is batch-first and does **not** consume the live path — see
  [TODO](#12-todo--roadmap-start-here).*
- **Batch** — `POST /sim/batch` fans out `scenarios × methods × seeds` across a Rayon pool,
  writing a JSONL tick-log + manifest per run, one durable KPI row per run into SQLite, and
  a `summary.csv`. This is the experiment path.

---

## 4. The simulation engine (`crates/simulation`)

The heart of the project. `src/lib.rs` just re-exports the modules; the real work:

| Module | Responsibility |
|---|---|
| [state.rs](crates/simulation/src/state.rs) (~2400 ln) | **The orchestrator.** `SimState` + `SimStateWrapper` (krABMaga `State` impl), the single `post_tick` pipeline, hard-collision detection, SAR/MOB dispatch, fleet top-up, snapshot building, JSONL logging, WebSocket broadcast. `run_collision_avoidance` (the CPA/COLREGs give-way logic) lives here too. |
| [scenario.rs](crates/simulation/src/scenario.rs) | `Scenario` & `Method` enums + **`SimConfig`** — the single, serde-serialisable struct holding *every* tunable knob (fleet size, radii, comms rates, storm track, nested `simcol`/`sar`/`forcefield` params). `for_scenario()` is the factory that bakes scenario+method presets. **Start here to change experiment parameters.** |
| [vessel.rs](crates/simulation/src/vessel.rs) | `VesselAgent`, the `VesselState` FSM (`Active → Docked / Evac → Rescued / Lost`), route navigation, docking, and the **crew fatigue model** (base + weather + circadian accumulation, recovery in port, comms penalty above a threshold). Ship dimensions & typical speeds by type. |
| [weather.rs](crates/simulation/src/weather.rs) | The 50×50 **stochastic hazard field** `W ∈ [0,1]`, `WeatherPreset`, `hazard_at()`, and downsampling for lightweight logs. |
| [comms.rs](crates/simulation/src/comms.rs) | `VesselMsg` / `MsgKind` (collision_warning, giving_way, keep_distance, …) and the rolling comms log surfaced in snapshots. |
| [simcol.rs](crates/simulation/src/simcol.rs) | **DTU SIMCOL** collision-consequence model: reduced-mass oblique-impact energy (Pedersen–Zhang), Minorsky penetration, and a **SOLAS-shaped survival surrogate** `S_i`. Also `classify_collision()` → head-on / front-to-side / side-to-side. ⚠ The `S_i` knobs are the project's only remaining `TODO-VERIFY` (deliberate surrogate). |
| [sar.rs](crates/simulation/src/sar.rs) | The **search-and-rescue chain**: `RescueAgent` (helo/patrol, mobilise→transit→search→embark phases), MASSIM/IAMSAR **Koopman random-search detection**, plus the **man-overboard** path (`MobPerson` per-person datums + `MobSearchAgent`). `SarParams` in `SimConfig`. |
| [ashrafi.rs](crates/simulation/src/ashrafi.rs) | **Ashrafi (2024)** month-group seasonal SAR degradation — scales transit/search speed & boarding time by calendar month (`sim_month`). |
| [iwrap.rs](crates/simulation/src/iwrap.rs) | **IWRAP Mk II** collision-frequency estimator — an *offline validator* that cross-checks the emergent collision rate against the regulator-grade model. Driven by the `iwrap` bin, not wired into per-run KPIs. |
| [forcefield.rs](crates/simulation/src/forcefield.rs) | Optional **Xiao (2013) artificial force-field** steering (repulsion + Nomoto rudder dynamics). Default **off**; when off, behaviour is byte-identical to the CPA track (determinism guard). |
| [land.rs](crates/simulation/src/land.rs) | `LandMask` from `coastline.geojson` — grounding: a vessel that steps onto land is reverted to its last valid position. |
| [kpi.rs](crates/simulation/src/kpi.rs) | `KpiAccumulator` (running totals) → `KpiSnapshot` (the 6 hypothesis KPIs) + diagnostic counters (collision-type breakdown, MOB tallies, lost/rescued cumulative). |
| [ais.rs](crates/simulation/src/ais.rs) | Route & port loading, and the `lat/lon ↔ field-coordinate` transforms + world dimensions used everywhere. |
| [bin/iwrap.rs](crates/simulation/src/bin/iwrap.rs) | CLI: loads the route network, builds multi-class traffic legs, prints per-year collision estimates + a `<1/yr` verdict. |

### 4.1 The vessel state machine

```
        spawn ─► Active ◄──────────► Docked         (dock dwell, crew recovers fatigue)
                   │
                   │ struck & founders            │ struck, stays afloat
                   ▼                              ▼
                 Evac  ──(SAR in time)──► Rescued   casualties → persons-in-water (MOB)
                   │                        │              │
                   └──(window elapses)──► Lost  ◄──────────┘ (not recovered in time = fatality)
                                            │
                              Rescued & Lost are terminal → reaped → fleet tops back up
```

A **fatality is emergent**, not a dice roll: it occurs only when a liferaft occupant or a
person-in-water is not recovered before their survival window elapses.

### 4.2 KPIs (`KpiSnapshot`)

| Field | Meaning | Hypothesis |
|---|---|---|
| `fatal_per_1k_hrs` | fatalities per 1000 ship-hours | H1, H3 |
| `collision_per_1k_hrs` | hard collisions per 1000 ship-hours | (calibration) |
| `survival_ratio` | crew surviving exposure | H2 |
| `avg_tta_hours` | mean rescue time-to-arrival | H2 |
| `evac_activation_rate` | fraction of runs triggering evac | H1 |
| `mean_p_prep` | mean crew preparedness (awareness×fatigue) | H1 |

Ship-hours accrue 0.25 h per **Active** vessel per tick. Diagnostic-only counters
(collision geometry breakdown, MOB in-water/recovered/lost) ride in the snapshot
`telemetry` block, not in `KpiSnapshot`.

### 4.3 The tick pipeline (the heart of the engine)

Every tick runs through **one** function — `SimState::post_tick` in
[state.rs](crates/simulation/src/state.rs) — so both the live and batch paths behave
identically. Order matters; this is the sequence:

1. **Weather** field advances one step.
2. **Grounding** — any vessel now on land reverts to its last valid position.
3. **Hazard-gated speed reduction + fatigue** — `v_eff = v · max(0.30, 1 − 0.45·W)` (Baseline B & Proposed only); every vessel accrues fatigue from local hazard + time-of-day.
4. **Storm dampening** — vessels inside the storm zone are slowed by `storm_speed_factor`.
5. **ResumeRoute** comms logged for vessels finishing an avoidance manoeuvre.
6. **Collision avoidance** — `apply_force_field` (if `forcefield.enabled`) **or** `run_collision_avoidance` (CPA/TCPA detection → COLREGs give-way → starboard waypoint), gated by weather × fatigue comms success.
7. **Avoidance cooldown** counts down.
8. **Same-destination following** — a trailing vessel bound for the same port matches its leader's speed / holds station instead of overtaking (kills funnel-to-port false collisions).
9. **Hard-collision detection** — **rising-edge** per encounter (counted once when a pair *enters* the 0.10 nm contact radius, excluding VTS harbour water): classify geometry → `SIMCOL::evaluate` → founder ⇒ `Evac`, else casualties ⇒ man-overboard incident.
10. **`run_sar`** — dispatch/advance/resolve rescue assets (`Evac → Rescued / Lost`).
11. **`run_mob_sar`** — recover or lose persons in the water.
12. **`reap_terminal_vessels`** — remove `Rescued`/`Lost`; fleet top-up respawns replacements.
13. **KPI accumulation** (tick totals + `p_prep`).
14. **Snapshot** built → JSONL log (batch) and/or WebSocket broadcast (live).

**Determinism is a hard invariant.** All randomness flows through one seeded `SmallRng`
threaded through `SimState`. Same seed ⇒ byte-identical KPIs (guarded by
`test_determinism_same_seed`). Anything you add that draws randomness must use `self.rng`.

### 4.4 Tests

- [tests/full_run.rs](crates/simulation/tests/full_run.rs) — smoke runs, **determinism**, seed-variation, fleet-size invariants.
- [tests/calibration.rs](crates/simulation/tests/calibration.rs) — the report's §5.2 target: CalmPassage/BaselineA mean `collision_per_1k_hrs < 0.5` (the slow 5-seed×2880 version is `#[ignore]`; run with `--include-ignored`).
- [tests/unit_p_raft.rs](crates/simulation/tests/unit_p_raft.rs) — liferaft/survival unit checks.
- Plus module unit tests inside `simcol.rs`, `sar.rs`, `iwrap.rs`, `forcefield.rs`, `ashrafi.rs`.

---

## 5. The API server (`crates/api`)

Axum + Tokio. Binds `0.0.0.0:3000`, permissive CORS (dev). Modules:

| Module | Role |
|---|---|
| [main.rs](crates/api/src/main.rs) | Router wiring (the endpoint table below). |
| [routes.rs](crates/api/src/routes.rs) | HTTP handlers + `AppState` (live handle + active batch). |
| [runner.rs](crates/api/src/runner.rs) | `spawn_sim` (one live run) and `spawn_batch` (Rayon-parallel sweep; writes manifests, CSV, SQLite rows; deep-merges config overrides). |
| [stats.rs](crates/api/src/stats.rs) | **Mann–Whitney U + Holm–Bonferroni correction + Cliff's δ** over `BatchResult`s → `HypothesisResult`s. |
| [db.rs](crates/api/src/db.rs) | SQLite results store (the *retained* tier, keyed by `batch_id`), `deep_merge` for sweep overrides, Parquet export, sweep-label derivation. |
| [ws.rs](crates/api/src/ws.rs) | WebSocket handler streaming live snapshots. |

### Endpoints

| Method + path | Purpose |
|---|---|
| `GET /health` | liveness |
| `POST /sim/start` · `POST /sim/stop` · `GET /sim/kpi` | live single-run control (scenario/method/seed/ticks/vessels/loop) |
| `GET /ws` | live snapshot stream |
| `POST /sim/batch` | launch a sweep (`n_seeds`, `n_ticks`, `scenarios[]`, `methods[]`, `config_override`) |
| `GET /sim/batch/status` | progress (total/completed/failed/running/pct) |
| `GET /sim/batch/results` · `GET /sim/batch/stats` · `GET /sim/batch/hypothesis` | raw results & hypothesis tests |
| `GET /sim/batch/export.parquet?all=` | Parquet export (this batch or the whole sweep) |
| `GET /sim/ais-paths` | the route network (for the map's faint AIS layer) |
| `GET /sim/runs` | list completed runs (from `outputs/*_manifest.json`, newest first) |
| `GET /sim/runs/{id}/log` · `GET /sim/runs/{id}/manifest` | a run's JSONL tick-log / manifest (playback) |

**Two storage tiers (by design):** playback `.jsonl` logs + manifests are **latest-batch-only**
(each batch wipes the previous batch's logs so disk doesn't balloon ~GB/sweep). The **results
DB (SQLite)** and any exported Parquet are **append-only / retained** for cross-batch comparison.
`config_override` is deep-merged over scenario defaults, but experiment identity
(scenario/method/seed) is always re-pinned and can't be overridden.

---

## 6. Hyperparameter sweeping

**This is the intended way to improve the model.** The whole batch machinery exists so you can
**search parameter space and read outcomes back out of the result artifacts** — the JSON manifests,
`sweep_summary.csv`, and `sweep_all.parquet` — instead of guessing. A *sweep* runs the experiment
once per parameter combination and lands a **parameter → KPI** table you sort to pick winners.

**Define "better" first.** Two objectives drive tuning right now (both open — see [§12 TODO](#12-todo--roadmap-start-here)):

- **Calibration realism** — reproduce real-AIS collision density (`CalmPassage` ≈ 1.0 collisions/1k-hrs) *and* restore the storm/calm **contrast** (real ≈ 2.7×, currently ≈ 1.3×).
- **Experiment signal** — maximise the effect size by which `ProposedSystem` beats the baselines on `fatal_per_1k_hrs` / `survival_ratio` (H1–H3), read from `GET /sim/batch/stats` (Mann–Whitney U + Holm–Bonferroni + Cliff's δ).

### 6.1 Two tiers of knobs

| Tier | Where it lives | How to sweep | Examples |
|---|---|---|---|
| **1 — `SimConfig`** | live, no rebuild | batch `config_override` (deep-merged), driven by [tools/sweep.py](tools/sweep.py) | `n_vessels`, `comms_success_rate`, `storm_comms_success_rate`, `storm_radius_nm`, `storm_speed_factor`, `follow_vicinity_nm`, `collision_warn_cpa_nm`, `simcol.*`, `sar.*`, `forcefield.enabled` |
| **2 — route / lane structure** | baked into `data/*.json` by `gen_routes` | regenerate to a scratch JSON per variant, then point the sweep at it via the `ais_path` field | fleet mix `--n-cargo/--n-passenger/--n-tanker`, `--grid-nm`, `--clearance-nm`, `--seed`, lanes on/off `--no-lanes`, router weights `w_lane`/`w_shallow`/`jitter` |

Anything in [`SimConfig`](crates/simulation/src/scenario.rs) is Tier 1. **Lanes and route shape are
Tier 2** — they're properties of the generated *network*, not of a run, so they need a fresh
`data/ais_paths.json` from the [generator](#7-route-generation--data-tooling).

### 6.2 Tier 1 — sweep `SimConfig` with `tools/sweep.py`

The driver already exists. Start the API, edit the CONFIG block, run it:

```python
# tools/sweep.py  — edit the CONFIG block
SCENARIOS = ["CalmPassage", "StormCorridor"]
METHODS   = ["ProposedSystem", "BaselineA"]
N_SEEDS   = 30
N_TICKS   = 2880
SWEEP = {                       # dotted SimConfig path → values; full cartesian product
    "n_vessels":                [15, 20, 25, 35],
    "storm_comms_success_rate": [0.04, 0.08, 0.15],
}
```

```bash
cargo run -p api --release          # terminal 1
python tools/sweep.py --dry-run     # preview the grid (prints each override JSON)
python tools/sweep.py --fresh       # run it (wipes results.db → export = exactly this sweep)
```

It runs one batch per grid point and writes to `outputs/`:

- **`sweep_summary.csv`** — one row per *(sweep point × scenario × method)* with the **mean of every KPI**. This is the parameter-vs-outcome table: sort by your objective (`collision_per_1k_hrs`, `fatal_per_1k_hrs`, …) to read off the best setting.
- **`sweep_all.parquet`** — raw per-run rows (label, params, all KPIs) for stats in polars/pandas/R.
- The **results DB** retains every point across sweeps; `GET /sim/batch/stats` gives the significance tests per point.

### 6.3 Tier 2 — sweep routes & lanes (the JSONs)

Because the network is baked into `data/ais_paths.json`, sweep it by **generating one JSON per
variant to a scratch path, then overriding `ais_path`** — non-destructively, without touching the
committed baseline:

```bash
# 1. Generate route variants to scratch. Ports are identical across fleet/lane variants
#    at fixed --grid-nm/--clearance-nm, so share ONE ports.json and vary only the routes.
gen() { cargo run -q -p aisprocess --bin gen_routes -- "$@"; }
gen --n-cargo 20 --n-passenger 15 --n-tanker 10 --out /tmp/routes_sparse.json --ports-out /tmp/ports.json
gen --n-cargo 50 --n-passenger 35 --n-tanker 25 --out /tmp/routes_dense.json  --ports-out /tmp/ports.json
gen --no-lanes                                   --out /tmp/routes_nolane.json --ports-out /tmp/ports.json
```

```python
# 2. tools/sweep.py — sweep the network as an `ais_path` axis (crossable with Tier-1 knobs):
SWEEP = {
    "ais_path":   ["/tmp/routes_sparse.json", "/tmp/routes_dense.json", "/tmp/routes_nolane.json"],
    "ports_path": ["/tmp/ports.json"],       # shared → keep as a single-value axis
    "n_vessels":  [20, 30],
}
```

`ais_path` / `ports_path` are ordinary `SimConfig` fields, so the batch's `config_override` honours
them — they are **not** re-pinned like scenario/method/seed (verified in
[runner.rs](crates/api/src/runner.rs)). Read the winners from `sweep_summary.csv` exactly as in Tier 1.

**Current limitations (small tooling gaps — tracked in [§12 TODO](#12-todo--roadmap-start-here)):**

- Router cost weights (`w_lane`, `w_shallow`, `jitter`, …) are `RouterParams` defaults **not yet exposed as `gen_routes` CLI flags** — to sweep them today, edit `RouterParams::default()` in [router.rs](crates/aisprocess/src/router.rs) and rebuild, or add the flags (a quick change following the existing `--grid-nm` pattern).
- `sweep.py` takes the **cartesian product of independent axes**, so a *matched* route+ports pair can't be a single coupled axis — keep ports shared (as above), or add a zipped/variant-list mode.
- "Better" is currently read by eye from `sweep_summary.csv`; there's no automatic objective score to rank points yet.

---

## 7. Route generation & data tooling

Vessel routes used to be manual (multi-GB Danish AIS CSV dumps → ETL → JSON). They are now
**procedurally generated, offline, and depth-aware** — no downloaded data or network at run
time. `crates/aisprocess` is lib + two bins.

**`crates/aisprocess/src`:**

| Piece | Role |
|---|---|
| [bathymetry.rs](crates/aisprocess/src/bathymetry.rs) | Loads the EMODnet sea-depth grid (`data/bathymetry.bin.gz`). |
| [lanes.rs](crates/aisprocess/src/lanes.rs) | Loads the EMODnet vessel-density "lane attractiveness" field (`data/lanes.bin.gz`). |
| [ports.rs](crates/aisprocess/src/ports.rs) | Built-in major-port gazetteer for the sim bbox. |
| [router.rs](crates/aisprocess/src/router.rs) | **Cost-field weighted A\*** per vessel class: draft + under-keel clearance make shallow cells impassable; cost = distance + shallow-margin + shore-clearance − lane-bonus. RDP simplification + a raw-bathymetry reject pass guarantees no route sample sits below draft+UKC. Seeded jitter = deterministic-but-varied tracks. |
| [gen_routes.rs](crates/aisprocess/src/gen_routes.rs) (bin `gen_routes`) | Composes the above into `data/ais_paths.json` + `data/ports.json`. |
| [main.rs](crates/aisprocess/src/main.rs) (bin `aisprocess`) | Legacy AIS CSV ETL (kept for reference). |

**`tools/` (Python — stdlib + numpy/requests only, no gdal/rasterio):**

| Script | Produces |
|---|---|
| [fetch_bathymetry.py](tools/fetch_bathymetry.py) | `data/bathymetry.*` from EMODnet DTM via keyless WCS |
| [fetch_lanes.py](tools/fetch_lanes.py) | `data/lanes.*` from EMODnet vessel-density (2023) via keyless WCS |
| [build_coastline.py](tools/build_coastline.py) | `data/coastline.geojson` land mask |
| [sweep.py](tools/sweep.py) | Scripted hyperparameter sweep over the batch API → `outputs/sweep_summary.csv` + `sweep_all.parquet` |
| [waypoint_editor.html](tools/waypoint_editor.html) | Manual waypoint editor (legacy helper) |

> ⚠ **`gen_routes` defaults its `--out`/`--ports-out` to the committed `data/` files.** An
> early run without those flags once silently overwrote the calibration baseline. **Always
> point `--out`/`--ports-out` at a scratch path** until you've decided to adopt new routes.

---

## 8. The frontend (`frontend/`)

React 19 + Vite + TypeScript. Map = **deck.gl** over a keyless CARTO-dark **MapLibre** basemap;
charts = **recharts**. It is a **batch-first playback workbench** — you launch runs, watch the
grid fill, pick a run, and scrub its tick-log. (Leaflet was removed; the live-WS path is *not*
currently wired into the UI.)

**Shell** ([App.tsx](frontend/src/App.tsx)) = a permanent **left panel** beside a full-bleed map:

| Area | File | Role |
|---|---|---|
| Left panel + tab strip | [components/LeftPanel.tsx](frontend/src/components/LeftPanel.tsx) | Tabs: **Runs / Statistics / Telemetry / Comms**, plus a pinned playback dock. Charts lazy-load. |
| Runs launcher + grid | [components/RunsPanel.tsx](frontend/src/components/RunsPanel.tsx) | Configure & `POST /sim/batch`; the run grid fills live (polls status). |
| Map | [components/MapGL.tsx](frontend/src/components/MapGL.tsx) | `forwardRef` deck.gl map (domain-clamped camera, `fitDomain`/`zoomBy`): vessels as directional icons tinted by state, ports, collisions (fading), wrecks, SAR/MOB assets, storm rings, weather bitmap overlay, faint AIS route network. |
| Map wrapper / controls | [components/MapPane.tsx](frontend/src/components/MapPane.tsx), [components/MapControlBar.tsx](frontend/src/components/MapControlBar.tsx) | Camera + layer toggles (routes / weather / legend / route-type filter). |
| Aggregate stats | [components/StatsPanel.tsx](frontend/src/components/StatsPanel.tsx), [components/StatisticsPanel.tsx](frontend/src/components/StatisticsPanel.tsx) | Hypothesis ranking + box-plots (from `/sim/batch/stats`) and a selected-run summary. |
| Time-series | [components/TelemetryPanel.tsx](frontend/src/components/TelemetryPanel.tsx) | recharts KPI/telemetry curves that grow with the scrubber. |
| Comms | [components/CommsLog.tsx](frontend/src/components/CommsLog.tsx) | Message log for the current tick. |
| Playback controls | [components/PlaybackDock.tsx](frontend/src/components/PlaybackDock.tsx) | Play/pause, scrubber, speed presets. |

**Hooks & contract:**

- [hooks/usePlayback.ts](frontend/src/hooks/usePlayback.ts) — the playback engine: lists runs, loads a run's JSONL into `ticks[]`, drives the scrubber (`currentIdx`) with a `setInterval` at the chosen speed. One shared instance is lifted to `App`.
- [hooks/useRunSeries.ts](frontend/src/hooks/useRunSeries.ts) — derives KPI / telemetry / comms series from `ticks[]`.
- [types.ts](frontend/src/types.ts) — **the backend contract.** `TickMessage`, `VesselSnapshot`, `TelemetrySnapshot`, `KpiSnapshot`, `HypothesisResult`, etc. mirror the Rust snapshot schema — **keep this in sync with `SimState::build_snapshot` when you add snapshot fields.**
- [theme/tokens.ts](frontend/src/theme/tokens.ts) — palette + shared `METHOD_COLORS` / `KPI_META` / scenario labels.

Code-splitting is deliberate: the deck.gl `gl` chunk and the recharts `charts` chunk are split
out ([vite.config.ts](frontend/vite.config.ts)); the recharts tabs are `React.lazy`.

---

## 9. Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (stable) — Linux CI additionally needs `libfontconfig1-dev`.
- [Node.js](https://nodejs.org/) 20+
- Optional: `make`; a LaTeX toolchain (`pdflatex` + `bibtex`) for the report; Python 3.11+ for `tools/`.

### Run it locally

```bash
# Terminal 1 — API server (serves data/ + outputs/ relative to CWD, so run from repo root)
cargo run -p api --release

# Terminal 2 — frontend dev server
cd frontend
npm install
npm run dev
```

Open **http://localhost:5173**. With `make` installed, `make dev` runs both.

> The API reads `data/*` and reads/writes `outputs/*` **relative to the working directory** —
> always launch it from the repo root.

### First run-through

1. Start both servers. 2. In **Runs**, launch a small batch (e.g. 2 seeds, CalmPassage +
StormCorridor, Proposed + BaselineA). 3. Watch the grid fill. 4. Pick a run and press play —
the map animates and Telemetry/Comms grow with the scrubber. 5. Open **Statistics** for the
hypothesis tests once ≥2 methods have finished.

---

## 10. Command cheat-sheet

```bash
# ── Run ────────────────────────────────────────────────────────────────────
cargo run -p api --release                 # backend  (localhost:3000)
cd frontend && npm run dev                 # frontend (localhost:5173)
make dev                                   # both together

# ── Checks (match CI) ──────────────────────────────────────────────────────
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cd frontend && npm run build && npm run lint
make check                                 # all of the above + tsc

# ── Tests ──────────────────────────────────────────────────────────────────
cargo test                                 # unit + integration (fast)
cargo test --test calibration -- --include-ignored   # slow calibration guard

# ── Regenerate routes (WRITE TO SCRATCH until you adopt them!) ──────────────
cargo run -p aisprocess --bin gen_routes -- \
    --seed 42 --out /tmp/ais_paths.json --ports-out /tmp/ports.json
cargo run -p aisprocess --bin iwrap        # offline IWRAP collision-freq check

# ── Data fetchers (regenerate bundled inputs) ──────────────────────────────
python tools/fetch_bathymetry.py           # → data/bathymetry.*
python tools/fetch_lanes.py                # → data/lanes.*
python tools/build_coastline.py            # → data/coastline.geojson

# ── Hyperparameter sweep (§6) — backend must be running ────────────────────
#   Tier 1: edit the CONFIG/SWEEP block in tools/sweep.py (SimConfig axes), then:
python tools/sweep.py --dry-run            # preview the grid
python tools/sweep.py --fresh              # run → outputs/sweep_summary.csv + sweep_all.parquet
#   Tier 2 (routes/lanes): regenerate variants to scratch, add an "ais_path" axis (§6.3):
cargo run -p aisprocess --bin gen_routes -- --n-cargo 50 --out /tmp/routes_dense.json --ports-out /tmp/ports.json

# ── Report ─────────────────────────────────────────────────────────────────
pwsh report/build.ps1                      # → report/build_latex/report.pdf
```

---

## 11. Conventions & gotchas (read before you touch things)

- **Determinism is sacred.** One seeded `SmallRng` per `SimState`. New stochastic behaviour
  must draw from `self.rng`; `test_determinism_same_seed` will catch you if not.
- **One tick body.** All per-tick physics lives in `SimState::post_tick`. Don't add a second
  place things happen — live and batch must stay identical.
- **Never silently clobber `data/`.** It's the committed calibration baseline. `gen_routes`
  defaults to those paths — pass `--out`/`--ports-out` to scratch until adoption is decided.
- **The frontend `types.ts` is a contract.** Add a snapshot field in `build_snapshot`? Mirror
  it in `types.ts` (and consume it) or the map/panels won't see it.
- **CI is the bar:** `cargo fmt` + `clippy -D warnings` (workspace lints are `pedantic`) and
  `frontend: npm run build` (tsc) + `npm run lint`. Green locally = green in CI.
- **The report has a build gotcha.** It builds into `report/build_latex/`; a *stale*
  `report/report.aux` at the repo-relative root shadows it and makes new labels show as
  "undefined". [report/build.ps1](report/build.ps1) clears those stale artifacts for you — use it.
- **`config_override` deep-merges** over scenario defaults (any field, incl. nested
  `simcol`/`sar`/`forcefield`), but scenario/method/seed are always re-pinned.

---

## 12. TODO / roadmap — start here

Ordered by impact. Items are grounded in the code and the open project notes.

### P1 — Finish the science (the actual deliverable)

- [ ] **Make the hyperparameter sweep the primary tuning loop ([§6](#6-hyperparameter-sweeping)).**
  Drive [tools/sweep.py](tools/sweep.py) + the batch API's `config_override` to search for better
  settings — fleet size `n_vessels`, storm comms/geometry, following radii, `simcol`/`sar` params —
  **and** the route/lane knobs via regenerated JSONs (`--n-cargo/--n-passenger/--n-tanker`,
  `--no-lanes`, router `w_lane`). Score points from `outputs/sweep_summary.csv` against the §6
  objectives (calibration realism + H1–H3 effect size), then **bake the winners into the scenario
  defaults** in [scenario.rs](crates/simulation/src/scenario.rs). Enabling tooling (small, do first):
  (a) expose `w_lane`/`w_shallow`/`jitter` as `gen_routes` CLI flags; (b) add a zipped/variant-list
  axis mode to `sweep.py` so matched route+ports variants sweep cleanly; (c) add an objective score
  that auto-ranks sweep points.
- [ ] **Run the definitive experiment and report results.** Execute the full 30-seed ×
  4-scenario × 3-method batch, then confirm/refute **H1/H2/H3** from `/sim/batch/stats`.
  The report still says *"We hypothesise…"* — it needs final numbers, effect sizes, and figures.
- [ ] **Fix the compressed storm/calm collision contrast.** Under the new depth+lane routes the
  StormCorridor-vs-CalmPassage collision ratio fell from ~2.7× (old real-AIS tracks) to ~1.3×,
  which weakens the H1/H3 signal. **This is a storm-model / scenario-design problem, not routing.**
  Levers in [scenario.rs](crates/simulation/src/scenario.rs) (`StormCorridor`) and the storm code in
  [state.rs](crates/simulation/src/state.rs) / [weather.rs](crates/simulation/src/weather.rs):
  `storm_comms_success_rate`, `storm_radius_nm`, storm **track placement over the busy lanes**,
  corridor density. *Run a multi-seed batch to confirm before tuning — the numbers above are single-seed.*
- [ ] **Revisit the BaselineA "honest storm comms" decision** now that traffic is dispersed along
  real lanes — it interacts with the contrast problem above.

### P2 — Model credibility

- [ ] **Resolve or formally accept the SIMCOL `S_i` survival surrogate** — the *only* remaining
  `TODO-VERIFY` in the codebase ([simcol.rs](crates/simulation/src/simcol.rs), lines ~67–84). It's a
  deliberate SOLAS-II-1-shaped surrogate because per-ship residual-stability curves aren't simulated.
  Either source real curves or document the surrogate as a stated limitation in the report and move on.

### P3 — Report hygiene

- [ ] **Fix the stale intro line.** [report.tex](report/report.tex) §Introduction still says vessels
  are driven *"along hand-authored shipping tracks"*, contradicting the abstract and §3.4, which
  describe the procedural depth+lane generator. Update the intro prose.
- [ ] **Populate [report/figures/](report/figures/)** with generated result figures (KPI box-plots,
  hypothesis-ranking) once the definitive batch has run. (The causal-pipeline figure is already TikZ.)

### P4 — Frontend / product

- [ ] **Browser walkthrough of the current shell** (commit `e8c7e06`, "left-panel tabs, clean map").
  Confirm GL render, live-playback feel, and layout across sizes — earlier redesigns passed
  tsc/lint/build but were never visually QA'd.
- [ ] **Decide the fate of live mode.** `POST /sim/start`, `/sim/stop`, and the `/ws` WebSocket
  ([ws.rs](crates/api/src/ws.rs)) still exist server-side but the batch-first UI doesn't use them.
  Either re-expose a "Live" tab or remove the dead server path to cut confusion.
- [ ] *(nice-to-have)* Replace the 2.5 s batch-status polling with a Server-Sent-Events / push
  "run-completed" signal.

### P5 — Data / infra (optional, never built)

- [ ] **Opt-in live-AIS refresh** (Digitraffic Baltic keyless / AISStream free key) to update the
  route network from real traffic on demand — the offline generator stays the default.
- [ ] Consider surfacing the offline IWRAP `N_c` per-run into the results table (currently offline-only,
  by design — the report frames it as external validation).

---

## 13. Glossary & scientific references

The credibility of the model rests on accredited sub-models. Source PDFs live in `citings/`
(gitignored). See [report/report.tex](report/report.tex) and [report/mybib.bib](report/mybib.bib)
for full citations.

| Term | What it is |
|---|---|
| **krABMaga** | The Rust ABM framework the engine runs on (deterministic, data-parallel). |
| **CPA / TCPA** | Closest Point of Approach / Time to CPA — the encounter-detection geometry driving avoidance. |
| **COLREGs** | International collision-avoidance regulations — the deterministic give-way baseline. |
| **DTU SIMCOL** | Accredited probabilistic ship-collision **damage/consequence** model (`simcol.rs`). |
| **IWRAP Mk II** | Regulator-grade collision-**frequency** model — used here as an offline cross-check (`iwrap.rs`). |
| **IAMSAR / MASSIM** | International SAR manual + a Monte-Carlo search-detection model (Koopman random-search) (`sar.rs`). |
| **Ashrafi (2024)** | Month-conditioned Arctic SAR environmental degradation (`ashrafi.rs`). |
| **Xiao (2013)** | Artificial-force-field nautical-traffic steering — the optional avoidance track (`forcefield.rs`). |
| **Karatas (2018)** | Simulation of a man-overboard search operation — grounds the MOB path in `sar.rs`. |
| **`W`** | Local weather hazard scalar ∈ [0,1] from the stochastic field; halves comms success and slows vessels. |
| **`P_prep`** | Crew preparedness = awareness × fatigue — the human-factor multiplier on comms success. |
| **tick** | 15 simulated minutes. A standard run = 2880 ticks = 30 days. |
| **ship-hour** | 0.25 h per Active vessel per tick — the KPI denominator. |

---

*This README is the developer manual. The scientific narrative (equations, accreditation,
results framing) lives in [report/report.tex](report/report.tex); ongoing design decisions and
their rationale are tracked in the maintainer's project notes.*
