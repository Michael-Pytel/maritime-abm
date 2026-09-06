# Speaker notes

## Title

- Baltic–North Sea ABM
- human-factor CPA + SAR
- weather × fatigue → warning success

## Problem Statement

- 75–96% human error
- COLREGs assume heard + agreed give-way
- breaks: storms (VHF/AIS) + fatigue (missed CPA)
- most ABMs: fatigue is a side model, not “does the manoeuvre happen?”

## Research Question & Hypotheses

- RQ: does fatigue-aware warnings beat classical COLREGs?
- storm radio loss is **shared** (not a method privilege)
- A: $m=1.00$, no slowdown, $\alpha_M=2.0$
- B: $m=0.82$ + weather slowdown, $\alpha_M=1.3$
- P: $m=0.65$ smart watch, same slowdown, $\alpha_M=1.0$
- H1/H3: **Blind Shore only** (≥20% fatals, ≥10% collisions)
- H2: $P_{\mathrm{prep}}$ up vs A in **every** scenario

## Causal Safety Pipeline

- the paper: $P_{\mathrm{comms}}=\min(q_a,q_b)\,\min(g(F_a),g(F_b))$
- delivered → give-way → averted
- missed → geometry → hard hit → SIMCOL → SAR
- founder vs stay-afloat/MOB comes later

## Scenarios & Scale

- 4 × 3 × 30 = **360 runs**
- underway AIS scale (~774 moving Baltic), not moored census
- Calm 750 / 14d — calibration
- Storm 900 / ~4d — moving cell, comms 0.08
- Blind 900 / 7d — shore radio 0.55 → **primary H1/H3**
- Deep 500 / 7d — winter SAR
- stats: MWU, Holm on six contrasts, $|\delta|\ge 0.2$

## Primary KPIs

- H1 fatal = unrecovered **PIW** inside $\tau_{\mathrm{mob}}$
- liferaft Lost ≠ H1
- H3 = rising-edge hard contacts / 1k ship-hrs
- H2 $P_{\mathrm{prep}}$ from $W$ and $F$, $\alpha_M$ by method
- IWRAP $N_c$ = geometry stamp, not a hypothesis

## Does the simulated world look right?

- Baseline **A only** — if this is junk, H1–H3 are junk
- Calm 0.55 coll/1k hrs ∈ 0.5–2.0 band
- Storm **3.71×** Calm — weather works
- Blind fatals 0.49 vs Calm 0.10 — **why H1/H3 live here**
- Storm = shared blackout → methods cannot separate on fatals

## Workbench — Map & Playback

- batch client, **not** in-browser ABM
- Storm / Proposed / seed 5 / tick 2760
- left: launch factorial, SSE progress, KPI tiles
- map: tracks, avoidance rings, storm disc, collisions
- scrubber / play / speed

## Workbench — Comms Log

- same run, Comms tab
- Warning → GivingWay → MaintainingCourse → ResumeRoute
- this is the $P_{\mathrm{comms}}$ draw you can inspect
- numbers in the paper = JSON/SQLite export, not the GUI

## Agent Platform & Software

- krABMaga 0.6 = the ABM library (`Agent`, `Schedule`, `State`)
- only VesselAgent + PostTickAgent on the schedule (200 / 1000)
- Rescue/MOB/PIW are **objects in post_tick**, not extra scheduled agents
- comms ≠ JADE/FIPA: enum `MsgKind`, logged each tick
- COLREGs dialogue is PostTick-mediated and lossy ($P_{\mathrm{comms}}$)
- message names live on the CPA slide, not here
- Axum/Rayon = experiment harness; React = replay only

## Project Structure

- engine crate map: vessel / state / comms / simcol / sar / weather
- api, frontend, committed `data/`, report
- stack details already on the platform slide

## Agent Types

- VesselAgent: route / avoidance WP / $F$ / FSM
- PostTickAgent: shared physics (order 1000)
- RescueAgent: liferaft helo/patrol
- MobSearchAgent: PIW, no embark
- MobPerson: drift + $\tau_{\mathrm{mob}}$

## Macro-Tick Sequence

- $\Delta t=5$ min
- 1 navigate, then **2–13 post_tick**
- order that matters: CPA (6) **before** collide (8) **before** SAR/MOB (9–10)
- new waypoint used **next** tick, not this one

## Vessel State Machine

- Active ↔ Docked (port dwell)
- founder → Evac (liferaft)
- located → Rescued; timeout → Lost
- afloat overboard → MOB persons, **not** Evac
- terminal ships respawn (fleet $N$ fixed)

## Give-Way, MOB \& SAR

