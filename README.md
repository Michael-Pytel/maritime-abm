# Maritime ABM

Agent-based model of maritime traffic with a Rust backend and React frontend.

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 20+

## Running locally

Start the backend and frontend in separate terminals:

```bash
# Terminal 1 — API server 
cargo run -p api --release

# Terminal 2 — frontend dev server 
cd frontend
npm install
npm run dev
```

Then open [http://localhost:5173](http://localhost:5173).

If you have `make` installed you can run both with:

```bash
make dev
```

## Checks

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Frontend
cd frontend && npm run build && npm run lint
```
