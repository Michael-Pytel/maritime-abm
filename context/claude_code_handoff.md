# Claude Code Handoff — Maritime ABM: report ↔ code reconciliation + restructure
 
> **Task for this session.** Restructure `report.tex` so it describes the *actual* Rust/krABMaga
> system (Baltic+North Sea, hand-authored vessel tracks + manual ports, multi-vessel), and write in the accredited
> death-probability and SAR methods (DTU SIMCOL, IWRAP, TU Delft Force Field, MASSIM, Ashrafi)
> **as implemented** (present tense). The human will provide the source PDFs for those external
> methods so equations are verified, not invented. Until those PDFs arrive, mark any external
> equation you can't source as `% TODO-VERIFY` rather than guessing.
 
---
 
## 0. The one fact that explains everything
 
**The current `report.tex` describes an OLD design that the code no longer matches.** The report
is written around Python/Mesa, a 100×100 nm synthetic archipelago, manual HTML waypoints, a
ship-to-ship weather mesh, coastal stations, and a deterministic rescue pipeline. The **actual
code** is Rust/krABMaga on the **real Baltic+North Sea**, with **hand-authored vessel tracks +
manually drawn ports** (both drawn in `tools/waypoint_editor.html`), **CPA/COLREGS
collision avoidance**, **fatigue-coupled comms**, and a **per-crew Bernoulli** consequence model —
and has **no mesh, no coastal stations, and no SAR subsystem at all**. The restructure must close
this gap *and* layer in the accredited methods the human wants.

> **CORRECTION (manual tracks, not AIS).** An earlier draft of this handoff claimed the code uses
> *AIS-derived routes* and *automatic ports*. That is wrong. Routes in `data/ais_paths.json` are
> **hand-authored keypoint polylines** drawn in `tools/waypoint_editor.html` (Vessels tab) — the
> file is merely *named* `ais_paths.json`. Ports in `data/ports.json` are **manually drawn polygons**
> from the same tool's Ports tab; the sim loads `ports.json` first and only falls back to deriving
> ports from route endpoints (`ais::extract_ports`) when that file is absent. The `crates/aisprocess`
> ETL *can* generate routes from real `aisdk-*.csv` dumps, but it is **not** the live data source. So
> the report's original "manual HTML waypoint lanes" wording was actually *correct* — keep it manual.
 
---
 
## 1. Files in play
 
| File | What it is | Status |
|---|---|---|
| `report.tex` | The manuscript ("Decentralised Weather Mesh Networking…"), Interspeech template | **Rewrite target** — currently describes old Mesa design |
| `mybib.bib` | Bibliography (16 entries) | Needs new entries for SIMCOL/IWRAP/ForceField/MASSIM/Ashrafi; **also has a bug, see §6** |
| `RESEARCH_SYNTHESIS.md` | Human's reconciliation of report vs code vs external lit; flags what matters | Source of truth for *intent* |
| `DEEP_RESEARCH_BRIEF.md` | Code-derived math spec (reverse-engineered from Rust) | Source of truth for *what code actually does* |
| `Maritime_ABM_..._Improvement.pdf` | A Gemini deep-research review | **Evaluated the OLD report, not the code** — its "migrate to Rust" advice is already done; treat framing as stale but citations as real |
 
---
 
## 2. Report ↔ Code divergence table (what to fix)
 
