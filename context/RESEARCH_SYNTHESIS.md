# Research Synthesis — Maritime ABM: Report vs Code vs External Literature

> **Purpose.** A single condensed knowledge base reconciling three sources:
> 1. **`report.tex` / `report.pdf`** — the written manuscript ("Decentralised Weather Mesh Networking for Maritime Safety").
> 2. **The actual Rust/krABMaga code** (documented in [DEEP_RESEARCH_BRIEF.md](DEEP_RESEARCH_BRIEF.md)).
> 3. **The Gemini deep-research report** (`Maritime ABM Simulation Comparison and Improvement.pdf`, mirrored in `research.md`).
>
> Written so a future Claude Code session can pick up with full context. File references are clickable.

---

## 0. TL;DR — the single most important finding

**The deep-research PDF evaluated the *report*, not the *code*.** It says so explicitly: *"the subsequent evaluation … must be drawn entirely from the architecture detailed within the primary manuscript."* The "primary manuscript" is [report.tex](report/report.tex); the "supplementary technical brief" is our [DEEP_RESEARCH_BRIEF.md](DEEP_RESEARCH_BRIEF.md), which the tool says it received **truncated at §6.3** (a copy-paste artifact — the file on disk is complete).

Consequently the PDF describes a system that **the current Rust code does not implement.** The report and the code have diverged badly:

