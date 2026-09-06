# Experiment design

Factorial design: **4 scenarios × 3 methods × N seeds**. One tick = **15 minutes**; default `n_ticks = 2880` (**30 simulated days**).

Presets live in [`crates/simulation/src/scenario.rs`](../crates/simulation/src/scenario.rs) (`SimConfig::for_scenario`).

## Scenarios

| Scenario | Vessels | Stressor |
|---|---|---|
| `CalmPassage` | 20 | Calm weather — calibration reference |
| `StormCorridor` | 25 | Moving storm (Channel → North Sea → Skagerrak → Gdańsk); inside zone comms ≈ 8 %, speed × 0.45 |
| `BlindShore` | 25 | Fleet-wide degraded comms (0.55) + tighter avoidance berth |
| `DeepWaterRescue` | 15 | Sparse fleet, slower / more distant SAR vs fixed survival windows |

## Methods

| Method | Behaviour |
|---|---|
| `BaselineA` | Classical COLREGs; no weather *awareness* (does not slow in storm). Still takes environmental storm-comms loss. |
| `BaselineB` | Weather-aware speed; shore-oriented fatigue/comms modelling |
| `ProposedSystem` | Fatigue-aware watch scheduling + coordinated avoidance + full rescue chain |

## Hypotheses

From the report (Mann–Whitney U, Holm–Bonferroni, Cliff’s \|\δ\| ≥ 0.2):

- **H1** — Proposed reduces fatal-event rate by ≥ 20 % vs Baseline A
- **H2** — Proposed improves post-incident survival by ≥ 15 % vs Baseline A
- **H3** — Proposed beats Baseline B on fatals under `StormCorridor`

**Causal claim:** collision-warning success is `environmental_factor × crew_fatigue_factor`. Consequence (SIMCOL) and SAR turn misses into measurable survival KPIs.

Definitive batch artifacts and write-up: [`report/report.pdf`](../report/report.pdf) §Results; JSON under `outputs/definitive_*.json` when retained locally.
