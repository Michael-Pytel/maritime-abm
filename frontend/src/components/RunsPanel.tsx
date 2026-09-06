import { useState, useEffect, useCallback, useMemo, memo } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import type { BatchStatus, RunManifest } from "../types";
import {
  color, radius, METHOD_COLORS, SCEN_SHORT, methodShort,
  ALL_SCENARIOS, ALL_METHODS,
} from "../theme/tokens";

const API = "http://localhost:3000";
type PB = ReturnType<typeof usePlayback>;

/** Left-drawer "Runs" panel: launch a batch and pick a completed seed to play. */
export default function RunsPanel({ pb }: { pb: PB }) {
  const [scenarios, setScenarios] = useState<string[]>([...ALL_SCENARIOS]);
  const [methods, setMethods] = useState<string[]>([...ALL_METHODS]);
  const [nSeeds, setNSeeds] = useState(10);
  const [nTicks, setNTicks] = useState(2880);
  const [status, setStatus] = useState<BatchStatus | null>(null);
  const [posting, setPosting] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [overrideText, setOverrideText] = useState("");

  const running = status?.running === true;

  const override = parseOverride(overrideText);
  const overrideErr = overrideText.trim() && override === null ? "Invalid JSON object" : null;
  const labelPreview = override && Object.keys(override).length ? deriveLabel(override) : "default";

  const refresh = useCallback(() => {
    fetch(`${API}/sim/batch/status`).then(r => r.json() as Promise<BatchStatus>).then(setStatus).catch(() => {});
    pb.refreshRuns();
  }, [pb]);

  // SSE push for batch progress (replaces 2.5 s polling). Always connected so a
  // late-open tab still sees an in-flight batch; falls back to one-shot refresh.
  useEffect(() => {
    let es: EventSource | null = null;
    try {
      es = new EventSource(`${API}/sim/batch/events`);
      es.onmessage = (ev) => {
        try {
          const s = JSON.parse(ev.data) as BatchStatus & { status?: string };
          if (s.status === "no_batch" && s.running !== true) return;
          if (typeof s.total === "number") setStatus(s);
          if (s.running === false || (typeof s.completed === "number" && s.completed > 0)) {
            pb.refreshRuns();
          }
        } catch { /* ignore malformed frames */ }
      };
      es.onerror = () => { /* EventSource auto-reconnects */ };
    } catch {
      refresh();
    }
    return () => { es?.close(); };
  }, [pb, refresh]);

  async function launch() {
    if (overrideErr) return;
    setPosting(true);
    await fetch(`${API}/sim/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        n_seeds: nSeeds, n_ticks: nTicks, scenarios, methods,
        ...(override && Object.keys(override).length ? { config_override: override } : {}),
      }),
    }).catch(() => {});
    setPosting(false);
    const s = await fetch(`${API}/sim/batch/status`).then(r => r.json() as Promise<BatchStatus>).catch(() => null);
    if (s) setStatus(s);
    pb.refreshRuns();
  }

  const toggle = (arr: string[], v: string, set: (x: string[]) => void) =>
    set(arr.includes(v) ? arr.filter(x => x !== v) : [...arr, v]);

  const playable = useMemo(() => pb.runs.filter(r => r.has_log), [pb.runs]);
  const total = scenarios.length * methods.length * nSeeds;
  const disabled = posting || running || total === 0 || overrideErr !== null;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      {/* Launcher */}
      <div style={{ padding: "14px 14px 12px", borderBottom: `1px solid ${color.border}` }}>
        <SectionLabel>Launch batch</SectionLabel>
        <div style={{ height: 8 }} />
        <Chips label="Scenarios" opts={ALL_SCENARIOS} sel={scenarios} onT={v => toggle(scenarios, v, setScenarios)} short={SCEN_SHORT} disabled={running} />
        <div style={{ height: 8 }} />
        <Chips label="Methods" opts={ALL_METHODS} sel={methods} onT={v => toggle(methods, v, setMethods)} colors={METHOD_COLORS} disabled={running} />
        <div style={{ display: "flex", gap: 8, marginTop: 10, alignItems: "flex-end" }}>
          <Num label="Seeds" value={nSeeds} onChange={setNSeeds} min={1} max={100} disabled={running} />
          <Num label="Ticks" value={nTicks} onChange={setNTicks} min={100} max={10000} step={100} disabled={running} />
        </div>
        <button
          onClick={launch}
          disabled={disabled}
          style={{
            width: "100%", marginTop: 10, padding: "8px 0",
            background: disabled ? color.border : color.accentDeep,
            color: disabled ? color.faint : "#fff",
            border: `1px solid ${running ? color.hair : color.accent}`, borderRadius: radius.sm,
            cursor: posting || running ? "default" : "pointer", fontSize: 12, fontWeight: 600,
          }}
        >
          {posting ? "Starting…" : running ? "Running in background…" : `▶ Run ${total} sims`}
        </button>

        {status && (
          <div style={{ marginTop: 10 }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 10, color: color.sub, marginBottom: 4 }}>
              <span>{status.completed}/{status.total} done{(status.failed ?? 0) > 0 ? ` · ${status.failed} failed` : ""}</span>
              <span style={{ color: running ? "#60a5fa" : "#22c55e", fontWeight: 700 }}>{status.progress_pct.toFixed(0)}%</span>
            </div>
            <div style={{ background: color.border, borderRadius: 4, height: 6, overflow: "hidden" }}>
              <div style={{ height: "100%", width: `${status.progress_pct}%`, background: running ? "linear-gradient(90deg,#3b82f6,#60a5fa)" : "#22c55e", transition: "width .4s" }} />
            </div>
          </div>
        )}

        {/* Advanced — parameter override (sweep) + export */}
        <div style={{ marginTop: 12 }}>
          <button onClick={() => setShowAdvanced(v => !v)} style={{
            background: "none", border: "none", cursor: "pointer", padding: 0,
            color: color.muted, fontSize: 10, fontWeight: 700, textTransform: "uppercase",
            letterSpacing: "0.08em", display: "flex", alignItems: "center", gap: 6,
          }}>
            {showAdvanced ? "▾" : "▸"} Advanced
          </button>
          {showAdvanced && (
            <div style={{ marginTop: 8 }}>
              <div style={{ fontSize: 10, color: color.muted, marginBottom: 6, lineHeight: 1.5 }}>
                Partial <code style={{ color: color.sub }}>SimConfig</code> JSON, deep-merged over scenario defaults for every run in the batch (sweep).
              </div>
              <textarea
                value={overrideText}
                onChange={e => setOverrideText(e.target.value)}
                spellCheck={false}
                placeholder={'{ "collision_warn_cpa_nm": 0.5 }'}
                rows={3}
                style={{
                  width: "100%", boxSizing: "border-box", padding: "7px 9px",
                  background: color.bg, color: "#e2e8f0",
                  border: `1px solid ${overrideErr ? "#b91c1c" : color.hair}`,
                  borderRadius: radius.sm, fontSize: 11, fontFamily: "ui-monospace, monospace",
                  outline: "none", resize: "vertical",
                }}
              />
              <div style={{ fontSize: 10, color: overrideErr ? color.bad : color.faint, marginTop: 4 }}>
                {overrideErr ?? `label: ${labelPreview}`}
              </div>
              <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
                <button onClick={() => download(`${API}/sim/batch/export.parquet`)} style={exportBtn}>⬇ Batch .parquet</button>
                <button onClick={() => download(`${API}/sim/batch/export.parquet?all=true`)} style={exportBtn}>⬇ Sweep .parquet</button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Completed runs */}
      <div style={{ flex: 1, overflowY: "auto", padding: "10px 12px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
          <SectionLabel>Completed runs · {playable.length}</SectionLabel>
          <button onClick={refresh} title="Refresh" style={{ background: "none", border: "none", color: color.faint, cursor: "pointer", fontSize: 12 }}>⟳</button>
        </div>
        {playable.length === 0 && (
          <div style={{ fontSize: 11, color: color.faint, lineHeight: 1.6, padding: "8px 0" }}>
            No completed runs yet. Launch a batch — each seed becomes playable here the moment it finishes.
          </div>
        )}
        <RunGrid playable={playable} selectedRunId={pb.selectedRunId} onSelect={pb.setSelectedRunId} />
      </div>
    </div>
  );
}

const RunGrid = memo(function RunGrid({
  playable, selectedRunId, onSelect,
}: {
  playable: RunManifest[];
  selectedRunId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      {playable.map(r => (
        <RunCard key={r.run_id} run={r} selected={selectedRunId === r.run_id} onSelect={() => onSelect(r.run_id)} />
      ))}
    </div>
  );
});

function RunCard({ run, selected, onSelect }: { run: RunManifest; selected: boolean; onSelect: () => void }) {
  const col = METHOD_COLORS[run.method] ?? color.accent;
  const coll = run.kpis?.collision_per_1k_hrs;
  const nc = run.iwrap_nc_per_year;
  return (
    <button
      onClick={onSelect}
      style={{
        display: "flex", alignItems: "center", gap: 8, width: "100%", textAlign: "left",
        padding: "7px 9px", borderRadius: radius.sm, cursor: "pointer",
        background: selected ? color.panelSel : "#0a1220",
        border: `1px solid ${selected ? color.accent : color.border}`,
      }}
    >
      <span style={{ width: 6, height: 6, borderRadius: "50%", background: col, flexShrink: 0 }} />
      <span style={{ fontSize: 11, fontWeight: 600, color: "#e2e8f0", minWidth: 40 }}>{SCEN_SHORT[run.scenario] ?? run.scenario}</span>
      <span style={{ fontSize: 10, color: col }}>{methodShort(run.method)}</span>
      <span style={{ fontSize: 10, color: color.faint }}>#{run.seed}</span>
      <span style={{ flex: 1 }} />
      {typeof nc === "number" && nc > 0 && (
        <span style={{ fontSize: 10, color: color.muted, fontVariantNumeric: "tabular-nums" }} title="IWRAP N_c / yr">
          Nc {nc.toFixed(3)}
        </span>
      )}
      {typeof coll === "number" && (
        <span style={{ fontSize: 10, color: color.sub, fontVariantNumeric: "tabular-nums" }} title="collisions / 1k hrs">
          {coll.toFixed(2)}
        </span>
      )}
    </button>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <span style={{ fontSize: 10, color: color.faint, textTransform: "uppercase", letterSpacing: "0.08em", fontWeight: 700 }}>
      {children}
    </span>
  );
}

function Chips({ label, opts, sel, onT, colors, short, disabled }: {
  label: string; opts: string[]; sel: string[]; onT: (v: string) => void;
  colors?: Record<string, string>; short?: Record<string, string>; disabled?: boolean;
}) {
  return (
    <div>
      <div style={{ fontSize: 9, color: color.faint, textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 5, fontWeight: 700 }}>{label}</div>
      <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
        {opts.map(o => {
          const active = sel.includes(o);
          const c = colors?.[o] ?? color.accent;
          return (
            <button key={o} onClick={() => !disabled && onT(o)} style={{
              padding: "3px 9px", background: active ? `${c}22` : color.border,
              color: active ? c : color.muted, border: `1px solid ${active ? c : color.hair}`,
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
      <span style={{ fontSize: 9, color: color.muted, textTransform: "uppercase", letterSpacing: "0.06em" }}>{label}</span>
      <input type="number" value={value} min={min} max={max} step={step ?? 1} disabled={disabled}
        onChange={e => onChange(Number(e.target.value))}
        style={{ padding: "5px 8px", background: color.border, color: color.text, border: `1px solid ${color.hair}`, borderRadius: 5, fontSize: 12, width: 68, outline: "none" }} />
    </div>
  );
}

// ── override + export helpers (salvaged from the removed BatchPanel) ──────────

/** Parses the override textarea into a plain object, or null if invalid; {} if empty. */
function parseOverride(text: string): Record<string, unknown> | null {
  const t = text.trim();
  if (!t) return {};
  try {
    const v: unknown = JSON.parse(t);
    if (v && typeof v === "object" && !Array.isArray(v)) return v as Record<string, unknown>;
    return null;
  } catch {
    return null;
  }
}

/** Mirrors the server's derive_label: flattens nested keys to `a.b=v_c=w`. */
function deriveLabel(obj: Record<string, unknown>): string {
  const parts: string[] = [];
  const walk = (prefix: string, v: unknown) => {
    if (v && typeof v === "object" && !Array.isArray(v)) {
      for (const [k, val] of Object.entries(v)) walk(prefix ? `${prefix}.${k}` : k, val);
    } else {
      parts.push(`${prefix}=${typeof v === "string" ? v : JSON.stringify(v)}`);
    }
  };
  walk("", obj);
  return parts.length ? parts.join("_").slice(0, 120) : "default";
}

/** Triggers a browser download; the endpoint sets Content-Disposition. */
function download(url: string) {
  const a = document.createElement("a");
  a.href = url;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
}

const exportBtn: React.CSSProperties = {
  padding: "5px 10px", background: color.panel2, color: color.sub,
  border: `1px solid ${color.hair}`, borderRadius: radius.sm, cursor: "pointer", fontSize: 10, fontWeight: 600,
};