- left: lower id give-way; $\mathbf{w}_{\mathrm{avoid}}=(L_f,L_o)$ starboard
- $d_{\mathrm{CPA}}$ gate, then $P_{\mathrm{comms}}$ coin-flip
- mid: one searcher, many PIW, growing $A(\tau)$
- recover on CDP or lost at $\tau_{\mathrm{mob}}\approx 1.5$ h
- right: RescuePhase FSM (Fig.~11) — helo/patrol
- MOB agents skip Embarking; retire when empty

## Inter-Agent Communications (CPA)

- stand-on = higher id; give-way = lower id
- fail $P_{\mathrm{comms}}$ → **no messages, no WP**
- success: CollisionWarning → GivingWay+WP → MaintainingCourse
- ResumeRoute later (queue empty)
- KeepDistance = port following, separate

## Hypothesis Outcomes

- all **six** primary contrasts confirmed (Holm + $|\delta|\ge 0.2$)
- H1 Blind fatals $0.486\to 0.304$ (~37%, $\delta=+0.53$)
- H3 Blind coll $1.26\to 1.11$ (~12%, $\delta=+0.54$)
- H2 $P_{\mathrm{prep}}$ up in all four; Calm $\delta=-1.00$ strongest
- $\delta$ sign: fatals/collisions down is +, prep up is − (A vs P)

## KPI Distributions

- 30 seeds, A / B / P
- point to Blind fatals + collisions (H1/H3)
- point to prep shift everywhere (H2)
- Storm collisions: overlap, little method gap

## Preparedness & Storm Collisions

- left: A < B < P prep **everywhere** = mechanism
- right: Storm coll only modest drop — shared blackout
- that is why primary outcomes are Blind, not Storm

## Hypothesis Heatmap

- exploratory **full** battery, not the Holm-6
- green = $|\delta|\ge 0.2$ and $p_{\mathrm{corr}}<0.05$ on that battery
- primary claim stays the six contrasts
- Deep survival: directional, not a primary claim

## Key Findings

- coupling $W\times F$ into warnings is an actionable lever
- Blind: fewer fatals and collisions (H1/H3)
- prep up everywhere (H2)
- Storm stays hard for everyone

## Limitations & Next Steps

- $n=20$, scalar $F$, no watch bills
- give-way = lower id, no multi-ship COLREGs
- SIMCOL / Koopman surrogates
- no RCC contention (one helo + patrol)
- next: heterogeneous manning, richer COLREGs, contested SAR

## Thank You

- github.com/Michael-Pytel/maritime-abm

# Q&A

## Why H1/H3 on Blind, not Storm?

- Storm comms 0.08 is **environmental**, same for A/B/P
- fatigue lever cannot show up in a blackout
- Blind: patchy shore radio 0.55 → room for $g(F)$
- Storm still used: H2 + 3.71× collision stressor

## What counts as a fatality?

- unrecovered **person-in-water** inside $\tau_{\mathrm{mob}}\approx 1.5$ h
- liferaft **Lost** logged separately, **not** in H1
- founder → Evac chain, not the KPI fatal

## What is IWRAP doing here?

- external $N_c$ stamp on static routes + fleet $N$
- 1.6–6.6 /yr for the whole multi-corridor domain
- **not** a hypothesis; geometry/sanity check
- not per-run AIS tracks

## Why lower-id give-way?

- reproducible COLREGs **surrogate**
- not full stand-on/give-way hierarchy, no VTS
- limitation, called out on purpose
- force-field track exists, **off** in definitive runs

## Why $n=20$ and scalar $F$?

- IMO A.1047 leaves manning to flags; 15–25 typical cargo/tanker
- exposure count for overboard Bernoulli, not hotel crews
- one $F$ per ship + shared circadian — not STCW watch bills

## Why $\Delta t=5$ min?

- bridge CPA/TCPA alarm band ~12–15 min → several ticks
- scaled from older 15 min calibration (hourly fatigue unchanged)
- finer clock **lowers** discrete contact counts vs 15 min

## Why 360 runs / Holm?

- 4 scenarios × 3 methods × 30 seeds
- primary = **six** contrasts only (Blind fatals+collisions, prep × 4)
- Holm on that six; heatmap = exploratory full battery
- confirm if $p_{\mathrm{corr}}<0.05$ **and** $|\delta|\ge 0.2$

## Cliff $\delta$ sign looks backwards?

- always **A vs P**
- fatals/collisions: P lower → $\delta>0$ (H1/H3)
- prep: P higher → $\delta<0$ (H2)
- magnitude = effect size, not “good/bad” by sign alone

## Why not claim Deep Water / Storm fatals?

- Deep: directional survival, winter SAR, **not** primary
- Storm fatals: shared blackout, no method separation
- H2 still holds there (prep)

## Real AIS or invented routes?

- **synthesised** from EMODnet bathymetry + AIS **density**
- 80 routes, 42 ports, class drafts + UKC
- not replayed voyages; reproducible seeds
- fleets = underway scale (~774 Baltic moving), skip ~3k moored

