import { useState, useMemo, Component, lazy, Suspense } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { usePlayback } from "./hooks/usePlayback";
import type { KpiSnapshot, TelemetrySnapshot, VesselMsg } from "./types";
import RunsWorkspace from "./components/RunsWorkspace";
import PathsPanel from "./components/PathsPanel";
import CommsLog from "./components/CommsLog";

// The recharts-backed panels are the only consumers of the (large) charts
// bundle and never sit on the default tab — lazy-load them so recharts is
// kept out of the initial payload.
const BatchPanel = lazy(() => import("./components/BatchPanel"));
const StatsPanel = lazy(() => import("./components/StatsPanel"));
const TelemetryPanel = lazy(() => import("./components/TelemetryPanel"));

type Tab = "runs" | "telemetry" | "comms" | "batch" | "stats" | "paths";

const TAB_CONFIG: { id: Tab; label: string; icon: string }[] = [
  { id: "runs",      label: "Runs",          icon: "◎" },
  { id: "telemetry", label: "Telemetry",     icon: "≋" },
  { id: "comms",     label: "Comms",         icon: "📡" },
  { id: "batch",     label: "Batch Results", icon: "⚙" },
  { id: "stats",     label: "Statistics",    icon: "▦" },
  { id: "paths",     label: "AIS Paths",     icon: "〜" },
];

type TelemetryPoint = TelemetrySnapshot & { step: number };

export default function App() {
  const [tab, setTab] = useState<Tab>("runs");
  const playback = usePlayback();
  const ticks = playback.ticks;

  // Telemetry / Comms are driven by the currently selected playback run.
  const kpiHistory = useMemo<KpiSnapshot[]>(
    () => ticks.map(t => t.kpis).filter(Boolean),
    [ticks],
  );
  const telemetryHistory = useMemo<TelemetryPoint[]>(
    () => ticks.filter(t => t.telemetry).map(t => ({ ...(t.telemetry as TelemetrySnapshot), step: t.step })),
    [ticks],
  );
  const commsLog = useMemo<VesselMsg[]>(() => ticks.flatMap(t => t.comms_log ?? []), [ticks]);

  const latestKpis = playback.currentTick?.kpis;
  const runName = playback.manifest
    ? `${playback.manifest.scenario} · ${playback.manifest.method} · #${playback.manifest.seed}`
    : null;

  return (
    <div style={{
      display: "flex", flexDirection: "column", height: "100vh",
      background: "#0a0f1e", color: "#f1f5f9",
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
      overflow: "hidden",
    }}>
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "8px 16px", background: "#060c18",
        borderBottom: "1px solid #1e3a5f", flexShrink: 0, minHeight: 44,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div style={{
            width: 28, height: 28, borderRadius: 6,
            background: "linear-gradient(135deg, #1d4ed8, #0ea5e9)",
            display: "flex", alignItems: "center", justifyContent: "center",
            fontSize: 14, flexShrink: 0,
          }}>⚓</div>
          <div>
            <div style={{ fontWeight: 700, fontSize: 13, letterSpacing: "-0.01em" }}>NavMesh</div>
            <div style={{ fontSize: 9, color: "#475569", letterSpacing: "0.06em", textTransform: "uppercase" }}>
              Maritime Safety Sim
            </div>
          </div>
        </div>

        <div style={{ width: 1, height: 28, background: "#1e293b", marginLeft: 4 }} />

        {latestKpis && (
          <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
            <QuickKpi label="Collisions" value={latestKpis.collision_per_1k_hrs} color="#f87171" fmt={v => v.toFixed(3)} suffix="/1k" />
            <QuickKpi label="Survival" value={latestKpis.survival_ratio} color="#34d399" fmt={v => v.toFixed(3)} />
          </div>
        )}

        <div style={{ flex: 1 }} />

        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          padding: "4px 12px",
          background: runName ? "#0c1f3a" : "#111827",
          border: `1px solid ${runName ? "#1d4ed8" : "#1e293b"}`,
          borderRadius: 20, fontSize: 11,
          color: runName ? "#93c5fd" : "#64748b", fontWeight: 600,
          maxWidth: 380, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
        }}>
          {runName ?? "No run selected"}
        </div>
      </div>

      {/* Tab bar */}
      <div style={{
        display: "flex", gap: 4,
        background: "#060c18", borderBottom: "1px solid #1e293b",
        padding: "0 12px", flexShrink: 0,
      }}>
        {TAB_CONFIG.map(({ id, label, icon }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            style={{
              padding: "10px 18px", background: "transparent",
              color: tab === id ? "#f1f5f9" : "#475569",
              border: "none",
              borderBottom: tab === id ? "2px solid #3b82f6" : "2px solid transparent",
              cursor: "pointer", fontSize: 12,
              fontWeight: tab === id ? 600 : 400,
              display: "flex", alignItems: "center", gap: 6,
              transition: "color 0.15s", letterSpacing: "0.01em",
            }}
          >
            <span style={{ fontSize: 12, color: tab === id ? "#3b82f6" : "#334155" }}>{icon}</span>
            {label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflow: "hidden", display: "flex" }}>

        {tab === "runs" && <RunsWorkspace pb={playback} />}

        {tab === "telemetry" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            {ticks.length === 0
              ? <EmptyHint text="Select a run in the Runs tab to see its telemetry." />
              : <Suspense fallback={<PanelFallback />}><TelemetryPanel kpiHistory={kpiHistory} telemetryHistory={telemetryHistory} /></Suspense>}
          </div>
        )}

        {tab === "comms" && (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            {ticks.length === 0
              ? <div style={{ padding: "24px 28px" }}><EmptyHint text="Select a run in the Runs tab to see its comms log." /></div>
              : <CommsLog messages={commsLog} />}
          </div>
        )}

        {tab === "batch" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            <Suspense fallback={<PanelFallback />}><BatchPanel /></Suspense>
          </div>
        )}

        {tab === "stats" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            <Suspense fallback={<PanelFallback />}><StatsPanel /></Suspense>
          </div>
        )}

        {tab === "paths" && (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <PathsPanel />
          </div>
        )}
      </div>
    </div>
  );
}