| `report.tex` claims (OLD) | Actual Rust code (`DEEP_RESEARCH_BRIEF.md`) | Action |
|---|---|---|
| Python / Mesa framework | **Rust / krABMaga 0.6** (axum, tokio, rayon, geo/rstar, React19/leaflet frontend) | Rewrite all framework text |
| 100×100 nm synthetic archipelago | **Real Baltic+North Sea**, equirectangular, φ∈[50.5,66.0]°, λ∈[−5.0,31.0]°, ≈1136×930 nm, Natural-Earth coastline in R-tree | Rewrite world/geometry §; `WORLD_SIZE_NM`=100 constant is wrong |
| Manual HTML waypoint lanes | **Hand-authored keypoint polylines** drawn in `tools/waypoint_editor.html` → `data/ais_paths.json` (schema `mmsi/name/vessel_type/waypoints[]`); waypoint pursuit + 5-level land sub-stepping. `aisprocess` ETL exists but is **not** the live source. | **Keep manual** — original wording was right; only update mechanism (real geography, polyline pursuit, land sub-stepping). Do **not** rewrite to "AIS-driven/automatic". |
| *(not in old report)* Ports | **Manually drawn polygons** in `tools/waypoint_editor.html` (Ports tab) → `data/ports.json`; loaded via `load_ports_from_file`, auto-derivation from route endpoints (`extract_ports`) is only a fallback when `ports.json` is missing | Describe ports as **manually authored** zones, not auto-derived |
| Ship-to-ship weather mesh (2-hop gossip, +30 nm) | **No mesh.** Vessels read weather only at own cell | Remove mesh as the headline OR reframe (see §5 decision) |
| Coastal station agents (noisy broadcast) | **None** | Remove |
| Confidence-weighted forecast fusion + `E_wx` | **None** — no forecasts exist | Remove fusion eqs |
| Damage points → SUNK/EVAC/RESCUED states | **Only `Active`/`Docked`**; fatalities = per-crew Bernoulli at collision | Replace with accredited model (SIMCOL, §4) |
| `P_err` + crew archetypes (Green/Std/Vet) | **Fatigue scalar F** (base+weather+circadian), recovers in port, per-method factor | Reconcile to the F model actually coded |
| `P_prep` from forecast error + archetype | `P_prep = max(0,1−α_M·W)·max(0,1−0.70F)`, α_M∈{2.0 A,1.3 B,1.0 Prop} | Use the coded formula |
| `P_raft` survivability wired in | Exists **only in a unit test** (`tests/unit_p_raft.rs`), not wired | Either wire it or don't claim it |
| Rescue agents HELO 80kn/PATROL 25kn, SOS, TTA | **None** — snapshot emits empty `rescue_agents`/`shore_stations`; `evac/fatal/rescued` hardcoded 0 | Build SAR (MASSIM+Ashrafi, §4) |
 
**What the code added that the report under-sells (KEEP + foreground):**
CPA/TCPA collision avoidance with COLREGS-style give-way (lower-id turns starboard, 3-message VHF
exchange `CollisionWarning→GivingWay→MaintainingCourse`+`ResumeRoute`, cooldown); comms success
gated by **weather × fatigue**; a deterministic moving **storm corridor** (English Channel→North
Sea→Skagerrak→Kattegat→Baltic→Gdańsk); real coastline land mask with R-tree grounding.
 
---
 
## 3. What the code ACTUALLY computes (verified, transcribe these)
 
**Coordinate system** — equirectangular, k_lat=60 nm/°, φ₀=58.25°, k_lon=60·cos φ₀; x=(λ−λmin)·k_lon,
y=(φ−φmin)·k_lat. E–W distortion ≤~6–10% at edges, accepted.
 
**Time** — Δt=0.25 h (15 min); T=2880 ticks (30 d); UTC clock advances 0.25 h/tick from 06:00.
 
**Cruise speed by vessel type** (u~U(0,1)): passenger/ferry/ro-ro/ro-pax 15+8u; tanker 10+4u;
cargo/container/bulk 10+6u; else 10+10u.
 
**Waypoint pursuit** — arrival when ρ<0.5 nm; step s=min(0.25·v, ρ); land sub-stepping over up to 5
halved steps s·2⁻ʲ; grounding guard reverts to last valid position. Ports (manually drawn polygons
in `data/ports.json`): on entry the vessel docks, dwells D~U{8,48} ticks, then reverses direction.
Fixed fleet: respawn onto a random hand-authored track when |fleet|<N.
 
**Fatigue F∈[0,1]** (internal, not snapshotted):
- circadian c(t)=½(1−cos(2π(t−3)/24)), peaks ~03:00, trough ~15:00
- ΔF=(0.0015 + 0.0040·W + 0.0008·c)·m(method), m∈{1.00 A, 0.82 B, 0.65 Prop}
- docked recovery F−=0.025/tick (~40 ticks)
- comms degradation g(F)=1 if F≤0.40 else 1−0.65·(F−0.40)/(1−0.40), range [0.35,1]
**Weather field** — 50×50 grid, 3 channels (sea s, vis v, wind u), 7 stages:
1. regime 2-state Markov, p_cs=0.03/p_sc=0.08 (preset-scaled); targets Calm(0.30,0.80,0.25)/Storm(0.72,0.30,0.75)
2. ≤8 Lagrangian storm cells: c+=ċ, I−=τ (0.006 storm/0.020 calm), r=8(1+0.15sinθ)√I, θ+=0.08; remove if I<0.12; spawn prob 0.08·R, R=1.35 storm/0.65 calm
3. AR(1)+advection: X_{t+1}=ρ_X·adv(X_t)+(1−ρ_X)·μ_X+σ_X·(0.3+𝔲)·η, 𝔲=0.30, η~U(−1,1); ρ=(0.95,0.90,0.93), σ=(0.07,0.06,0.07)
4. system overlay: ω=(1−√(d²/r²))²·I; s+=ω, u+=ω, v−=0.85ω
5. cross-channel coupling: s+=0.35(u−0.5)−0.20(v−0.5); u−=0.25(v−0.5) **(note: v not modified despite 3-eq comment)**
6. smoothing (5-pt, centre weight 2) + gradient limiter G_max=0.12
7. hazard fusion: W=clip((1.0·s+0.5·(1−v)+0.8·u)/(1.0+0.5+0.8)); nearest-cell sampling
**Storm corridor** (separate from field) — single moving zone, drifts 0.30 nm/tick along fixed
lat/lon track; degrades comms + damps speed inside.
 
