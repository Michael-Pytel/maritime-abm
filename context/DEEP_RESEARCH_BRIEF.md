# Deep-Research Brief — Maritime Agent-Based Safety Simulation

> **What this document is.** A faithful, code-derived mathematical specification of a maritime
> agent-based model (ABM) and its supporting software stack. It was reverse-engineered **only from
> the source code** (Rust backend + React/TypeScript frontend), *not* from any accompanying report or
> paper. Every equation, constant, and threshold below is transcribed directly from the
> implementation, with the originating source file noted. Use it to (a) locate existing work that is
> substantially similar, and (b) serve as a clean, self-contained baseline for a more rigorous
> write-up.

---

## 0. Instructions to the research agent

Please do the following:

1. **Find prior art / near-duplicates.** Identify published models, theses, open-source projects, and
   papers that implement *substantially the same thing* as what is specified below — i.e. a
   tick-based maritime traffic ABM with vessels following AIS-derived routes, a stochastic gridded
   weather/hazard field, CPA/COLREGS-style collision avoidance, crew-fatigue dynamics, and
   probabilistic casualty outcomes, evaluated by non-parametric hypothesis testing across scenarios.
2. **Decompose by component.** For each numbered model component (§4–§13), surface the canonical
   references and competing formulations (e.g. for CPA/TCPA, for AR(1) spatio-temporal weather
   fields, for circadian fatigue models, for collision-consequence/fatality models). State where our
   formulation is standard, where it is a simplification, and where it diverges from the literature.
3. **Be explicit about novelty vs. convention.** For each equation, say whether it matches an
   established model, is a reasonable ad-hoc engineering choice, or is unusual/unsupported.
4. **Return citations** (DOIs / arXiv / standards numbers where possible) grouped by component.

Keywords to seed the search: *agent-based maritime traffic simulation, AIS trajectory simulation,
closest point of approach (CPA/TCPA), COLREGS give-way modelling, ship collision risk index,
maritime collision consequence model, crew fatigue circadian model (e.g. SAFTE/FAST, three-process
model), stochastic weather field AR(1) advection, regime-switching Markov weather, search-and-rescue
ABM, Mann–Whitney U with Holm–Bonferroni, Cliff's delta effect size, krABMaga / Rust ABM.*

---

## 1. System architecture (what is actually built)

A Rust Cargo **workspace** with three crates plus a TypeScript single-page frontend.

| Component | Tech | Role |
|---|---|---|
| `crates/simulation` | Rust, **krABMaga 0.6** ABM engine, `geo`/`geojson`/`rstar` (R-tree), `rand` `SmallRng` | The model: agents, weather field, KPIs, scenario config |
| `crates/api` | Rust, **axum 0.8** + **tokio** (async), **rayon** (data-parallel batch) | REST + WebSocket server; live run control; batch sweeps; statistics |
| `crates/aisprocess` | Rust | Offline ETL: parses raw `aisdk-*.csv` AIS dumps → per-vessel keypoint routes (`data/ais_paths.json`) |
| `frontend` | **React 19**, **react-leaflet 5** (Leaflet maps), **recharts**, Vite | Live map, weather overlays, KPI/comms panels, batch & playback UI |

**Data flow.** `aisprocess` produces route polylines → `simulation` steps the world and emits a JSON
snapshot every *N* ticks over a `tokio::broadcast` channel → `api` relays it on `ws://…/ws` → the
frontend renders vessels, ports, storm zone, weather grid, comms log, and KPIs. Batch runs execute
in parallel via `rayon`, write per-run `*.jsonl` tick logs + `*_manifest.json` + a `summary.csv`,
and expose aggregate statistics.

**Important implementation note (for honest comparison later).** There are **two parallel tick
implementations** of identical logic: `SimState::run_tick` (standalone) and `PostTickAgent::step`
(the krABMaga-scheduled path actually used by `run_blocking`). They must be kept in sync; any
discrepancy is a latent bug. Vessel agents are scheduled at krABMaga ordering `200` and a single
`PostTickAgent` at ordering `1000`.

---

## 2. Coordinate system & projection (`ais.rs`)

The world is an **equirectangular projection** with a cosine-of-mid-latitude correction, so that one
"field unit" equals **one nautical mile** on both axes at the reference parallel. Domain (Baltic +
North Sea):

