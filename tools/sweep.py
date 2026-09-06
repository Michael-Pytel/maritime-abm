#!/usr/bin/env python3
"""Scripted hyperparameter sweep driver for the maritime-abm batch API.

Runs the batch experiment once per combination of the swept parameters, waits
for each to finish, and records the mean KPIs so you can compare parameters
against outcomes. Pure standard library — no third-party deps required to run.

Usage
-----
1. Start the API server (from the repo root):   cargo run -p api --release
2. Edit the CONFIG block below.
3. Preview without running anything:            python tools/sweep.py --dry-run
4. Run the sweep (fresh DB recommended):         python tools/sweep.py --fresh

Outputs (written to ./outputs)
------------------------------
  sweep_summary.csv   one row per (sweep point x scenario x method): the swept
                      parameter columns + n_runs + mean of each KPI.
  sweep_ranked.csv    one row per sweep point with an objective score (lower =
                      better), sorted best-first. See score_point() below.
  sweep_all.parquet   raw per-run table (label, params, all KPIs) for deeper
                      analysis (load with polars; pandas needs a working pyarrow).

Objective score (lower is better)
---------------------------------
  score = calib_calm + calib_ratio + signal_h1 + signal_h2 + signal_h3

  calib_calm   = |CalmPassage BaselineA collision_per_1k_hrs − 1.0|
  calib_ratio  = |StormCorridor/CalmPassage BaselineA collision ratio − 2.7|
                 (0 if either scenario missing)
  signal_h1    = max(0, 0.20 − relative_reduction) where relative_reduction is
                 (A−Proposed)/A on fatal_per_1k_hrs (pooled over scenarios with
                 both methods; prefers ≥20 % reduction)
  signal_h2    = max(0, 0.15 − relative_gain) on survival_ratio Proposed vs A
  signal_h3    = max(0, 0.20 − relative_reduction) on StormCorridor fatals
                 Proposed vs BaselineB

Axes
----
* SWEEP  — cartesian product of independent dotted SimConfig paths.
* VARIANTS — when non-empty, each dict is one coupled override set (matched
  ais_path + ports_path, etc.) and SWEEP is ignored. Use this for Tier-2
  route/lane variants that must stay zipped together.

Notes
-----
* Each batch wipes the previous batch's playback .jsonl logs (latest-only, by
  design). The KPI numbers for every point are retained in the results DB.
* --fresh deletes outputs/results.db first so the exported parquet contains
  exactly this sweep and nothing left over from earlier runs.
"""
from __future__ import annotations

import argparse
import csv
import itertools
import json
import os
import time
import urllib.error
import urllib.request

API = "http://localhost:3000"

# ── CONFIG ── edit this block ────────────────────────────────────────────────
# Scout defaults (Tier-1). Archives:
#   outputs/promote_30seed_{summary,ranked}.csv  — best Tier-1 @ 30 seeds (0.08/120)
#   outputs/tier2_sweep_{summary,ranked}.csv     — route density VARIANTS @ 5 seeds
SCENARIOS = ["CalmPassage", "StormCorridor"]
METHODS = ["ProposedSystem", "BaselineA", "BaselineB"]
N_SEEDS = 5
N_TICKS = 2880

# Cartesian product of independent axes. Ignored when VARIANTS is non-empty.
SWEEP: dict[str, list] = {
    "storm_comms_success_rate": [0.04, 0.08, 0.15],
    "storm_radius_nm": [120.0, 160.0, 200.0],
}

# Coupled override sets (zipped axes). When non-empty, each entry is one sweep
# point and SWEEP is ignored. Example Tier-2:
# VARIANTS = [
#     {"ais_path": "outputs/tier2/routes_sparse.json", "ports_path": "outputs/tier2/ports.json"},
#     {"ais_path": "outputs/tier2/routes_baseline.json", "ports_path": "outputs/tier2/ports_baseline.json"},
#     {"ais_path": "outputs/tier2/routes_dense.json", "ports_path": "outputs/tier2/ports.json"},
# ]
VARIANTS: list[dict] = []
# ─────────────────────────────────────────────────────────────────────────────