**Speed reduction** — hazard-gated f_W=max(0.30,1−0.45W) (B & Proposed only; A ignores weather);
storm-zone f_storm∈{1.0 A, 0.45 StormCorridor, 0.70 default}. Applied by lerping position back.
 
**CPA/TCPA + give-way** —
- velocity v(ψ,v)=v·Δt·(sinψ,cosψ)
- t*=−(Δp·Δv)/‖Δv‖², d_CPA=‖Δp+t*·Δv‖ (t* in ticks)
- degenerate: ‖Δv‖²<1e−9 → (‖Δp‖,∞); t*≤0 → (‖Δp‖,0)
- risk gate (ALL): ‖Δp‖≤20 nm, d_CPA≤5 nm, 0<t*≤12 ticks, not in cooldown
- lower-id vessel gives way (turn starboard); P_comms=min(q_a,q_b)·min(g(F_a),g(F_b)), q=storm_comms_rate(as low as 0.08 in StormCorridor) else nominal
- avoidance waypoint: w_avoid=p+L_f(sinψ,cosψ)+L_o(cosψ,−sinψ), L_f=3, L_o=5 nm; cooldown 3 ticks
**Consequence model (CURRENT — to be replaced by SIMCOL)** — hard collision at ≤0.30 nm (2·r_coll, r_coll=0.15):
- evac (per vessel): P_evac=min(1, 0.30+0.70·min(W,1)+0.20·F_max)
- per-crew fatality: P_fatal=min(1, 0.04·(1+3W)·μ_M·(1+2·F_max)), μ_M∈{2.0 A, 1.2 B, 1.0 Prop}, Bernoulli per crew member (n=10/vessel)
**KPIs** (ship-hrs = 0.25 h per Active vessel per tick):
collision_per_1k_hrs, fatal_per_1k_hrs, evac_activation_rate (per 1k hrs in code),
survival_ratio=1−fatalities/crew_exposed, avg_tta_hours=0.25·mean(t*) **over warnings (NOT rescue,
since no rescue exists)**, mean_p_prep.
 
**Stats** — factorial {4 scenarios}×{3 methods}×{N seeds}, N=30 (cap 100), rayon-parallel, per-run
SmallRng. Two-tailed Mann–Whitney U (avg-rank ties, normal approx, Φ via A&S 26.2.17),
Cliff's δ=(U₁−U₂)/(n₁n₂), Holm–Bonferroni step-down; significant if p_corr<0.05, confirmed if
also |δ|≥0.2. Pairs: (A vs Prop), (B vs Prop), (A vs B).
 
**Methods** — BaselineA (classical COLREGS, weather-unaware, perfect VHF, no slowdown);
BaselineB (shore rest alerts, weather-aware slowdown); ProposedSystem (smart scheduling + best mitigations).
 
---
 
## 4. Accredited methods to write in AS IMPLEMENTED (wait for PDFs to verify formulas)
 
> Per the human: present these in present tense as if coded. **Do not invent the equations** — they
> will upload the PDFs. Until then, scaffold the sections and `% TODO-VERIFY` each external formula.
> Synthesis-level summaries below are for placement/intent only.
 
1. **DTU SIMCOL — probabilistic damage & survivability** (Lützen; Pedersen). Replaces the per-crew
   Bernoulli model. Time-domain external dynamics + internal deformation; Minorsky absorbed-energy
   correlation → damage PDF → survivability factor **S_i** (1 = survives that flooding case, <1 →
   foundering) as a function of impact velocity/angle/displacement. Plugs into `kpi.rs` consequence.
   Sources: Lützen *Ship Collision Damage* (PhD); *Simplified Ship Collision Model* (VTechWorks);
   probabilistic damage-stability refs.
2. **IWRAP Mk II — regulator-grade collision frequency** (Friis-Hansen, Lützen, DTU). **N_c = N_a·P_c**;
   geometric candidates N_a from AIS lateral distributions × causation prob P_c; P_head-on=0.5×10⁻⁴,
   P_crossing=1.3×10⁻⁴; acceptance ≈ <1 collision/yr/fairway. Implement as synthetic-AIS export of
   agent tracks + offline N_c comparison (Baseline A vs Proposed). Sources: IALA IWRAP wiki + theory
   PDF; *Models for Estimating the Potential Number of Ship Collisions* (J. Navigation).