function EmptyHint({ text }: { text: string }) {
  return (
    <div style={{ fontSize: 12, color: "#64748b", lineHeight: 1.6, maxWidth: 480 }}>{text}</div>
  );
}

function PanelFallback() {
  return (
    <div style={{ fontSize: 12, color: "#475569", padding: "8px 0" }}>Loading…</div>
  );
}

function QuickKpi({
  label, value, color, fmt, suffix,
}: {
  label: string;
  value: number | null | undefined;
  color: string;
  fmt: (v: number) => string;
  suffix?: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-start" }}>
      <span style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.07em" }}>
        {label}
      </span>
      <span style={{ fontSize: 13, fontWeight: 700, color, fontVariantNumeric: "tabular-nums", lineHeight: 1.2 }}>
        {typeof value === "number" && isFinite(value) ? fmt(value) : "—"}
        {suffix && <span style={{ fontSize: 9, color: "#64748b", marginLeft: 2 }}>{suffix}</span>}
      </span>
    </div>
  );
}

export class AppErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null; componentStack: string | null }
> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null, componentStack: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("App render error:", error, info);
    this.setState({ componentStack: info.componentStack ?? null });
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{
          height: "100vh", display: "flex", flexDirection: "column",
          alignItems: "center", justifyContent: "center",
          background: "#0a0f1e", color: "#f1f5f9",
          fontFamily: "monospace", padding: 32, gap: 16, overflowY: "auto",
        }}>
          <div style={{ fontSize: 28 }}>⚠</div>
          <div style={{ fontWeight: 700, fontSize: 15, color: "#f87171" }}>Render error</div>
          <pre style={{
            background: "#0d1526", border: "1px solid #ef4444", borderRadius: 8,
            padding: "12px 16px", fontSize: 12, color: "#fca5a5",
            maxWidth: 800, width: "100%", overflowX: "auto", whiteSpace: "pre-wrap",
          }}>
            {this.state.error.message}{"\n\n"}{this.state.error.stack}
          </pre>
          <button
            onClick={() => this.setState({ error: null, componentStack: null })}
            style={{
              padding: "6px 18px", background: "#1e3a8a", color: "#93c5fd",
              border: "1px solid #1d4ed8", borderRadius: 6, cursor: "pointer",
              fontSize: 12, fontWeight: 600,
            }}
          >
            Dismiss & retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