KPI_KEYS = [
    "survival_ratio",
    "fatal_per_1k_hrs",
    "collision_per_1k_hrs",
    "avg_tta_hours",
    "evac_activation_rate",
    "mean_p_prep",
]

TARGET_CALM_COLLISION = 1.0
TARGET_STORM_CALM_RATIO = 2.7
TARGET_H1_REDUCTION = 0.20
TARGET_H2_GAIN = 0.15
TARGET_H3_REDUCTION = 0.20


# ── HTTP helpers (stdlib only) ───────────────────────────────────────────────
def _get(path: str):
    with urllib.request.urlopen(API + path) as r:
        return json.load(r)


def _post(path: str, body: dict):
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        API + path, data=data, headers={"Content-Type": "application/json"}, method="POST"
    )
    with urllib.request.urlopen(req) as r:
        return json.load(r)


def _download(path: str, dest: str):
    with urllib.request.urlopen(API + path) as r:
        with open(dest, "wb") as f:
            f.write(r.read())


# ── override construction ────────────────────────────────────────────────────
def set_path(d: dict, dotted: str, value) -> None:
    """Sets a dotted key (`a.b.c`) into a nested dict."""
    keys = dotted.split(".")
    for k in keys[:-1]:
        d = d.setdefault(k, {})
    d[keys[-1]] = value


def build_override(combo: dict) -> dict:
    ov: dict = {}
    for path, val in combo.items():
        set_path(ov, path, val)
    return ov


def derive_label(combo: dict) -> str:
    """Mirrors the server's auto-label for dry-run display."""
    if "ais_path" in combo:
        base = os.path.basename(str(combo["ais_path"])).removesuffix(".json")
        return base
    parts = [f"{k}={v}" for k, v in combo.items()]
    return "_".join(parts)[:120] if parts else "default"


# ── sweep logic ──────────────────────────────────────────────────────────────
def expand_combos() -> list[dict]:
    if VARIANTS:
        return [dict(v) for v in VARIANTS]
    if not SWEEP:
        return [{}]
    names = list(SWEEP.keys())
    return [dict(zip(names, vals)) for vals in itertools.product(*SWEEP.values())]


def param_columns(combos: list[dict]) -> list[str]:
    cols: list[str] = []
    seen: set[str] = set()
    for combo in combos:
        for k in combo:
            if k not in seen:
                seen.add(k)
                cols.append(k)
    return cols


def wait_for_batch() -> None:
    while True:
        s = _get("/sim/batch/status")
        done, total = s.get("completed", 0), s.get("total", 0)
        print(f"\r    {done}/{total} ({s.get('progress_pct', 0):.0f}%)", end="", flush=True)
        if not s.get("running", False):
            print()
            return
        time.sleep(2)


def group_means(results: list) -> list[dict]:
    """Means of each KPI grouped by (scenario, method) within one batch."""
    groups: dict = {}
    for r in results:
        groups.setdefault((r["scenario"], r["method"]), []).append(r["kpi"])
    rows = []
    for (scenario, method), kpis in sorted(groups.items()):
        row = {"scenario": scenario, "method": method, "n_runs": len(kpis)}
        for k in KPI_KEYS:
            vals = [kp[k] for kp in kpis if isinstance(kp.get(k), (int, float))]
            row[k] = round(sum(vals) / len(vals), 6) if vals else ""
        rows.append(row)
    return rows


def _num(row: dict, key: str) -> float | None:
    v = row.get(key)
    return float(v) if isinstance(v, (int, float)) else None


def _lookup(rows: list[dict], scenario: str, method: str) -> dict | None:
    for r in rows:
        if r.get("scenario") == scenario and r.get("method") == method:
            return r
    return None


def _rel_reduction(baseline: float | None, proposed: float | None) -> float | None:
    if baseline is None or proposed is None or baseline <= 0:
        return None
    return (baseline - proposed) / baseline


def _rel_gain(baseline: float | None, proposed: float | None) -> float | None:
    if baseline is None or proposed is None or baseline <= 0:
        return None
    return (proposed - baseline) / baseline