3. **TU Delft Artificial Force Field** (Xiao, Ligteringen, van Gulijk) — *optional realism track.*
   Attractive destination + repulsive hazard/vessel potentials, AIS-calibrated; COLREGS emerges;
   CCDA driving-style clusters. Layer onto existing CPA so P_prep/fatigue scales repulsion gain.
   Source: Huddersfield eprint 23345 / ResearchGate 269332267.
4. **MASSIM — maritime search** (Onggo & Karatas). IAMSAR search patterns (expanding square, sector,
   parallel sweep), cooperative vs evading targets, **drifting liferafts** (wind/current). This is the
   "search phase" of the new rescue subsystem. Sources: simulation.su 2015; Lancaster eprint 78979;
   Karatas 2012 classification.
5. **Ashrafi — Arctic SAR degradation** (UiT, Barents Sea). Monte-Carlo met severity, expert map
   g_i(·), **month-conditioned** rescue time, icing/no-go thresholds, fuel & mechanical-failure
   limits. Replaces the linear max(0.40,1−0.35W) rescue penalty with severity bands + no-go.
   Sources: ESREL 2023 P580; Kyung Hee pure; Canadian-Arctic helicopter rescue-time network model.
**SAR subsystem to build (currently absent):** `RescueAgent` (HELO/PATROL), SOS trigger on
collision/evac, dispatch from shore, mobilisation delay, transit with environmental penalty,
RESCUED/SUNK states, real evac/fatal/rescued telemetry (currently hardcoded 0 in `state.rs`).
Then layer MASSIM (drift+search) and Ashrafi (month/icing/no-go) so survival_ratio/avg_tta_hours
become meaningful.
 
---
 
## 5. Strategic fork the human already half-answered
 
The **headline thesis of `report.tex` is the weather mesh** — which **does not exist in code**. The
human's restructure brief says the real system is multi-vessel Rust on hand-authored tracks with
accredited death/SAR models. Two coherent endpoints:
 
- **(Likely intended) Re-baseline the report on the real system** — foreground CPA + fatigue-gated
  comms + storm corridor + real geography + hand-authored track following as the contribution; add SIMCOL/IWRAP/SAR as
  the rigour upgrades. Drop or demote the mesh.
- **(Larger) Build the mesh to match the old thesis** — biggest effort, deepest gap; only if they
  explicitly commit.
**Default to the first** unless the human says otherwise. Either way, the report's title/abstract/H1–H3
hypotheses currently assume the mesh and must be revised to match whichever path is chosen.
 
---
 
## 6. Concrete bugs / loose ends to fix while here
 
- **`mybib.bib` line 89 has a stray extra `}`** after the `chauvin2013` entry (the entry closes at
  line 87, then a duplicate `}` at 89). Remove it or BibTeX may choke.
- `report.tex` constant `WORLD_SIZE_NM=100` and all "100×100 nm" / "archipelago" text contradict the
  real Baltic+North Sea domain.
- KPI semantics: report says `avg_tta_hours` = rescue TTA; code measures **collision-warning** TTA
  (no rescue). Pick one and make report+code agree.
- `BlindShore` and `DeepWaterRescue` scenarios differ from defaults **only in fleet size** (25, 15) —
  the report's per-scenario stressors (spawn annuli, σ_shore doubling, green-crew %) are **not coded**.
- Two parallel tick loops (`SimState::run_tick` vs `PostTickAgent::step`) must stay in sync — latent
  bug risk; consider collapsing.
- `P_raft` exists only in `tests/unit_p_raft.rs` — wire it in or stop claiming it in the report.
---
 
## 7. Authors / template facts (don't lose these)
 
- Authors: Michał Pytel, Paweł Pozorski — Warsaw University of Technology.
- Template: Interspeech (`\documentclass[cameraready]{Interspeech}`), `IEEEtran` bib style.
- Crew per vessel n=10; collision radius 0.15 nm (hit at 0.30); 30 seeds; 2880 ticks.
- Real engine constants live in `SimulationConfig` (overridable via dashboard GUI).
---
 
## 8. What to do first in the session
 
1. Read `report.tex`, `DEEP_RESEARCH_BRIEF.md`, `RESEARCH_SYNTHESIS.md` in full.
2. Confirm the §5 fork with the human (mesh-out vs mesh-in) — it changes title/abstract/hypotheses.
3. Wait for the external PDFs (§4) before writing any SIMCOL/IWRAP/ForceField/MASSIM/Ashrafi equation.
4. Restructure `report.tex` around the real system (§2–§3), write accredited methods as implemented
   (§4), fix bugs (§6). Mark unverified external formulas `% TODO-VERIFY`.