$$
\phi \in [\phi_{\min}, \phi_{\max}] = [50.5^\circ, 66.0^\circ], \qquad
\lambda \in [\lambda_{\min}, \lambda_{\max}] = [-5.0^\circ, 31.0^\circ].
$$

With $k_{\text{lat}} = 60\ \text{nm}/^\circ$ and reference parallel
$\phi_0 = \tfrac12(\phi_{\min}+\phi_{\max}) = 58.25^\circ$, the longitude scale is

$$
k_{\text{lon}} = 60\,\cos\phi_0 \quad[\text{nm}/^\circ].
$$

Forward projection of a geodetic point to field coordinates $(x,y)$ in nm:

$$
x = (\lambda - \lambda_{\min})\,k_{\text{lon}}, \qquad
y = (\phi - \phi_{\min})\,k_{\text{lat}}.
$$

Domain extent: height $= 15.5^\circ\cdot 60 = 930$ nm exactly; width
$= 36^\circ\cdot 60 \cos 58.25^\circ \approx 1136$ nm. Distances within the engine are therefore
treated as Euclidean nm (E–W distortion ≤ ~6–10 % at the bbox edges, accepted without correction).

**Point-in-polygon** (port docking) uses standard ray casting. **Land mask** (`land.rs`) loads
Natural-Earth coastline polygons (GeoJSON, projected on load) into an **R-tree** of bounding boxes;
`contains_point` and `segment_crosses_land` are accelerated point/segment-vs-polygon tests.

---

## 3. Time discretisation

Discrete fixed step ("tick"):

$$
\Delta t = 15\ \text{min} = 0.25\ \text{h}.
$$

Default horizon $T = 2880$ ticks $= 30$ days. A wall-clock-of-day variable advances
$\text{utc} \leftarrow (\text{utc} + 0.25) \bmod 24$ each tick (initialised at 06:00), and drives the
circadian fatigue term (§5).

---

## 4. Vessel agent — kinematics & voyage logic (`vessel.rs`)

Each vessel $i$ carries state: position $\mathbf{p}_i=(x,y)$, an ordered route
$R_i = (\mathbf{w}_0,\dots,\mathbf{w}_{m-1})$ of waypoints (AIS-derived), a route index and travel
direction $d_i\in\{+1,-1\}$, heading $\psi_i$, cruise speed $v_i$ (kn), discrete state
$\sigma_i\in\{\text{Active}, \text{Docked}\}$, crew count $n_i$, and a fatigue scalar $F_i$ (§5).

**Cruise speed by type** (uniform draw, $u\sim U(0,1)$):

$$
v_i =
\begin{cases}
15 + 8u & \text{passenger / ferry / ro-ro / ro-pax}\\
10 + 4u & \text{tanker}\\
10 + 6u & \text{cargo / container / bulk / general cargo}\\
10 + 10u & \text{otherwise.}
\end{cases}
$$

**Waypoint pursuit.** Let the active target be $\mathbf{t}$ (an avoidance waypoint if one is queued,
else the current route waypoint $\mathbf{w}_{k}$). With
$\boldsymbol{\delta} = \mathbf{t} - \mathbf{p}_i$ and $\rho = \lVert\boldsymbol{\delta}\rVert$:

- **Arrival test:** waypoint considered reached when $\rho < 0.5$ nm.
- **Step length:** the per-tick travel distance is

$$
s = \min\!\big(v_i \,\Delta t,\ \rho\big) = \min(0.25\,v_i,\ \rho).
$$

- **Proposed move:** $\mathbf{p}_i' = \mathbf{p}_i + \dfrac{\boldsymbol{\delta}}{\rho}\, s$.

- **Heading** (compass; 0° = north, 90° = east):

$$
\psi_i = \big(90^\circ - \operatorname{atan2}(\delta_y,\delta_x)\big) \bmod 360^\circ.
$$

**Land-collision sub-stepping.** To prevent tunnelling through thin landmasses, the move is accepted
at the first of up to 5 geometrically halved step lengths $s\cdot 2^{-j}$, $j=0,\dots,4$, whose
segment does not cross land; if all are blocked the vessel skips to the next waypoint (or docks). A
post-move **grounding guard** reverts $\mathbf{p}_i$ to the last valid position if it ended on land.

**Ports & dwell.** On entering a port polygon (or reaching a route endpoint) the vessel docks for a
randomised dwell

$$
D \sim \text{Uniform}\{D_{\min},\,\dots,\,D_{\max}\}, \qquad (D_{\min},D_{\max}) = (8, 48)\ \text{ticks (default)},
$$