def score_point(point_rows: list[dict]) -> dict:
    """Lower score is better. See module docstring for formula."""
    calm_a = _lookup(point_rows, "CalmPassage", "BaselineA")
    storm_a = _lookup(point_rows, "StormCorridor", "BaselineA")

    calm_coll = _num(calm_a, "collision_per_1k_hrs") if calm_a else None
    storm_coll = _num(storm_a, "collision_per_1k_hrs") if storm_a else None

    calib_calm = abs(calm_coll - TARGET_CALM_COLLISION) if calm_coll is not None else 0.0
    if calm_coll and storm_coll and calm_coll > 0:
        ratio = storm_coll / calm_coll
        calib_ratio = abs(ratio - TARGET_STORM_CALM_RATIO)
    else:
        ratio = None
        calib_ratio = 0.0

    # H1 / H2: pool scenarios that have both A and Proposed
    h1_gaps: list[float] = []
    h2_gaps: list[float] = []
    for scenario in {r["scenario"] for r in point_rows}:
        a = _lookup(point_rows, scenario, "BaselineA")
        p = _lookup(point_rows, scenario, "ProposedSystem")
        if not a or not p:
            continue
        red = _rel_reduction(_num(a, "fatal_per_1k_hrs"), _num(p, "fatal_per_1k_hrs"))
        if red is not None:
            h1_gaps.append(red)
        gain = _rel_gain(_num(a, "survival_ratio"), _num(p, "survival_ratio"))
        if gain is not None:
            h2_gaps.append(gain)

    h1_red = sum(h1_gaps) / len(h1_gaps) if h1_gaps else None
    h2_gain = sum(h2_gaps) / len(h2_gaps) if h2_gaps else None
    signal_h1 = max(0.0, TARGET_H1_REDUCTION - h1_red) if h1_red is not None else 0.0
    signal_h2 = max(0.0, TARGET_H2_GAIN - h2_gain) if h2_gain is not None else 0.0

    storm_b = _lookup(point_rows, "StormCorridor", "BaselineB")
    storm_p = _lookup(point_rows, "StormCorridor", "ProposedSystem")
    h3_red = None
    if storm_b and storm_p:
        h3_red = _rel_reduction(
            _num(storm_b, "fatal_per_1k_hrs"), _num(storm_p, "fatal_per_1k_hrs")
        )
    signal_h3 = max(0.0, TARGET_H3_REDUCTION - h3_red) if h3_red is not None else 0.0

    score = calib_calm + calib_ratio + signal_h1 + signal_h2 + signal_h3
    return {
        "score": round(score, 6),
        "calib_calm": round(calib_calm, 6),
        "calib_ratio": round(calib_ratio, 6),
        "storm_calm_ratio": round(ratio, 6) if ratio is not None else "",
        "h1_rel_reduction": round(h1_red, 6) if h1_red is not None else "",
        "h2_rel_gain": round(h2_gain, 6) if h2_gain is not None else "",
        "h3_rel_reduction": round(h3_red, 6) if h3_red is not None else "",
        "signal_h1": round(signal_h1, 6),
        "signal_h2": round(signal_h2, 6),
        "signal_h3": round(signal_h3, 6),
        "calm_a_collision": round(calm_coll, 6) if calm_coll is not None else "",
        "storm_a_collision": round(storm_coll, 6) if storm_coll is not None else "",
    }


