# Experiment design

Factorial design: **4 scenarios × 3 methods × N seeds**. One tick = **5 minutes**.

Presets live in [`crates/simulation/src/scenario.rs`](../crates/simulation/src/scenario.rs) (`SimConfig::for_scenario`).

## Scenarios

| Scenario | Vessels | Horizon | Stressor |
|---|---|---|---|
| `CalmPassage` | 750 | 14 d (4032 ticks) | Calm weather — calibration exposure |
| `StormCorridor` | 900 | ≈4 d (1152 ticks) | Translating storm (~12 kn, ~3 d Channel→Gdańsk); zone comms ≈ 8 %, speed × 0.45 |
| `BlindShore` | 900 | 7 d (2016 ticks) | Fleet-wide degraded comms (0.55) + tighter avoidance berth |
| `DeepWaterRescue` | 500 | 7 d (2016 ticks) | Winter Ashrafi D, slower / more distant SAR vs fixed MOB window |

Fleet sizes track **concurrent underway** Baltic AIS (~774 moving). Contact/CPA: Calm Baseline A band **0.5–2.0** collisions / 1k ship-hrs at Δt=5 min; Storm/Calm ≥ 2. See report §Parameter Rationale and §Limitations.

**Do not** pass `n_ticks` in `/sim/batch` unless intentionally overriding; omit it so each scenario keeps its horizon.

## Methods

| Method | Behaviour |
|---|---|
| `BaselineA` | Classical COLREGs; no weather *awareness* (does not slow in storm). Still takes environmental storm-comms loss. |
| `BaselineB` | Weather-aware speed; shore-oriented fatigue/comms modelling |
| `ProposedSystem` | Fatigue-aware watch scheduling + coordinated avoidance + full rescue chain |

## Hypotheses (primary)

Mann–Whitney U, Holm over the **six** primary contrasts, Cliff’s \|\δ\| ≥ 0.2:

- **H1** — Under `BlindShore`, Proposed reduces fatal-event rate by ≥ 20 % vs Baseline A
- **H2** — Proposed raises `mean_p_prep` vs Baseline A in **each** of the four scenarios
- **H3** — Under `BlindShore`, Proposed reduces collision rate by ≥ 10 % vs Baseline A

Storm fatals vs B and Deep survival complements are exploratory only.
