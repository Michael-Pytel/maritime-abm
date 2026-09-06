# Contributing

Thanks for working on maritime-abm. This file covers **how to develop and land changes**.
For system behaviour and APIs, start at [docs/](docs/README.md).

## Workflow

1. Branch from `main`.
2. Keep the engine deterministic (see below).
3. Match CI locally before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cd frontend && npm run build && npm run lint
```

If a `Makefile` is present, `make check` / `make test` wrap the same bar.

## Where to change what

| Goal | Start here |
|---|---|
| Experiment knobs / scenarios | [`crates/simulation/src/scenario.rs`](crates/simulation/src/scenario.rs) |
| Per-tick physics | [`crates/simulation/src/state.rs`](crates/simulation/src/state.rs) — **only** `SimState::post_tick` |
| Snapshot / UI fields | `build_snapshot` **and** [`frontend/src/types.ts`](frontend/src/types.ts) |
| Batch / stats / DB | [`crates/api/`](crates/api/) |
| Routes / bathymetry | [`crates/aisprocess/`](crates/aisprocess/), [`tools/`](tools/) |
| Paper / figures | [`report/`](report/), [`tools/plot_results.py`](tools/plot_results.py) |

## Conventions

- **Determinism is sacred.** One seeded `SmallRng` per `SimState`. New stochastic behaviour must draw from `self.rng`. Guarded by `test_determinism_same_seed`.
- **One tick body.** All per-tick physics lives in `SimState::post_tick`. Live and batch paths were unified; do not add a second place things happen.
- **Never silently clobber `data/`.** It is the committed calibration baseline. `gen_routes` defaults to those paths — pass `--out` / `--ports-out` to scratch until adoption is decided.
- **`frontend/src/types.ts` is a contract.** Add a snapshot field in Rust? Mirror it in TypeScript (and consume it) or the map/panels will not see it.
- **`config_override` deep-merges** over scenario defaults (including nested `simcol` / `sar` / `forcefield`), but scenario / method / seed are always re-pinned.
- **Exhaustive `match` on Rust enums** — use a `never` default so new variants fail at compile time.

## Tests worth knowing

```bash
cargo test                                              # fast unit + integration
cargo test -p simulation --test full_run                # determinism / smoke
cargo test -p simulation --test calibration -- --include-ignored   # slow guard
cargo test -p api                                       # DB / parquet helpers
```

## Report build

```bash
pwsh report/build.ps1    # → report/build_latex/report.pdf
```

The paper builds into `report/build_latex/`. A stale `report/report.aux` at the repo-relative root can shadow labels — `build.ps1` clears those artifacts.

## Pull requests

- Prefer small, reviewable diffs; keep docs in sync when behaviour or APIs change.
- Mention which docs page you updated (if any).
- Do not commit `outputs/`, secrets, or regenerated `data/` unless the PR is explicitly adopting a new calibration baseline.
