# Maritime ABM

Tick-based **agent-based model of maritime traffic safety** on the Baltic and North Sea.

Vessels follow depth- and lane-constrained routes, resolve encounters with a CPA/COLREGs
give-way rule gated by weather × crew fatigue, and — on collision — feed DTU SIMCOL damage
plus an IAMSAR/MASSIM + Ashrafi SAR chain. Batches compare methods and scenarios with
non-parametric hypothesis tests.

| Layer | Stack |
|---|---|
| Engine | Rust + [krABMaga](https://krabmaga.github.io/) |
| API | Axum (batch + SSE progress) |
| UI | React 19 + Vite + deck.gl / MapLibre |
| Paper | LaTeX in [`report/`](report/) |

## Quick start

```bash
# Terminal 1 — API (from repo root; reads data/, writes outputs/)
cargo run -p api --release

# Terminal 2 — UI
cd frontend && npm install && npm run dev
```

Open http://localhost:5173. Launch a small batch under **Runs**, pick a finished seed, scrub the map.

**Prerequisites:** Rust (stable), Node.js 20+. Optional: Python 3.11+ for `tools/`, LaTeX for the paper.

## Repository layout

```
maritime-abm/
├── crates/simulation/   # ABM engine
├── crates/api/          # HTTP batch API + SSE
├── crates/aisprocess/   # depth/lane route generator
├── frontend/            # playback workbench
├── data/                # committed calibration inputs
├── tools/               # fetchers, sweep, plotting
├── docs/                # implementation docs
├── report/              # academic write-up
└── outputs/             # gitignored batch artifacts
```

`data/` is the calibration baseline — do not overwrite casually. See [docs/data-and-routes.md](docs/data-and-routes.md).

## Legacy

An earlier Python / Streamlit prototype lives in the archived repo
[Pawlo77/maritime-safety-simulation](https://github.com/Pawlo77/maritime-safety-simulation).
This Rust + React codebase replaces it.

## Documentation

| Doc | Contents |
|---|---|
| [docs/](docs/README.md) | Implementation docs index |
| [docs/architecture.md](docs/architecture.md) | System architecture (Mermaid) |
| [docs/experiment.md](docs/experiment.md) | Scenarios, methods, hypotheses |
| [docs/simulation.md](docs/simulation.md) | Tick pipeline, modules, KPIs |
| [docs/api.md](docs/api.md) | HTTP API reference |
| [docs/frontend.md](docs/frontend.md) | UI shell & contracts |
| [docs/sweeping.md](docs/sweeping.md) | Hyperparameter sweeps |
| [docs/data-and-routes.md](docs/data-and-routes.md) | Route generation & data tools |
| [docs/glossary.md](docs/glossary.md) | Domain terms & model citations |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev workflow, conventions, CI |
| [report/report.pdf](report/report.pdf) | Scientific narrative & results |

## License / academic use

Course / research project at Warsaw University of Technology. See authors in [`report/report.pdf`](report/report.pdf).