| The report (and therefore the PDF) describes… | The actual Rust code has… |
|---|---|
| **Python / Mesa** framework | **Rust / krABMaga** — already migrated ✅ |
| Decentralised **ship-to-ship weather mesh** (hop-limited gossip, +30 nm) | **No mesh at all** — vessels read weather only at their own cell |
| **Coastal station agents** broadcasting noisy forecasts | **None** |
| **Confidence-weighted forecast fusion** + residual error $E_{wx}$ | **None** — no forecasts exist |
| **Rescue agents** (HELO 80 kn / PATROL 25 kn, SOS, TTA) | **None** — snapshot emits empty `rescue_agents`/`shore_stations` |
| **Damage points → SUNK / EVAC / RESCUED** states | **Only `Active` / `Docked`**; fatalities are per-crew Bernoulli at collision |
| **$P_{err}$ + crew archetypes** (Green/Standard/Veteran) | A **fatigue scalar $F$** with per-method factors — different formula |
| **$P_{prep}$** from forecast error + archetype | $P_{prep}=(1-\alpha_M W)(1-0.70F)$ — different formula |
| **$P_{raft}$** survivability ($0.90-0.60W+0.25P_{prep}$) | Exists **only in a unit test**, not wired in |
| **100×100 nm synthetic archipelago** | **~1136×930 nm real Baltic+North Sea** coastline |
| **AIS-derived routes** (per PDF's recommendation) | **Manually drawn keypoint routes** via `tools/waypoint_editor.html` |

**What the code added that the report/PDF don't emphasise:** a real **CPA/TCPA collision-avoidance system** with COLREGS-style give-way and a 3-message VHF exchange, comms-success gating by weather **and** fatigue, a moving deterministic **storm corridor**, and a real coastline land mask with R-tree grounding. These are genuinely good and largely absent from the report's narrative.

### Net effect on the PDF's five recommendations

1. **"Migrate Mesa → krABMaga"** — **already done.** Ignore. (The PDF's entire "Computational Architecture" section is moot; you are not on Python/Mesa.)
2. **"Adopt AIS routing + Artificial Force Fields"** — **half-moot, half-valuable.** You deliberately use manual keypoints; the *Force Field* dynamic-avoidance idea is still valuable and you already have a CPA system to build on.
3. **"Probabilistic damage (SIMCOL) instead of 3-strike"** — **valuable but mis-targeted:** the code has *no* damage model at all (not even the 3-strike one the report claims), so this is a from-scratch design choice.
4. **"Modernise SAR (MASSIM search + Ashrafi Arctic + drift)"** — **valuable & entirely absent from code.** This is the "rescue vessels" work you mentioned.
5. **"Align to IWRAP / IALA"** — **valuable & genuinely novel for this project.** Output synthetic AIS → IWRAP for regulator-grade collision frequency.

---

## 1. Three-way reconciliation, subsystem by subsystem

Legend: ✅ in code · 📄 in report only · ❌ nowhere · ⭐ external best-practice target.

### 1.1 World & geography
- ✅ **Code:** equirectangular Baltic+North Sea, ~1136×930 nm, Natural-Earth coastline polygons in an R-tree ([ais.rs](crates/simulation/src/ais.rs), [land.rs](crates/simulation/src/land.rs)).
- 📄 **Report:** abstract 100×100 nm island-archipelago.
- **Verdict:** code is *more* realistic than the report; update the report, not the code.

### 1.2 Routing / trajectories
- ✅ **Code:** ordered keypoint polylines authored in [tools/waypoint_editor.html](tools/waypoint_editor.html), stored as `data/ais_paths.json` (schema `mmsi/name/vessel_type/waypoints[]`), reused by the (separate, currently unused-for-live-data) [aisprocess](crates/aisprocess/src/main.rs) AIS ETL crate. Waypoint pursuit with land sub-stepping ([vessel.rs](crates/simulation/src/vessel.rs)).
- 📄/⭐ **Report & PDF:** recommend empirical **AIS extraction** and **TU Delft Artificial Force Field** (Xiao/Ligteringen/van Gulijk): destination = attractive force, hazards/vessels = repulsive forces, calibrated on AIS; COLREGS behaviour emerges. CCDA driving-style clustering.
- **Verdict:** keeping manual keypoints is a legitimate choice for a controlled experiment. The valuable upgrade is **force-field-style continuous avoidance** layered on the existing CPA detector, so $P_{prep}$ could modulate repulsion strength rather than just gating speed.

### 1.3 Weather / hazard field
- ✅ **Code:** 50×50 grid, 7-stage update — regime-switching Markov chain, Lagrangian storm cells, AR(1)+advection, overlay, cross-channel coupling, smoothing/gradient-limiting, fusion to $W\in[0,1]$ ([weather.rs](crates/simulation/src/weather.rs)). Plus a deterministic moving storm corridor.
- 📄 **Report:** same "Tier-2" field conceptually, but framed as the substrate for the mesh.
- **Verdict:** strongest, most-aligned subsystem. ⭐ Optional realism: condition severity on calendar month (Ashrafi) and add sea-spray-icing / "no-go" thresholds for any future SAR assets.

### 1.4 Communication
- ✅ **Code:** **vessel-to-vessel CPA collision-avoidance** only — `CollisionWarning → GivingWay → MaintainingCourse → ResumeRoute`, success-gated by weather × fatigue ([comms.rs](crates/simulation/src/comms.rs), `run_collision_avoidance` in [state.rs](crates/simulation/src/state.rs)).
- 📄 **Report:** the **headline thesis** — hop-limited gossip weather mesh, shore broadcast model `Eq.Prx`, forecast fusion, shore-trust decay $\lambda_{ship}$, SOS relay. **None of this is in the code.**
- **Verdict:** biggest report↔code gap. Decide deliberately (see §5): either build the mesh (to match the thesis) or rewrite the report around the CPA system you actually built.

### 1.5 Human factors
- ✅ **Code:** fatigue scalar $F$ = base + weather + cosine-circadian, recovered in port, per-method build-rate; degrades comms success via $g(F)$ ([vessel.rs](crates/simulation/src/vessel.rs)).
- 📄 **Report:** $P_{err}$ from awake-hours + circadian + **crew archetype** (Green/Standard/Veteran; $w_a{=}0.12, w_c{=}1.5, B_c{=}4.0$); $P_{prep}$ from forecast error + archetype ($\eta{=}1.5,\xi{=}0.2$).
- ⭐ **External:** DTU **Bayesian Belief Networks / Fault Tree** human-reliability (Friis-Hansen, Pedersen): "Asleep" node dominates causation (~60%); human error ≈75% of $P_c$.
- **Verdict:** code's fatigue model is reasonable but simpler and *differently parameterised* than the report. Pick one definition of $P_{prep}$ and make report+code agree.

### 1.6 Collisions & consequences
- ✅ **Code:** hard collision at ≤0.30 nm → per-crew Bernoulli fatality $P_{fatal}=0.04(1+3W)\mu_M(1+2F)$, two evac draws ([kpi.rs](crates/simulation/src/kpi.rs)).
- 📄 **Report:** 3 damage points → SUNK; groundings increment damage → EVAC.
- ⭐ **External:** DTU **SIMCOL** (Lützen) — time-domain external dynamics + internal deformation, Minorsky energy correlation → damage PDF and survivability factor $S_i$.
- **Verdict:** neither the code's Bernoulli model nor the report's 3-strike model is physics-based. SIMCOL-style $S_i$ from impact velocity/angle/displacement is the principled upgrade.

### 1.7 Search & rescue
- ❌ **Code:** **nothing.** `build_snapshot` emits empty `rescue_agents`, `shore_stations`, `wrecks`; telemetry `evac/fatal/rescued = 0` hardcoded ([state.rs](crates/simulation/src/state.rs)).
- 📄 **Report:** full pipeline — SOS → station dispatch → HELO 80 kn / PATROL 25 kn, 2-tick mobilisation, `TTA = d/(v·max(0.40, 1−0.35W))`, rescue within 0.2 nm → RESCUED.
- ⭐ **External:** **MASSIM** (Onggo & Karatas) — IAMSAR search patterns (expanding square, sector, parallel sweep), cooperative vs evading targets, **drifting liferafts** (wind/current); **Ashrafi (UiT, Barents Sea)** — Monte-Carlo met severity, expert map $g_i(\cdot)$, month-conditioned rescue time, icing/no-go, fuel/mechanical limits.
- **Verdict:** **this is the "rescue vessels" work you mentioned — it must be built from scratch.** Start from the report's spec, then layer drift + search patterns (MASSIM) and environmental degradation (Ashrafi). Without a search phase, any `survival_ratio` is inflated.

### 1.8 KPIs & statistics
- ✅ **Code:** 6 KPIs (`collision_per_1k_hrs`, `fatal_per_1k_hrs`, `evac_activation_rate`, `survival_ratio`, `avg_tta_hours`, `mean_p_prep`); Mann–Whitney U + Holm–Bonferroni + Cliff's δ ([stats.rs](crates/api/src/stats.rs)). Note `avg_tta_hours` currently measures **collision-warning TTA**, not rescue TTA (no rescue exists).
- 📄 **Report:** same KPIs but `avg_tta_hours` = rescue Time-to-Arrival.
- ⭐ **External:** **IWRAP Mk II** (Friis-Hansen, Lützen, DTU): $N_c = N_a \cdot P_c$, with $P_{head\text{-}on}=0.5\times10^{-4}$, $P_{crossing}=1.3\times10^{-4}$; regulator-grade *annual collision frequency*.
- **Verdict:** internal KPIs are fine for hypothesis tests; add an **IWRAP bridge** (export synthetic AIS → $N_c$) for external validity.

---

## 2. Corrections — do **not** be misled by the deep-research PDF

1. **You are already on krABMaga (Rust).** The entire "Mesa Python bottleneck → migrate to krABMaga" section is obsolete. The migration *is the current codebase*.
2. **The "severely truncated brief" is a non-issue.** [DEEP_RESEARCH_BRIEF.md](DEEP_RESEARCH_BRIEF.md) is complete to §15; only the *pasted copy* cut off at §6.3. Re-feed the full file if you re-run the research.
3. **Routing is manual-by-design**, via [tools/waypoint_editor.html](tools/waypoint_editor.html). The PDF's "abandon manual HTML waypoints for AIS" assumes you have no reason to; for a controlled information-quality experiment, fixed lanes are defensible. Treat AIS/Force-Fields as an *optional realism* track, not a mandate.
4. **The PDF critiques features the code doesn't have** (3-strike damage, omniscient rescue dispatch). Those critiques apply to the *report's* design; for the code they are "design from scratch," not "fix."
5. **The citations are real and useful** even though the framing is off — see §4.

---

## 3. What is genuinely done well (keep, document, lean on)

- **CPA/TCPA + COLREGS give-way** with deterministic lower-id rule, starboard avoidance waypoint, cooldown, and a believable VHF message log. This is the real, defensible core and the report under-sells it.
- **Comms success gated by `weather × fatigue`** — a clean, novel coupling of environmental + human degradation onto warning delivery. Worth foregrounding as a contribution.
- **7-stage stochastic weather field** — substantive and well-structured.
- **Real coastline land mask** (R-tree, segment sub-stepping) — far beyond the report's archipelago.
- **Rigorous evaluation harness** — Mann–Whitney U + Holm–Bonferroni + Cliff's δ, parallel `rayon` batch, deterministic seeding, disk-backed manifests.

---

## 4. Distilled external knowledge (the useful payload from the research)

Each item: what it is · key sources · where it plugs into the code · rough effort.

### 4.1 IWRAP Mk II — regulator-grade collision risk (DTU)
- **What:** $N_c = N_a P_c$; geometric candidates from AIS lateral distributions × causation probability; acceptance ≈ <1 collision/yr/fairway.
- **Sources:** IALA IWRAP wiki + theory PDF; Friis-Hansen; *Models for Estimating the Potential Number of Ship Collisions* (J. Navigation).
- **Plug-in:** new exporter that writes each run's vessel tracks as synthetic AIS, plus an offline IWRAP comparison of Baseline A vs Proposed $N_c$. Touches [stats.rs](crates/api/src/stats.rs) / a new `crates/api` route + a tools script.
- **Effort:** Medium. **High payoff for external credibility.**

### 4.2 Artificial Force Field nautical traffic (TU Delft / MARIN)
- **What:** attractive destination + repulsive hazard/vessel potentials, AIS-calibrated; emergent COLREGS; CCDA driving-style clusters (K-means).
- **Sources:** Xiao, Ligteringen, van Gulijk — *Nautical traffic simulation with multi-agent system* (Huddersfield repo); MARIN traffic studies; TU Delft PWNT Lab.
- **Plug-in:** replace/augment the waypoint-pursuit + CPA injection in [vessel.rs](crates/simulation/src/vessel.rs) with a force integrator; let $P_{prep}$/fatigue scale repulsion gain.
- **Effort:** High. Optional realism track.

### 4.3 SIMCOL probabilistic damage & survivability (DTU/SDU — Lützen, Pedersen)
- **What:** external+internal collision mechanics, Minorsky absorbed-energy correlation → damage PDF; survivability factor $S_i$ from flooding case.
- **Sources:** Lützen PhD *Ship Collision Damage*; *Simplified Ship Collision Model* (VTechWorks); probabilistic damage-stability refs.
- **Plug-in:** replace the per-crew Bernoulli outcome in [kpi.rs](crates/simulation/src/kpi.rs) with $S_i$ from impact velocity/angle/displacement; feed survivors into evac/rescue.
- **Effort:** High. Do after a decision on whether damage realism is in scope.

### 4.4 DTU Bayesian Belief Network / Fault Tree human factors
- **What:** BBN nodes (Situation Assessment, Danger Detection, Incapacitation, **Asleep**) → causation $P_c$; "Asleep" can dominate (~60%); human ≈75% of $P_c$.
- **Sources:** Friis-Hansen & Pedersen; *Influences of variables on ship collision probability in a BBN model*; IALA causation-probability modelling.
- **Plug-in:** evolve the fatigue scalar in [vessel.rs](crates/simulation/src/vessel.rs) toward a small BBN feeding $P_{err}$ → warning-miss probability (already partly there via `fatigue_comms_factor`).
- **Effort:** Medium.

### 4.5 MASSIM maritime search (Onggo & Karatas)
- **What:** searcher detection of moving/drifting targets; IAMSAR patterns (expanding square, sector, parallel sweep); cooperative vs evading; test-driven validation.
- **Sources:** *Agent-Based Model of Maritime Search Operations* (simulation.su); Lancaster EPrints TDSM/MASSIM; Karatas search-problem classification.
- **Plug-in:** the **search phase** of a new rescue subsystem — see §5 roadmap. New module in `crates/simulation` + agents.
- **Effort:** High.

### 4.6 Arctic SAR environmental degradation (Ashrafi, UiT)
- **What:** Monte-Carlo met severity, expert map $g_i(\cdot)$, **month-conditioned** rescue time, icing/no-go, fuel & mechanical-failure limits.
- **Sources:** Ashrafi — *ABM framework for SAR performance in the Barents Sea* (ESREL 2023, Kyung Hee pure); Canadian-Arctic helicopter rescue-time network model.
- **Plug-in:** environmental layer on rescue-asset speed/availability; replace the report's linear `max(0.40,1−0.35W)` with severity bands + no-go.
- **Effort:** Medium (after rescue assets exist).

### 4.7 krABMaga scaling (already your engine — for justification, not migration)
- **Sources:** JASSS *Reliable and Efficient ABM* (27/2/4); krABMaga GitHub/site; ECS-parallelism papers; ABM framework comparisons (MDPI, JuliaDynamics).
- **Use:** cite to justify scaling vessel counts and Bayesian-optimisation calibration sweeps — *not* a migration task.

---

## 5. Prioritised roadmap (for a future Claude Code session)

A strategic fork comes first because it changes everything downstream:

> **DECISION (needs the human):** Should the **report be rewritten to match the code** (honest, fast, describes the real CPA/fatigue/weather system), or should the **code be extended to match the report** (build the mesh + rescue + damage to deliver the original thesis)? Most likely answer: **a hybrid** — re-baseline the report on the code that exists, then add the highest-value missing pieces below.

**Tier 0 — make report and code tell the same story (low effort, high value)**
- Reconcile $P_{prep}$ (one formula), the world size, the routing description, and the KPI definitions between [report.tex](report/report.tex) and the code.
- Either wire in $P_{raft}$ (it already exists in [unit_p_raft.rs](crates/simulation/tests/unit_p_raft.rs)) or delete its mention from the report.
- Collapse the duplicated tick loop (`run_tick` vs `PostTickAgent`) to remove drift risk.

**Tier 1 — the rescue subsystem you flagged (high value, mostly new code)**
- Implement `RescueAgent` (HELO/PATROL), SOS trigger on collision/evac, dispatch from a shore station, mobilisation delay, transit with environmental speed penalty, `RESCUED`/`SUNK` states, and real `evac/fatal/rescued` telemetry (currently hardcoded 0 in [state.rs](crates/simulation/src/state.rs)).
- Then layer **MASSIM** (drifting-raft targets + IAMSAR search patterns) and **Ashrafi** (month/icing/no-go) so `survival_ratio` and `avg_tta_hours` become meaningful.

**Tier 2 — external validity**
- **IWRAP bridge:** synthetic-AIS export + offline $N_c$ comparison (Baseline A vs Proposed).

**Tier 3 — physics realism (scope-dependent)**
- **SIMCOL $S_i$** damage model replacing Bernoulli fatalities.
- **Force-field** continuous avoidance replacing waypoint-injection (optional; you have CPA already).

**Tier 4 — the original thesis (only if you commit to it)**
- Build the **weather mesh** (coastal stations, hop-limited relay, forecast fusion, shore-trust decay) so the headline hypothesis is actually testable. This is the largest single effort and the deepest report↔code gap.

---

## 6. Curated references (deduped from the PDF's 47, grouped)

**Routing / AIS / Force Fields**
- Xiao, Ligteringen, van Gulijk — *Nautical traffic simulation with multi-agent system* (Huddersfield eprint 23345; ResearchGate 269332267).
- *Influence of external conditions and vessel encounters … using AIS data* (ResearchGate 312456872).
- MARIN Traffic Studies; TU Delft PWNT Lab; *A Quasi-Intelligent Maritime Route Extraction from AIS* (MDPI Sensors 22/22/8639); *WAY: Estimation of Vessel Destination* (arXiv 2512.13190).

**IWRAP / collision frequency (DTU)**
- IALA IWRAP wiki: Probabilistic Collision & Grounding; Theory PDF; Causation Probability Modelling; Interpretation of causation factors.
- *Models for Estimating the Potential Number of Ship Collisions* (J. Navigation, Cambridge).

**Structural damage / survivability (DTU/SDU)**
- Lützen — *Ship Collision Damage* (PhD; ResearchGate 264374677).
- *Simplified Ship Collision Model* (VTechWorks).
- *Probabilistic Damage Stability* (DergiPark); *Structural Design and Response in Collision and Grounding* (ResearchGate 252788157).

**Human factors / BBN**
- Friis-Hansen & Pedersen (BBN/FTA causation).
- *Influences of variables on ship collision probability in a BBN model* (ResearchGate 235444275).

**Maritime SAR**
- Onggo & Karatas — *Agent-Based Model of Maritime Search Operations* (simulation.su 2015); Lancaster TDSM/MASSIM eprint 78979; Karatas 2012 search-problem classification.
- Ashrafi (UiT) — *ABM of SAR in the Barents Sea* (ESREL 2023 P580; Kyung Hee pure); Canadian-Arctic helicopter rescue-time network model (ISCRAM).
- *Unmanned Vehicle Collaboration … Maritime SAR* (ICAS 2016); *Optimizing Heterogeneous Maritime Search Teams* (Semantic Scholar).

**ABM engines / scalability**
- JASSS 27/2/4 *Reliable and Efficient ABM* (krABMaga); krABMaga GitHub + site; *Impact of ECS logic on parallel ABM performance* (CEUR Vol-4124); *Experimenting with ABM Simulation Tools* (MDPI 13/1/13); JuliaDynamics ABMFrameworksComparison; isislab-unisa ABM_Comparison.

**Adjacent ABM exemplars**
- *An Agent-Based Ship Firefighting Model* (MDPI JMSE 9/8/902).

---

## 7. Loose ends to verify next session
- Confirm whether `data/ais_paths.json` is now **only** hand-authored (waypoint_editor) or still partly real AIS via [aisprocess](crates/aisprocess/src/main.rs).
- `BlindShore` and `DeepWaterRescue` scenarios differ from defaults **only in fleet size** in [scenario.rs](crates/simulation/src/scenario.rs) — the report's per-scenario stressors (spawn annuli, σ_shore, green-crew %) are **not** implemented.
- `mean_p_prep` and `avg_tta_hours` semantics differ between report and code (see §1.5, §1.8).