then **reverses** $d_i \leftarrow -d_i$ for the return leg. A fixed fleet size is maintained: whenever
$\lvert\text{fleet}\rvert < N$, new vessels are spawned onto a random AIS route. The pseudo-random
dwell uses a SplitMix64-style hash of (id, tick).

---

## 5. Crew fatigue dynamics (`vessel.rs`)

A scalar $F_i \in [0,1]$ per vessel (0 = rested, 1 = exhausted). **Not** persisted to snapshots;
purely internal. Updated every tick.

**While Active**, with local hazard $W\in[0,1]$ (§6) and time-of-day $t_{\text{utc}}\in[0,24)$:

Circadian drive (peaks ≈ 03:00, trough ≈ 15:00):

$$
c(t_{\text{utc}}) = \tfrac12\Big(1 - \cos\!\big(\tfrac{2\pi (t_{\text{utc}} - 3)}{24}\big)\Big) \in [0,1].
$$

Per-tick accumulation:

$$
\Delta F = \big(\underbrace{0.0015}_{\beta_{\text{base}}} + \underbrace{0.0040}_{\beta_{W}}\,W + \underbrace{0.0008}_{\beta_{c}}\,c(t_{\text{utc}})\big)\cdot m(\text{method}),
$$

$$
m(\text{method}) =
\begin{cases}
1.00 & \text{Baseline A (unmanaged)}\\
0.82 & \text{Baseline B (shore rest alerts)}\\
0.65 & \text{Proposed (smart watch scheduling)}
\end{cases}
\qquad
F_i \leftarrow \min(1,\ F_i + \Delta F).
$$

**While Docked** (recovery): $F_i \leftarrow \max(0,\ F_i - 0.025)$ (full recovery in ≈ 40 ticks ≈ 10 h).

**Fatigue → comms degradation.** Tired watchkeepers miss/delay warnings. With threshold
$F^\ast = 0.40$ and max penalty $\kappa = 0.65$:

$$
g(F_i) =
\begin{cases}
1, & F_i \le F^\ast\\[4pt]
1 - \kappa\,\dfrac{F_i - F^\ast}{1 - F^\ast}, & F_i > F^\ast
\end{cases}
\quad\in [\,1-\kappa,\ 1\,] = [0.35,\ 1].
$$

This $g$ multiplies the collision-warning success probability (§8).

---

## 6. Stochastic weather / hazard field (`weather.rs`)

A $50\times 50$ Eulerian grid ($N=2500$ cells) of three physical channels — **sea state** $s$,
**visibility** $v$, **wind** $u$ — each in $[0,1]$, fused into a scalar hazard $W\in[0,1]$. The field
advances each tick through seven ordered stages.

### 6.1 Regime (two-state Markov chain)

A global regime $\in\{\text{Calm}, \text{Storm}\}$ flips per tick with Bernoulli probabilities
$p_{cs}$ (calm→storm) and $p_{sc}$ (storm→calm). Base values $p_{cs}=0.03$, $p_{sc}=0.08$, scaled by
preset:

$$
\text{Calm: }(0.35\,p_{cs},\,1.40\,p_{sc}),\quad
\text{Mixed: }(p_{cs},\,p_{sc}),\quad
\text{Stormy: }(2.00\,p_{cs},\,0.50\,p_{sc}).
$$

The regime selects the mean-reversion targets $(\mu_s,\mu_v,\mu_u)$:

$$
\text{Calm} \to (0.30,\ 0.80,\ 0.25), \qquad
\text{Storm} \to (0.72,\ 0.30,\ 0.75).
$$

### 6.2 Storm-cell lifecycle (Lagrangian blobs)

Up to $K_{\max}=8$ moving cells, each with centre $(c_x,c_y)$, radius $r$, intensity
$I\in[0,1]$, drift $(\dot c_x,\dot c_y)$, and a radius phase. Per tick:

$$
c \mathrel{+}= \dot c,\qquad I \mathrel{-}= \tau,\quad
\tau = \begin{cases}0.006 & \text{Storm}\\ 0.020 & \text{Calm}\end{cases}
$$

$$
r = 8\,\big(1 + 0.15\sin\theta\big)\sqrt{I}, \qquad \theta \mathrel{+}= 0.08 .
$$

