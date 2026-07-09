import { useState, useEffect, useRef, useCallback, useMemo, memo } from "react";
import { usePlayback, SPEED_PRESETS } from "../hooks/usePlayback";
import MapGL, { type RoutePath } from "./MapGL";
import type { BatchStatus, RunManifest } from "../types";

const API = "http://localhost:3000";

type PB = ReturnType<typeof usePlayback>;

const ALL_SCENARIOS = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"];
const ALL_METHODS = ["BaselineA", "BaselineB", "ProposedSystem"];
const SCEN_SHORT: Record<string, string> = {
  CalmPassage: "Calm", StormCorridor: "Storm", BlindShore: "Blind", DeepWaterRescue: "Deep",
};
const METHOD_COLORS: Record<string, string> = {
  BaselineA: "#6b7280", BaselineB: "#f59e0b", ProposedSystem: "#3b82f6",
};

/** Batch-first workspace: launch a batch, watch seeds complete, play any back. */
export default function RunsWorkspace({ pb }: { pb: PB }) {
  const [scenarios, setScenarios] = useState<string[]>([...ALL_SCENARIOS]);
  const [methods, setMethods] = useState<string[]>([...ALL_METHODS]);
  const [nSeeds, setNSeeds] = useState(10);
  const [nTicks, setNTicks] = useState(2880);
  const [status, setStatus] = useState<BatchStatus | null>(null);
  const [posting, setPosting] = useState(false);
  const [routes, setRoutes] = useState<RoutePath[]>([]);
  const [showRoutes, setShowRoutes] = useState(true);
  const [showWeather, setShowWeather] = useState(true);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const running = status?.running === true;

  // Faint AIS route network for map context (fetched once).
  useEffect(() => {
    fetch(`${API}/sim/ais-paths`)
      .then((r) => r.json() as Promise<{ name: string; vessel_type: string; waypoints: { lat: number; lon: number }[] }[]>)
      .then((data) =>
        setRoutes(
          data.map((d) => ({
            name: d.name,
            vessel_type: d.vessel_type,
            path: d.waypoints.map((w) => [w.lon, w.lat] as [number, number]),
          })),
        ),
      )
      .catch(() => {});
  }, []);

  const poll = useCallback(() => {
    fetch(`${API}/sim/batch/status`)
      .then((r) => r.json() as Promise<BatchStatus>)
      .then((s) => setStatus(s))
      .catch(() => {});
    pb.refreshRuns();
  }, [pb]);

  // While a batch is running, poll status + runs so the grid fills live.
  useEffect(() => {
    if (!running) {
      if (pollRef.current) clearInterval(pollRef.current);
      return;
    }
    pollRef.current = setInterval(poll, 2500);
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [running, poll]);

  async function launch() {
    setPosting(true);
    await fetch(`${API}/sim/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ n_seeds: nSeeds, n_ticks: nTicks, scenarios, methods }),
    }).catch(() => {});
    setPosting(false);
    // Kick an immediate poll so `running` flips on and the interval starts.
    const s = await fetch(`${API}/sim/batch/status`).then((r) => r.json() as Promise<BatchStatus>).catch(() => null);
    if (s) setStatus(s);
    pb.refreshRuns();
  }

  const toggle = (arr: string[], v: string, set: (x: string[]) => void) =>
    set(arr.includes(v) ? arr.filter((x) => x !== v) : [...arr, v]);

  // Memoised so the (potentially long) run grid does not re-render on every
  // playback frame — only when the set of runs or the selection changes.
  const playable = useMemo(() => pb.runs.filter((r) => r.has_log), [pb.runs]);
  const total = scenarios.length * methods.length * nSeeds;

  return (
    <div style={{ flex: 1, display: "grid", gridTemplateColumns: "320px 1fr", overflow: "hidden" }}>
      {/* ── Left: launcher + progress + run grid ── */}
      <div style={{ background: "#0d1526", borderRight: "1px solid #1e293b", display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <div style={{ padding: "14px 14px 12px", borderBottom: "1px solid #1e293b" }}>
          <div style={{ fontSize: 12, fontWeight: 700, color: "#e2e8f0", marginBottom: 10, letterSpacing: "0.02em" }}>
            BATCH
          </div>
          <Chips label="Scenarios" opts={ALL_SCENARIOS} sel={scenarios} onT={(v) => toggle(scenarios, v, setScenarios)} short={SCEN_SHORT} disabled={running} />
          <div style={{ height: 8 }} />
          <Chips label="Methods" opts={ALL_METHODS} sel={methods} onT={(v) => toggle(methods, v, setMethods)} colors={METHOD_COLORS} disabled={running} />
          <div style={{ display: "flex", gap: 8, marginTop: 10, alignItems: "flex-end" }}>
            <Num label="Seeds" value={nSeeds} onChange={setNSeeds} min={1} max={100} disabled={running} />
            <Num label="Ticks" value={nTicks} onChange={setNTicks} min={100} max={10000} step={100} disabled={running} />
          </div>
          <button
            onClick={launch}
            disabled={posting || running || total === 0}
            style={{
              width: "100%", marginTop: 10, padding: "8px 0",
              background: posting || running || total === 0 ? "#1e293b" : "#1d4ed8",
              color: posting || running || total === 0 ? "#475569" : "#fff",
              border: `1px solid ${running ? "#334155" : "#3b82f6"}`, borderRadius: 7,
              cursor: posting || running ? "default" : "pointer", fontSize: 12, fontWeight: 600,
            }}
          >
            {posting ? "Starting…" : running ? "Running in background…" : `▶ Run ${total} sims`}
          </button>

          {status && (
            <div style={{ marginTop: 10 }}>
              <div style={{ display: "flex", justifyContent: "space-between", fontSize: 10, color: "#94a3b8", marginBottom: 4 }}>
                <span>{status.completed}/{status.total} done{(status.failed ?? 0) > 0 ? ` · ${status.failed} failed` : ""}</span>
                <span style={{ color: running ? "#60a5fa" : "#22c55e", fontWeight: 700 }}>{status.progress_pct.toFixed(0)}%</span>
              </div>
              <div style={{ background: "#1e293b", borderRadius: 4, height: 6, overflow: "hidden" }}>
                <div style={{ height: "100%", width: `${status.progress_pct}%`, background: running ? "linear-gradient(90deg,#3b82f6,#60a5fa)" : "#22c55e", transition: "width .4s" }} />
              </div>
            </div>
          )}
        </div>

        {/* Run grid — fills as seeds complete */}
        <div style={{ flex: 1, overflowY: "auto", padding: "10px 12px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
            <span style={{ fontSize: 10, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em", fontWeight: 700 }}>
              Completed runs · {playable.length}
            </span>
            <button onClick={poll} title="Refresh" style={{ background: "none", border: "none", color: "#475569", cursor: "pointer", fontSize: 12 }}>⟳</button>
          </div>
          {playable.length === 0 && (
            <div style={{ fontSize: 11, color: "#475569", lineHeight: 1.6, padding: "8px 0" }}>
              No completed runs yet. Launch a batch — each seed becomes playable here the moment it finishes, while the rest keep computing.
            </div>
          )}
          <RunGrid playable={playable} selectedRunId={pb.selectedRunId} onSelect={pb.setSelectedRunId} />
        </div>
      </div>

      {/* ── Right: GL map + scrubber ── */}
      <div style={{ display: "flex", flexDirection: "column", overflow: "hidden" }}>
        <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
          <MapGL
            vessels={pb.currentTick?.vessels ?? []}
            ports={pb.currentTick?.ports ?? []}
            routes={routes}
            showRoutes={showRoutes}
            collisionEvents={pb.currentTick?.collision_events ?? []}
            currentStep={pb.currentTick?.step ?? 0}
            rescueAgents={pb.currentTick?.rescue_agents ?? []}
            wrecks={pb.currentTick?.wrecks ?? []}
            mobPersons={pb.currentTick?.mob_persons ?? []}
            mobAgents={pb.currentTick?.mob_agents ?? []}
            storm={pb.currentTick?.storm ?? null}
            weatherGrid={pb.currentTick?.weather_grid ?? []}
            weatherGridSize={pb.currentTick?.weather_grid_size ?? 0}
            showWeather={showWeather}
            latMin={pb.currentTick?.bbox?.lat_min}
            latMax={pb.currentTick?.bbox?.lat_max}
            lonMin={pb.currentTick?.bbox?.lon_min}
            lonMax={pb.currentTick?.bbox?.lon_max}
            transitionMs={pb.speedMs}
          />
          {!pb.selectedRunId && (
            <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", pointerEvents: "none" }}>
              <div style={{ background: "#0d1526cc", border: "1px solid #1e3a5f", borderRadius: 10, padding: "12px 18px", fontSize: 12, color: "#94a3b8" }}>
                Select a completed run to play it back
              </div>
            </div>
          )}
          <div style={{ position: "absolute", top: 10, right: 10, zIndex: 2, display: "flex", gap: 6 }}>
            <LayerToggle on={showWeather} onClick={() => setShowWeather((s) => !s)} activeColor="#fbbf24" label="⛈ Weather" />
            <LayerToggle on={showRoutes} onClick={() => setShowRoutes((s) => !s)} activeColor="#60a5fa" label="〜 Routes" />
          </div>
        </div>

        <KpiStrip pb={pb} />

        {/* Scrubber */}
        <Scrubber pb={pb} />
      </div>
    </div>
  );
}

/** Memoised list of run cards — re-renders only when runs/selection change,
 *  not on every playback frame. */
const RunGrid = memo(function RunGrid({
  playable, selectedRunId, onSelect,
}: {
  playable: RunManifest[];
  selectedRunId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      {playable.map((r) => (
        <RunCard key={r.run_id} run={r} selected={selectedRunId === r.run_id} onSelect={() => onSelect(r.run_id)} />
      ))}
    </div>
  );
});

function RunCard({ run, selected, onSelect }: { run: RunManifest; selected: boolean; onSelect: () => void }) {
  const col = METHOD_COLORS[run.method] ?? "#3b82f6";
  const coll = run.kpis?.collision_per_1k_hrs;
  return (
    <button
      onClick={onSelect}
      style={{
        display: "flex", alignItems: "center", gap: 8, width: "100%", textAlign: "left",
        padding: "7px 9px", borderRadius: 7, cursor: "pointer",
        background: selected ? "#12233f" : "#0a1220",
        border: `1px solid ${selected ? "#3b82f6" : "#1e293b"}`,
      }}
    >
      <span style={{ width: 6, height: 6, borderRadius: "50%", background: col, flexShrink: 0 }} />
      <span style={{ fontSize: 11, fontWeight: 600, color: "#e2e8f0", minWidth: 40 }}>{SCEN_SHORT[run.scenario] ?? run.scenario}</span>
      <span style={{ fontSize: 10, color: col }}>{run.method.replace("Baseline", "B-").replace("ProposedSystem", "Proposed")}</span>
      <span style={{ fontSize: 10, color: "#475569" }}>#{run.seed}</span>
      <span style={{ flex: 1 }} />
      {typeof coll === "number" && (
        <span style={{ fontSize: 10, color: "#94a3b8", fontVariantNumeric: "tabular-nums" }} title="collisions / 1k hrs">
          {coll.toFixed(2)}
        </span>
      )}
    </button>
  );
}

function KpiStrip({ pb }: { pb: PB }) {
  const k = pb.manifest?.kpis;
  if (!k) return null;
  const items: { label: string; value: string; color: string }[] = [
    { label: "Collisions /1k", value: k.collision_per_1k_hrs.toFixed(3), color: "#f87171" },
    { label: "Fatal /1k", value: k.fatal_per_1k_hrs.toFixed(3), color: "#fb7185" },
    { label: "Survival", value: k.survival_ratio.toFixed(3), color: "#34d399" },
    { label: "Avg TTA (h)", value: k.avg_tta_hours.toFixed(2), color: "#60a5fa" },
    { label: "Evac rate", value: k.evac_activation_rate.toFixed(3), color: "#fbbf24" },
  ];
  return (
    <div style={{ display: "flex", gap: 22, padding: "8px 16px", background: "#0a1220", borderTop: "1px solid #1e293b", flexShrink: 0 }}>
      {items.map((it) => (
        <div key={it.label} style={{ display: "flex", flexDirection: "column" }}>
          <span style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.06em" }}>{it.label}</span>
          <span style={{ fontSize: 13, fontWeight: 700, color: it.color, fontVariantNumeric: "tabular-nums" }}>{it.value}</span>
        </div>
      ))}
      <span style={{ flex: 1 }} />
      <span style={{ fontSize: 9, color: "#334155", alignSelf: "center" }}>final-tick KPIs</span>
    </div>
  );
}

function Scrubber({ pb }: { pb: PB }) {
  const n = pb.ticks.length;
  const step = pb.currentTick?.step ?? 0;
  return (
    <div style={{ background: "#060c18", borderTop: "1px solid #1e293b", padding: "8px 14px", display: "flex", alignItems: "center", gap: 12, flexShrink: 0 }}>
      <button
        onClick={() => pb.setPlaying((p) => !p)}
        disabled={n === 0}
        style={{ width: 30, height: 30, borderRadius: 6, background: n === 0 ? "#1e293b" : "#1d4ed8", color: n === 0 ? "#475569" : "#fff", border: "none", cursor: n === 0 ? "default" : "pointer", fontSize: 12 }}
      >{pb.playing ? "❚❚" : "▶"}</button>
      <input
        type="range" min={0} max={Math.max(0, n - 1)} value={pb.currentIdx}
        onChange={(e) => pb.setCurrentIdx(Number(e.target.value))}
        disabled={n === 0}
        style={{ flex: 1, accentColor: "#3b82f6" }}
      />
      <span style={{ fontSize: 11, color: "#94a3b8", fontVariantNumeric: "tabular-nums", minWidth: 90, textAlign: "right" }}>
        {pb.loading ? "loading…" : n === 0 ? "—" : `tick ${step} · ${pb.currentIdx + 1}/${n}`}
      </span>
      <select
        value={pb.speedMs}
        onChange={(e) => pb.setSpeedMs(Number(e.target.value))}
        style={{ background: "#1e293b", color: "#cbd5e1", border: "1px solid #334155", borderRadius: 6, fontSize: 10, padding: "3px 6px", cursor: "pointer" }}
      >
        {SPEED_PRESETS.map((s) => <option key={s.label} value={s.ms}>{s.label}</option>)}
      </select>
    </div>
  );
}

function LayerToggle({ on, onClick, activeColor, label }: { on: boolean; onClick: () => void; activeColor: string; label: string }) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: "5px 10px", fontSize: 10, fontWeight: 600,
        background: "#0d1526cc", color: on ? activeColor : "#64748b",
        border: "1px solid #1e3a5f", borderRadius: 6, cursor: "pointer",
      }}
    >{label}</button>
  );
}

function Chips({ label, opts, sel, onT, colors, short, disabled }: {
  label: string; opts: string[]; sel: string[]; onT: (v: string) => void;
  colors?: Record<string, string>; short?: Record<string, string>; disabled?: boolean;
}) {
  return (
    <div>
      <div style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 5, fontWeight: 700 }}>{label}</div>
      <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
        {opts.map((o) => {
          const active = sel.includes(o);
          const c = colors?.[o] ?? "#3b82f6";
          return (
            <button key={o} onClick={() => !disabled && onT(o)} style={{
              padding: "3px 9px", background: active ? `${c}22` : "#1e293b",
              color: active ? c : "#64748b", border: `1px solid ${active ? c : "#334155"}`,
              borderRadius: 14, cursor: disabled ? "default" : "pointer", fontSize: 10,
              fontWeight: active ? 600 : 400, opacity: disabled ? 0.55 : 1,
            }}>{short?.[o] ?? o}</button>
          );
        })}
      </div>
    </div>
  );
}

function Num({ label, value, onChange, min, max, step, disabled }: {
  label: string; value: number; onChange: (v: number) => void; min?: number; max?: number; step?: number; disabled?: boolean;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
      <span style={{ fontSize: 9, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>{label}</span>
      <input type="number" value={value} min={min} max={max} step={step ?? 1} disabled={disabled}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ padding: "5px 8px", background: "#1e293b", color: "#f1f5f9", border: "1px solid #334155", borderRadius: 5, fontSize: 12, width: 68, outline: "none" }} />
    </div>
  );
}