## How do A/B/P actually differ?

- only $m$, weather/storm slowdown, $\alpha_M$
- A: 1.00 / none / 2.0
- B: 0.82 / yes / 1.3
- P: 0.65 / same as B / 1.0
- SIMCOL, CPA geometry, storm radio: **shared**

## GUI vs paper numbers?

- GUI = inspect + replay JSONL
- hypotheses = exported batch JSON / SQLite
- do not quote KPI tiles as the result

## CPA this tick but ship still hits?

- avoidance WP injected in post_tick step 6
- consumed on **next** navigate
- collide is step 8 **same** tick → possible residual contact
- cooldown 2 ticks against thrashing

## Koopman / SIMCOL realistic?

- both **surrogates**
- SIMCOL + SOLAS-shaped $S_i$, not FE hull / residual stability
- $S^\dagger=0.35$: moderate damage stays afloat → MOB path (KPI)
- SAR: MASSIM/Koopman + Ashrafi month, no live RCC, one helo+patrol

## 3.71× Storm/Calm vs 2.7× target?

- dual calib: Calm A in 0.5–2.0 /1k hrs, Storm/Calm $\ge 2$
- thin-fleet grid ~2.4×; underway definitive **3.71×**
- denser routes **compress** weather contrast — kept 80-route baseline

## Does this transfer outside the Baltic?

- no — bbox, three methods, these seeds
- not MASS/autonomy, not adversarial AIS
- claim = mechanism under this ABM, **not** a casualty census

# Glossary

| Term | Full name | Short descr |
|---|---|---|
| ABM | Agent-based model | Tick-based sim of many agents + environment |
| krABMaga | krABMaga 0.6 | Rust ABM library (`Agent`, `Schedule`, `State`) |
| COLREGs | Collision Regulations | IMO give-way / stand-on rules (we use a lower-id surrogate) |
| CPA / TTA | Closest Point of Approach / Time to CPA | How close / when two tracks would meet |
| VHF / AIS | Very High Frequency / Automatic Identification System | Bridge radio + identity/position broadcasts |
| $W$ | Weather hazard | Local sea/vis/wind fused to $[0,1]$ |
| $F$ | Crew fatigue | Scalar per ship, circadian + weather, $[0,1]$ |
| $q$ | Link reliability | Weather/storm radio quality at a ship |
| $g(F)$ | Fatigue penalty | Cuts warning success once $F>0.40$ |
| $P_{\mathrm{comms}}$ | Warning-delivery probability | $\min(q_a,q_b)\,\min(g(F_a),g(F_b))$ |
| $P_{\mathrm{prep}}$ | Crew preparedness | Awareness $\times$ fatigue; H2 KPI |
| $\alpha_M$ | Method awareness weight | A 2.0 / B 1.3 / P 1.0 in $P_{\mathrm{prep}}$ |
| $m$ | Fatigue build-rate | A 1.00 / B 0.82 / P 0.65 |
| WP | Waypoint | Route target; avoidance WP is temporary starboard |
| $L_f,L_o$ | Forward / offset length | Avoidance WP: ahead + to starboard |
| SIMCOL | DTU ship-collision damage model | Energy → survival $S_i$; founder vs MOB |
| $S_i$ / $S^\dagger$ | Survival factor / founder threshold | $S_i<S^\dagger$ → Evac (liferaft) |
| IWRAP | IALA Waterway Risk Assessment Program Mk II | External collisions/**year** stamp $N_c$ |
| SAR | Search and rescue | Helo/patrol after founder (IAMSAR) |
| MOB / PIW | Man overboard / person-in-water | Afloat casualties, not the liferaft |
| CDP | Cumulative detection probability | Koopman random-search hit chance |
| $\tau_{\mathrm{mob}}$ | MOB survival window | $\approx 1.5$ h; unrecovered = H1 fatal |
| MASSIM | Maritime search simulation | Koopman detection framing we reuse |
| Ashrafi | Ashrafi et al. 2024 | Month factors for SAR speed/boarding |
| RCC | Rescue coordination centre | Real-world dispatcher; we do **not** model contention |
| FIPA / ACL | Foundation for Intelligent Physical Agents / Agent Communication Language | Standard agent speech-acts; we do **not** use them |
| SSE | Server-Sent Events | HTTP stream of batch progress |
| JSONL | JSON Lines | One tick snapshot per line (playback log) |
| Holm | Holm–Bonferroni | Multiple-testing correction on the six primary contrasts |
| Cliff $\delta$ | Cliff's delta | Effect size; confirm if $\lvert\delta\rvert\ge 0.2$ |
| MWU | Mann–Whitney U | Two-sided rank test, 30 seeds |
| ship-hour | Active-ship exposure | $1/12$ h per Active vessel per 5 min tick |
| tick | Macro time step | 5 simulated minutes |


