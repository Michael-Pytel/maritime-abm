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
                      parameter columns + n_runs + mean of each KPI. This is the
                      parameter-vs-KPI comparison table.
  sweep_all.parquet   raw per-run table (label, params, all KPIs) for deeper
                      analysis (load with polars; pandas needs a working pyarrow).

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
SCENARIOS = ["StormCorridor"]
METHODS = ["ProposedSystem", "BaselineA"]
N_SEEDS = 30
N_TICKS = 2880

# Each entry maps a dotted SimConfig path to the list of values to sweep over.
# The script runs the full cartesian product of every axis listed here.
# Nested params use dots, e.g. "forcefield.enabled".
SWEEP: dict[str, list] = {
    "collision_warn_cpa_nm": [0.5, 1.0, 2.0, 5.0],
    # "forcefield.enabled": [False, True],
    # "comms_success_rate": [1.0, 0.7, 0.4],
}
# ─────────────────────────────────────────────────────────────────────────────

KPI_KEYS = [
    "survival_ratio",
    "fatal_per_1k_hrs",
    "collision_per_1k_hrs",
    "avg_tta_hours",
    "evac_activation_rate",
    "mean_p_prep",
]


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
    parts = [f"{k}={v}" for k, v in combo.items()]
    return "_".join(parts)[:120] if parts else "default"


# ── sweep logic ──────────────────────────────────────────────────────────────
def expand_combos() -> list[dict]:
    if not SWEEP:
        return [{}]
    names = list(SWEEP.keys())
    return [dict(zip(names, vals)) for vals in itertools.product(*SWEEP.values())]


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
    total_runs = len(combos) * len(SCENARIOS) * len(METHODS) * N_SEEDS
    print(f"Sweep: {len(combos)} point(s) x {len(SCENARIOS)} scenario(s) x "
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
    param_cols = list(SWEEP.keys())
    cols = param_cols + ["scenario", "method", "n_runs"] + KPI_KEYS
    with open("outputs/sweep_summary.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=cols)
        w.writeheader()
        w.writerows(summary_rows)
    print(f"\nWrote outputs/sweep_summary.csv  ({len(summary_rows)} rows)")

    try:
        _download("/sim/batch/export.parquet?all=true", "outputs/sweep_all.parquet")
        print("Wrote outputs/sweep_all.parquet")
    except urllib.error.HTTPError as e:
        print(f"Parquet export skipped: {e}")

    # Pretty-print the comparison table if polars is available (optional).
    try:
        import polars as pl  # noqa: WPS433

        print("\nParameter vs KPI comparison:")
        print(pl.read_csv("outputs/sweep_summary.csv"))
    except Exception:
        print("\n(install polars for a pretty pivot; sweep_summary.csv is ready to plot)")


if __name__ == "__main__":
    main()
