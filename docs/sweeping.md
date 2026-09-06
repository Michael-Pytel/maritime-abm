# Hyperparameter sweeping

Use the batch API + [`tools/sweep.py`](../tools/sweep.py) to search parameter space and rank points from artifacts (`sweep_summary.csv`, `sweep_ranked.csv`, parquet).

## Objectives

- **Calibration** — CalmPassage Baseline A collisions near ~1.0 / 1k-hrs; Storm/Calm ratio near ~2.7× (definitive runs ≈ 0.60 and ≈ 2.48×).  
- **Signal** — Proposed vs baselines on `fatal_per_1k_hrs` / `survival_ratio` (H1–H3) via `/sim/batch/stats`.

## Tier 1 — `SimConfig`

Live overrides via `config_override` (dotted paths, cartesian `SWEEP`):

```python
# tools/sweep.py CONFIG block
SCENARIOS = ["CalmPassage", "StormCorridor"]
METHODS = ["ProposedSystem", "BaselineA"]
N_SEEDS = 30
N_TICKS = 2880
SWEEP = {
    "n_vessels": [15, 20, 25],
    "storm_comms_success_rate": [0.04, 0.08, 0.15],
}
```

```bash
cargo run -p api --release          # terminal 1
python tools/sweep.py --dry-run
python tools/sweep.py --fresh       # wipes results.db first
```

## Tier 2 — routes / lanes

Regenerate JSON to scratch, then sweep `ais_path` (and shared `ports_path`):

```bash
gen() { cargo run -q -p aisprocess --bin gen_routes -- "$@"; }
gen --n-cargo 20 --out /tmp/routes_sparse.json --ports-out /tmp/ports.json
gen --n-cargo 50 --out /tmp/routes_dense.json  --ports-out /tmp/ports.json
```

**`VARIANTS`:** when non-empty, each dict is one coupled override set (zipped route+ports); cartesian `SWEEP` is ignored.

Example (after generating networks under `outputs/tier2/`):

```python
VARIANTS = [
    {"ais_path": "outputs/tier2/routes_sparse.json", "ports_path": "outputs/tier2/ports.json"},
    {"ais_path": "outputs/tier2/routes_baseline.json", "ports_path": "outputs/tier2/ports_baseline.json"},
    {"ais_path": "outputs/tier2/routes_dense.json", "ports_path": "outputs/tier2/ports.json"},
]
SWEEP = {}
```

Artifacts from the last Tier-2 run: `outputs/tier2_sweep_{summary,ranked}.csv`.

Router CLI weights: `--w-lane`, `--w-shallow`, `--jitter`.

## Ranking

After a sweep, `outputs/sweep_ranked.csv` scores points (lower = better): calibration distance to Calm≈1.0 and Storm/Calm≈2.7, plus H1–H3 signal gaps. Formula is documented in the `sweep.py` module docstring.
