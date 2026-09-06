# Frontend (`frontend/`)

React 19 + Vite + TypeScript. Map = **deck.gl** over a keyless CARTO-dark **MapLibre** basemap; charts = **recharts**. Batch-first playback workbench (no Live tab).

## Shell

[`App.tsx`](../frontend/src/App.tsx): permanent **left panel** + full-bleed map.

| Area | File | Role |
|---|---|---|
| Left panel | [`LeftPanel.tsx`](../frontend/src/components/LeftPanel.tsx) | Tabs: Runs / Statistics / Telemetry / Comms + pinned dock |
| Runs | [`RunsPanel.tsx`](../frontend/src/components/RunsPanel.tsx) | Launch batch; **EventSource** progress; run grid |
| Map | [`MapGL.tsx`](../frontend/src/components/MapGL.tsx) | Vessels, ports, collisions, wrecks, SAR/MOB, storm, weather, AIS faint routes |
| Map chrome | [`MapPane.tsx`](../frontend/src/components/MapPane.tsx), [`MapControlBar.tsx`](../frontend/src/components/MapControlBar.tsx) | Camera + layer toggles |
| Stats | [`StatsPanel.tsx`](../frontend/src/components/StatsPanel.tsx), [`StatisticsPanel.tsx`](../frontend/src/components/StatisticsPanel.tsx) | Hypothesis + box plots / run summary |
| Telemetry | [`TelemetryPanel.tsx`](../frontend/src/components/TelemetryPanel.tsx) | KPI curves vs scrubber |
| Comms | [`CommsLog.tsx`](../frontend/src/components/CommsLog.tsx) | Messages at current tick |
| Playback | [`PlaybackDock.tsx`](../frontend/src/components/PlaybackDock.tsx) | Play / pause / speed / scrub |

## Hooks & contract

- [`usePlayback.ts`](../frontend/src/hooks/usePlayback.ts) — list runs, load JSONL, scrubber  
- [`useRunSeries.ts`](../frontend/src/hooks/useRunSeries.ts) — derive series from `ticks[]`  
- [`types.ts`](../frontend/src/types.ts) — **backend contract** (`TickMessage`, KPIs, manifests). Keep in sync with `SimState::build_snapshot`.  
- [`theme/tokens.ts`](../frontend/src/theme/tokens.ts) — palette, method colours, KPI labels  

Code-splitting: deck.gl `gl` chunk and recharts `charts` chunk ([`vite.config.ts`](../frontend/vite.config.ts)); chart tabs are `React.lazy`.

## Dev

```bash
cd frontend && npm install && npm run dev   # http://localhost:5173
npm run build && npm run lint               # CI bar
```

API base URL defaults to `http://localhost:3000`.