Cells with $I < 0.12$ or drifted off-grid are removed. A new cell spawns with probability
$0.08\cdot R$ where $R = 1.35$ (Storm) or $0.65$ (Calm); spawn location uniform, drift speed
$0.6\,(0.5+0.5u)$ at random bearing plus jitter, initial $I \sim U(0.12, 0.80)$.

### 6.3 AR(1) background with advection

The synoptic background advects (integer circular grid shift accumulated from drifts
$\dot a_x=0.018$, $\dot a_y=0.008$ cells/tick) and mean-reverts as a first-order autoregressive
process with multiplicative noise gain. For each channel $X\in\{s,v,u\}$ with persistence
$\rho_X$, target $\mu_X$, noise scale $\sigma_X$, scenario "unpredictability" $\mathfrak{u}=0.30$,
and $\eta \sim U(-1,1)$ i.i.d. per cell:

$$
X_{t+1} = \rho_X\,\mathrm{adv}(X_t) + (1-\rho_X)\,\mu_X + \sigma_X\,(0.3 + \mathfrak{u})\,\eta,
\quad\text{clipped to }[0,1].
$$

$$
(\rho_s,\rho_v,\rho_u) = (0.95,\ 0.90,\ 0.93), \qquad
(\sigma_s,\sigma_v,\sigma_u) = (0.07,\ 0.06,\ 0.07).
$$

### 6.4 System overlay (storm cells imprint on channels)

For each grid cell at distance $d$ from storm-cell centre with $d < r$, a smooth bump

$$
\omega = \Big(1 - \sqrt{d^2/r^2}\Big)^2 I
$$

is added to sea and wind and subtracted (×0.85) from visibility:

$$
s \mathrel{+}= \omega,\qquad u \mathrel{+}= \omega,\qquad v \mathrel{-}= 0.85\,\omega,
$$

each clamped to $[0,1]$.

### 6.5 Cross-channel coupling

$$
s \leftarrow \mathrm{clip}\big(s + 0.35\,(u-0.5) - 0.20\,(v-0.5)\big), \qquad
u \leftarrow \mathrm{clip}\big(u - 0.25\,(v-0.5)\big).
$$

*(Implementation note: visibility $v$ is not modified in this stage even though the source comment
references three coupling equations — only the two above are applied.)*

### 6.6 Smoothing + gradient limiting

Each channel is passed through a 5-point neighbourhood smoother (centre weight 2):

$$
X'_{r,c} = \frac{2 X_{r,c} + \sum_{(r',c')\in\mathcal{N}} X_{r',c'}}{2 + |\mathcal{N}|},
$$

then a single-pass gradient limiter caps any cell-to-neighbour difference at $G_{\max}=0.12$:
if $|X_{r,c} - X_{r',c'}| > G_{\max}$ then $X_{r,c} \leftarrow X_{r',c'} + G_{\max}\,\mathrm{sgn}(X_{r,c}-X_{r',c'})$.

### 6.7 Hazard fusion

$$
W = \mathrm{clip}\!\left(\frac{w_s\,s + w_v\,(1-v) + w_u\,u}{w_s + w_v + w_u},\ 0,\ 1\right),
\quad (w_s, w_v, w_u) = (1.0,\ 0.5,\ 0.8).
$$

**Sampling** the field at a vessel position $(x,y)$ uses nearest-cell lookup:
$\text{col} = \lfloor (x/W_{\text{world}})\cdot 50\rfloor$, $\text{row} = \lfloor (y/H_{\text{world}})\cdot 50\rfloor$ (clamped),
$W(x,y) = W_{\text{grid}}[\text{row},\text{col}]$.

### 6.8 Deterministic storm corridor (separate from the field)

Independently of the stochastic field, the `StormCorridor` scenario carries a **single moving storm
zone** of radius $r_{\text{storm}}$ that drifts at $0.30$ nm/tick along a fixed lat/lon waypoint
track (English Channel → North Sea → Skagerrak → Kattegat → Baltic → Gdańsk). It (a) **degrades
comms** for vessels inside it (§8) and (b) **damps speed** (§7).

---

## 7. Speed reduction under hazard (`state.rs`)

Two multiplicative slow-downs are applied by lerping the just-moved position back toward the last
valid position $\mathbf{p}^{\text{lv}}$ by a factor $f$:
$\mathbf{p} \leftarrow \mathbf{p}^{\text{lv}} + (\mathbf{p}-\mathbf{p}^{\text{lv}})\,f$.

