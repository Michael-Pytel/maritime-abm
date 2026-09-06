# Architecture

Batch-only execution: tools produce committed `data/`, the Rust engine runs factorial batches via the Axum API, and the React workbench plays back JSONL tick logs.

```mermaid
flowchart TB
  subgraph tools [tools and aisprocess]
    fetchers["tools/*.py\nEMODnet / Digitraffic"]
    genRoutes["aisprocess gen_routes\ndepth + lane A*"]
    fetchers --> bins["data/*.bin.gz"]
    bins --> genRoutes
    genRoutes --> aisJson["data/ais_paths.json\ndata/ports.json"]
  end

  subgraph engine [crates/simulation]
    sim["SimStateWrapper.run_blocking\npost_tick pipeline"]
    kpi["KpiAccumulator"]
    sim --> kpi
  end

  aisJson --> sim
  coastline["data/coastline.geojson"] --> sim

  subgraph api [crates/api]
    batch["POST /sim/batch\nRayon spawn_batch"]
    sse["GET /sim/batch/events\nSSE progress"]
    db["outputs/results.db\nretained KPIs"]
    playback["outputs/*.jsonl\nlatest-batch playback"]
    batch --> sim
    batch --> db
    batch --> playback
    batch --> sse
  end

  subgraph ui [frontend]
    runs["RunsPanel\nlaunch + EventSource"]
    map["deck.gl MapGL\nscrub JSONL"]
    stats["Stats / hypothesis"]
    runs --> batch
    runs --> sse
    map --> playback
    stats --> db
  end
```

## Storage tiers

| Tier | Artifacts | Lifetime |
|---|---|---|
| Playback | `outputs/<run>.jsonl`, `*_manifest.json` | **Latest batch only** (cleared on next batch) |
| Results | `outputs/results.db`, parquet export | **Retained** across batches for sweeps |

`config_override` deep-merges onto scenario defaults; scenario / method / seed are always re-pinned.

## Tick causality (summary)

```mermaid
flowchart LR
  W["Weather W"] --> F["Fatigue"]
  F --> C["p_comms = env × fatigue"]
  C --> A["CPA / COLREGs give-way"]
  A -->|fail| S["SIMCOL damage"]
  S --> R["SAR / MOB"]
  R --> K["KPIs"]
```

Full ordered list: [simulation.md — tick pipeline](simulation.md#tick-pipeline).