def write_ranked(summary_rows: list[dict], param_cols: list[str]) -> None:
    """Aggregate summary rows by sweep point and write sweep_ranked.csv."""
    by_point: dict[tuple, list[dict]] = {}
    for row in summary_rows:
        key = tuple(row.get(c, "") for c in param_cols)
        by_point.setdefault(key, []).append(row)

    ranked: list[dict] = []
    for key, rows in by_point.items():
        combo = dict(zip(param_cols, key))
        metrics = score_point(rows)
        ranked.append({**combo, **metrics, "label": derive_label(combo)})

    ranked.sort(key=lambda r: r["score"])
    cols = (
        ["rank", "score", "label"]
        + param_cols
        + [
            "calib_calm",
            "calib_ratio",
            "storm_calm_ratio",
            "h1_rel_reduction",
            "h2_rel_gain",
            "h3_rel_reduction",
            "signal_h1",
            "signal_h2",
            "signal_h3",
            "calm_a_collision",
            "storm_a_collision",
        ]
    )
    path = "outputs/sweep_ranked.csv"
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=cols, extrasaction="ignore")
        w.writeheader()
        for i, row in enumerate(ranked, 1):
            w.writerow({"rank": i, **row})
    print(f"Wrote {path}  ({len(ranked)} points, best score={ranked[0]['score'] if ranked else 'n/a'})")
    if ranked:
        best = ranked[0]
        print(
            f"  best: {best['label']}  storm/calm={best['storm_calm_ratio']}  "
            f"H1={best['h1_rel_reduction']} H2={best['h2_rel_gain']} H3={best['h3_rel_reduction']}"
        )


def preflight() -> bool:
    try:
        _get("/health")
        return True
    except urllib.error.URLError:
        print(f"Cannot reach the API at {API}.")
        print("Start it first:  cargo run -p api --release")
        return False


def main() -> None:
    global API
    ap = argparse.ArgumentParser(description="Scripted parameter sweep for maritime-abm.")
    ap.add_argument("--api", default=API, help="API base URL")
    ap.add_argument("--fresh", action="store_true",
                    help="delete outputs/results.db first (export = exactly this sweep)")
    ap.add_argument("--dry-run", action="store_true",
                    help="print the planned batches and exit without running")
    args = ap.parse_args()
    API = args.api

    combos = expand_combos()
    mode = "VARIANTS" if VARIANTS else ("SWEEP" if SWEEP else "default")
    total_runs = len(combos) * len(SCENARIOS) * len(METHODS) * N_SEEDS
    print(f"Sweep ({mode}): {len(combos)} point(s) x {len(SCENARIOS)} scenario(s) x "
          f"{len(METHODS)} method(s) x {N_SEEDS} seeds = {total_runs} runs")

    if args.dry_run:
        for i, combo in enumerate(combos, 1):
            print(f"  [{i}/{len(combos)}] {derive_label(combo)}  "
                  f"override={json.dumps(build_override(combo))}")
        return

    if not preflight():
        return

    if args.fresh:
        for suffix in ("", "-wal", "-shm"):
            p = f"outputs/results.db{suffix}"
            if os.path.exists(p):
                os.remove(p)
        print("[fresh] removed outputs/results.db")

    summary_rows: list[dict] = []
    for i, combo in enumerate(combos, 1):
        override = build_override(combo)
        body = {"n_seeds": N_SEEDS, "n_ticks": N_TICKS,
                "scenarios": SCENARIOS, "methods": METHODS}
        if override:
            body["config_override"] = override
        resp = _post("/sim/batch", body)
        print(f"[{i}/{len(combos)}] {resp.get('label', 'default')}")
        wait_for_batch()
        results = _get("/sim/batch/results")
        for row in group_means(results):
            summary_rows.append({**combo, **row})

    os.makedirs("outputs", exist_ok=True)
    param_cols = param_columns(combos)
    cols = param_cols + ["scenario", "method", "n_runs"] + KPI_KEYS
    with open("outputs/sweep_summary.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=cols, extrasaction="ignore")
        w.writeheader()
        w.writerows(summary_rows)
    print(f"\nWrote outputs/sweep_summary.csv  ({len(summary_rows)} rows)")

    write_ranked(summary_rows, param_cols)

    try:
        _download("/sim/batch/export.parquet?all=true", "outputs/sweep_all.parquet")
        print("Wrote outputs/sweep_all.parquet")
    except urllib.error.HTTPError as e:
        print(f"Parquet export skipped: {e}")

    try:
        import polars as pl  # noqa: WPS433

        print("\nParameter vs KPI comparison:")
        print(pl.read_csv("outputs/sweep_summary.csv"))
        print("\nRanked sweep points:")
        print(pl.read_csv("outputs/sweep_ranked.csv"))
    except Exception:
        print("\n(install polars for a pretty pivot; CSVs are ready to plot)")


if __name__ == "__main__":
    main()