**Hazard-gated** (applies to Baseline B and Proposed only; Baseline A ignores weather):

$$
f_W = \max\!\big(0.30,\ 1 - 0.45\,W\big).
$$

**Storm-zone** (vessels within $r_{\text{storm}}$ of the moving storm centre):

$$
f_{\text{storm}} = \text{storm\_speed\_factor} \in \{1.0 \text{ (A)},\ 0.45 \text{ (StormCorridor)},\ 0.70 \text{ (default)}\}.
$$

---

## 8. Collision avoidance — CPA detection + COLREGS-style give-way (`comms.rs`, `state.rs`)

### 8.1 Velocity and closest point of approach

Field-frame velocity from compass heading $\psi$ and speed $v$ (kn):

$$
\mathbf{v}(\psi, v) = v\,\Delta t\,(\sin\psi,\ \cos\psi), \qquad \Delta t = 0.25.
$$

For a pair $(a,b)$ with relative position $\Delta\mathbf{p} = \mathbf{p}_b-\mathbf{p}_a$ and relative
velocity $\Delta\mathbf{v} = \mathbf{v}_b-\mathbf{v}_a$, the time to CPA and the CPA distance are the
standard linear-motion result:

$$
t^\ast = -\,\frac{\Delta\mathbf{p}\cdot\Delta\mathbf{v}}{\lVert\Delta\mathbf{v}\rVert^2},
\qquad
d_{\text{CPA}} = \big\lVert \Delta\mathbf{p} + t^\ast\,\Delta\mathbf{v} \big\rVert .
$$

Degenerate cases: if $\lVert\Delta\mathbf{v}\rVert^2 < 10^{-9}$ (parallel/identical), return
$(d_{\text{CPA}}, t^\ast) = (\lVert\Delta\mathbf{p}\rVert, \infty)$; if $t^\ast \le 0$ (diverging),
return $(\lVert\Delta\mathbf{p}\rVert, 0)$. Here $t^\ast$ is measured in **ticks** (TTA).

### 8.2 Risk gate

A manoeuvre is considered for pair $(a,b)$ iff **all** hold (defaults shown):

$$
\lVert\Delta\mathbf{p}\rVert \le 20\ \text{nm}, \qquad
d_{\text{CPA}} \le 5\ \text{nm}, \qquad
0 < t^\ast \le 12\ \text{ticks},
$$

and the give-way vessel is not in avoidance cooldown.

### 8.3 Give-way rule & communication success

The vessel with the **lower id** is the give-way (turn-to-starboard) vessel; the other stands on. The
warning is only acted on if it is successfully *communicated*. The success probability is the product
of an environmental factor and a fatigue factor over both vessels:

$$
P_{\text{comms}} = \underbrace{\min\big(q(\mathbf{p}_a),\ q(\mathbf{p}_b)\big)}_{\text{weather/storm}}
\cdot
\underbrace{\min\big(g(F_a),\ g(F_b)\big)}_{\text{fatigue, §5}},
$$

where $q(\mathbf{p}) = \text{storm\_comms\_rate}$ if $\mathbf{p}$ is inside the storm zone, else the
nominal `comms_success_rate`. Storm comms rate is as low as **0.08** in `StormCorridor` (92 % of
warnings lost). The manoeuvre proceeds iff a uniform draw $< P_{\text{comms}}$.

### 8.4 Avoidance waypoint geometry

The give-way vessel receives one temporary waypoint placed `forward_nm` ahead and `offset_nm` to
**starboard** of its current heading $\psi$ (defaults $L_f=3$, $L_o=5$ nm):

$$
\mathbf{w}_{\text{avoid}} = \mathbf{p} + L_f\,(\sin\psi,\ \cos\psi) + L_o\,(\cos\psi,\ -\sin\psi).
$$

A three-message exchange is logged — `CollisionWarning{cpa, tta}` → `GivingWay` →
`MaintainingCourse` — and `ResumeRoute` once the avoidance waypoint is consumed. Each issued warning
records its TTA for the average-TTA KPI. A cooldown (default 3 ticks) prevents thrashing.

---

## 9. Hard collisions & consequence model (`kpi.rs`)

A **hard collision** is logged whenever two Active vessels are within
$d_{\text{hit}} = 2\,r_{\text{coll}} = 2\cdot 0.15 = 0.30$ nm. Each collision triggers a stochastic
consequence evaluation at the midpoint hazard $W$ and the worse of the two crews' fatigue
$F^{\max} = \max(F_a, F_b)$.

**Evacuation activation** (each of the two vessels draws independently):

$$
P_{\text{evac}} = \min\!\big(1,\ \underbrace{0.30}_{\text{base}} + (1-0.30)\,\min(W,1) + 0.20\,F^{\max}\big).
$$

**Per-crew fatality.** With method multiplier $\mu_M \in \{2.0\,(\text{A}),\ 1.2\,(\text{B}),\ 1.0\,(\text{Proposed})\}$
and fatigue multiplier $1 + 2F^{\max}$:

$$
P_{\text{fatal}} = \min\!\Big(1,\ \underbrace{0.04}_{\text{base/crew}}\,(1 + 3W)\,\mu_M\,(1 + 2F^{\max})\Big),
$$

applied as an independent Bernoulli trial for **each** of the $n_a + n_b$ crew members.

---

## 10. Crew preparedness $P_{\text{prep}}$ (`kpi.rs`)

Accumulated each tick for every Active vessel; the reported KPI is the mean over all active
vessel-ticks. With awareness factor $\alpha_M \in \{2.0\,(\text{A}),\ 1.3\,(\text{B}),\ 1.0\,(\text{Proposed})\}$:

$$
P_{\text{prep}} = \underbrace{\max(0,\ 1 - \alpha_M W)}_{\text{weather readiness}} \cdot \underbrace{\max(0,\ 1 - 0.70\,F)}_{\text{fatigue readiness}}.
$$

> **Gap flag for later comparison.** A liferaft-survivability term
> $P_{\text{raft}} = \mathrm{clip}(0.90 - 0.60\,W + 0.25\,P_{\text{prep}},\,0,\,1)$ exists **only as a
> unit test** (`tests/unit_p_raft.rs`) and is **not wired into the production tick loop or any KPI**.
> Treat it as specified-but-unused.

---

## 11. Key performance indicators (`kpi.rs`)

Let ship-hours accumulate $0.25$ h per Active vessel per tick. Over a run:

| KPI | Definition |
|---|---|
| `collision_per_1k_hrs` | $\dfrac{\#\text{collisions}}{\text{ship\_hrs}}\times 1000$ |
| `fatal_per_1k_hrs` | $\dfrac{\#\text{fatalities}}{\text{ship\_hrs}}\times 1000$ |
| `evac_activation_rate` | $\dfrac{\#\text{evac activations}}{\text{ship\_hrs}}\times 1000$ |
| `survival_ratio` | $1 - \dfrac{\#\text{fatalities}}{\#\text{crew exposed in collisions}}$ (=1 if none) |
| `avg_tta_hours` | $0.25\cdot\overline{t^\ast}$ over all issued warnings |
| `mean_p_prep` | mean $P_{\text{prep}}$ over active vessel-ticks |

---

## 12. Experimental design & statistics (`stats.rs`, `runner.rs`)

**Factorial sweep:** $\{4\text{ scenarios}\}\times\{3\text{ methods}\}\times\{N\text{ seeds}\}$
(default $N=30$, capped at 100), each run independent (`SmallRng` seeded per run), executed in
parallel with `rayon`.

- **Scenarios:** `CalmPassage`, `StormCorridor`, `BlindShore`, `DeepWaterRescue`.
  *(Gap flag: in code, `BlindShore` and `DeepWaterRescue` currently differ from the default only in
  fleet size — 25 and 15 vessels — i.e. they are not yet meaningfully distinct scenarios.)*
- **Methods:** `BaselineA` (classical COLREGS, fully weather-unaware, perfect VHF, no slow-down),
  `BaselineB` (shore rest alerts; weather-aware slow-down), `ProposedSystem` (smart scheduling +
  best mitigations).

**Hypothesis testing.** For every (KPI × scenario × method-pair) the two methods are compared with a
two-tailed **Mann–Whitney U** test (average-rank tie handling, normal approximation):

$$
\mu_U = \frac{n_1 n_2}{2}, \quad
\sigma_U = \sqrt{\frac{n_1 n_2 (n_1 + n_2 + 1)}{12}}, \quad
z = \frac{U - \mu_U}{\sigma_U}, \quad
p = 2\,\Phi(-|z|),
$$

with $\Phi$ via the Abramowitz–Stegun 26.2.17 rational approximation. Effect size is **Cliff's delta**

$$
\delta = \frac{U_1 - U_2}{n_1 n_2} \in [-1, 1].
$$

Multiplicity is controlled by **Holm–Bonferroni** step-down across all tests; a result is
`significant` if $p_{\text{corrected}} < 0.05$ and `confirmed` if additionally $|\delta| \ge 0.2$.
Method pairs tested: (A vs Proposed), (B vs Proposed), (A vs B).

---

## 13. Frontend (visualization & interaction)

React 19 + react-leaflet. Live WebSocket (`useSimSocket`) feeds: a Leaflet map of vessel markers
(colour-coded active/docked/avoiding), port markers, the moving storm circle, fading collision
markers, and a $50\times50$ weather hazard overlay (`WeatherOverlay`, plus gradient/isobar/wind-particle
layers). Panels: KPI cards, rolling vessel-to-vessel comms log, telemetry counts, a vessel-status
chart (recharts), batch controller, hypothesis-test/stats viewer, and JSONL **playback** of completed
runs.

---

## 14. Consolidated parameter table (defaults)

| Symbol / name | Value | Meaning |
|---|---|---|
| $\Delta t$ | 0.25 h | tick length |
| $T$ | 2880 | ticks per run |
| $N$ vessels | 20 (scenario-dependent) | fleet size |
| $n$ crew | 10 | crew per vessel |
| grid | $50\times 50$ | weather field |
| $\rho_{s,v,u}$ | 0.95, 0.90, 0.93 | AR(1) persistence |
| $\mu$ calm / storm | (0.30,0.80,0.25)/(0.72,0.30,0.75) | reversion targets |
| $\sigma_{s,v,u}$ | 0.07, 0.06, 0.07 | noise scale |
| $p_{cs},p_{sc}$ | 0.03, 0.08 | regime flip probs (Mixed) |
| $(w_s,w_v,w_u)$ | (1.0, 0.5, 0.8) | hazard fusion weights |
| $G_{\max}$ | 0.12 | gradient cap |
| warn radius / CPA / TTA | 20 nm / 5 nm / 12 ticks | avoidance gate |
| avoidance offset / forward | 5 nm / 3 nm | give-way waypoint |
| cooldown | 3 ticks | re-manoeuvre lockout |
| $r_{\text{coll}}$ | 0.15 nm → hit at 0.30 nm | collision radius |
| dwell | 8–48 ticks | port stay |
| fatigue rates | base 0.0015, weather 0.0040, circadian 0.0008, recovery 0.025 | §5 |
| $F^\ast, \kappa$ | 0.40, 0.65 | fatigue comms threshold/penalty |
| evac / fatal base | 0.30 / 0.04 | consequence model |
| storm comms (corridor) | 0.08 | degraded link |

---

## 15. Summary of what to compare against the literature

1. **Vessel kinematics:** waypoint-pursuit with land sub-stepping — compare to standard maritime ABM
   movement and to AIS-trajectory replay simulators.
2. **CPA/TCPA + give-way:** §8 is textbook relative-motion CPA; the lower-id give-way rule is a
   deterministic simplification of COLREGS crossing/head-on. Compare to ship collision-risk indices
   (CRI), velocity-obstacle methods, and learned COLREGS agents.
3. **Weather field:** §6 is a regime-switching AR(1) advected field with Lagrangian storm cells and
   gradient limiting — compare to stochastic spatio-temporal met-ocean generators and to simpler
   Perlin/Gaussian-random-field approaches used in other ABMs.
4. **Crew fatigue:** §5 (base + weather + cosine circadian, with recovery) — compare to SAFTE/FAST,
   the Three-Process Model of alertness, and watch-keeping fatigue studies.
5. **Consequence model:** §9 per-crew Bernoulli fatalities scaled by hazard, awareness, and fatigue —
   compare to maritime accident-consequence and evacuation models.
6. **Evaluation:** Mann–Whitney U + Holm–Bonferroni + Cliff's δ over a scenario×method×seed factorial
   — a conventional non-parametric simulation-comparison protocol; confirm best practice.

**Open questions for the research to address:** Are the AR(1) noise gain $\sigma(0.3+\mathfrak{u})$,
the hazard-fusion weights, and the linear consequence multipliers calibrated against any real data,
or are they free engineering parameters? Is there published work fusing **comms-degradation +
fatigue + CPA avoidance** in a single maritime ABM, or is that combination the genuinely novel part?
