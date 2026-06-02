import { useState, useCallback, useEffect, useRef, Component } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { useSimSocket } from "./hooks/useSimSocket";
import { usePlayback } from "./hooks/usePlayback";
import type { KpiSnapshot, TelemetrySnapshot, CollisionEvent, Storm } from "./types";
import SimMap from "./components/SimMap";
import KpiPanel from "./components/KpiPanel";
import ControlPanel from "./components/ControlPanel";
import VesselStatusChart from "./components/VesselStatusChart";
import BatchPanel from "./components/BatchPanel";
import StatsPanel from "./components/StatsPanel";
import PlaybackControls from "./components/PlaybackControls";
import TelemetryPanel from "./components/TelemetryPanel";
import PathsPanel from "./components/PathsPanel";
import CommsLog from "./components/CommsLog";
import type { VesselMsg } from "./types";

const MAX_HISTORY = 300;
const EMPTY: never[] = [];
type Tab = "live" | "batch" | "stats" | "playback" | "telemetry" | "paths" | "comms";

const TAB_CONFIG: { id: Tab; label: string; icon: string }[] = [
  { id: "live",      label: "Live Simulation", icon: "⬤" },
  { id: "comms",     label: "Comms",           icon: "📡" },
  { id: "batch",     label: "Batch Runs",      icon: "⚙" },
  { id: "stats",     label: "Statistics",      icon: "▦" },
  { id: "playback",  label: "Playback",        icon: "▶" },
  { id: "telemetry", label: "Telemetry",       icon: "≋" },
  { id: "paths",     label: "AIS Paths",       icon: "〜" },
];

type TelemetryPoint = TelemetrySnapshot & { step: number };

export default function App() {
  const { tick, connected } = useSimSocket();
  const kpiHistoryRef       = useRef<KpiSnapshot[]>([]);
  const telemetryHistoryRef = useRef<TelemetryPoint[]>([]);
  const [historyVer, setHistoryVer] = useState(0);
  const [tab, setTab] = useState<Tab>("live");

  const playback = usePlayback();

  useEffect(() => {
    if (!tick) return;
    const kh = kpiHistoryRef.current;
    if (kh.length > 0 && kh[kh.length - 1].step === tick.step) return;
    const next = kh.length >= MAX_HISTORY ? kh.slice(1) : kh.slice();
    next.push(tick.kpis);
    kpiHistoryRef.current = next;

    if (tick.telemetry) {
      const th = telemetryHistoryRef.current;
      const point: TelemetryPoint = { ...tick.telemetry, step: tick.step };
      const nextTh = th.length >= MAX_HISTORY ? th.slice(1) : th.slice();
      nextTh.push(point);
      telemetryHistoryRef.current = nextTh;
    }
    setHistoryVer(v => v + 1);
  }, [tick]);

  const kpiHistory       = kpiHistoryRef.current;
  const telemetryHistory = telemetryHistoryRef.current;

  // suppress unused var warning from historyVer
  void historyVer;

  const vessels    = tick?.vessels    ?? EMPTY;
  const step       = tick?.step       ?? 0;
  const ports      = tick?.ports      ?? EMPTY;
  const latestKpis = tick?.kpis;
  const commsLog: VesselMsg[] = tick?.comms_log ?? EMPTY;
  const collisionEvents: CollisionEvent[] = tick?.collision_events ?? EMPTY;
  const storm: Storm | null = tick?.storm ?? null;
  const weatherGrid: number[] = tick?.weather_grid ?? EMPTY;
  const weatherGridSize: number = tick?.weather_grid_size ?? 0;
  const bbox = tick?.bbox;

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
            <QuickKpi label="Collisions"  value={latestKpis.collision_per_1k_hrs} color="#f87171" fmt={v => v.toFixed(3)} suffix="/1k" />
            <QuickKpi label="Avoiding"    value={tick?.telemetry?.avoiding_count ?? 0} color="#f59e0b" fmt={v => String(v)} />
          </div>
        )}

        <div style={{ flex: 1 }} />

        <div style={{
          display: "flex", alignItems: "center", gap: 6,
          padding: "4px 10px",
          background: connected ? "#052e16" : "#450a0a",
          border: `1px solid ${connected ? "#22c55e" : "#ef4444"}40`,
          borderRadius: 20, fontSize: 11,
          color: connected ? "#22c55e" : "#f87171", fontWeight: 600,
        }}>
          <span style={{
            width: 6, height: 6, borderRadius: "50%",
            background: connected ? "#22c55e" : "#ef4444",
          }} />
          {connected ? `Step ${step.toLocaleString()}` : "Disconnected"}
        </div>
      </div>

      {/* Control panel */}
      <div style={{ background: "#0d1526", borderBottom: "1px solid #1e293b", flexShrink: 0 }}>
        <ControlPanel connected={connected} step={step} />
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
            onClick={() => { setTab(id); if (id === "playback") playback.refreshRuns(); }}
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
            <span style={{ fontSize: id === "live" ? 8 : 12, color: tab === id ? "#3b82f6" : "#334155" }}>
              {icon}
            </span>
            {label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflow: "hidden", display: "flex" }}>

        {tab === "live" && (
          <div style={{ flex: 1, display: "grid", gridTemplateColumns: "1fr 300px", overflow: "hidden" }}>
            <div style={{ position: "relative", overflow: "hidden" }}>
              <SimMap
                vessels={vessels} ports={ports}
                collisionEvents={collisionEvents} currentStep={step}
                storm={storm}
                weatherGrid={weatherGrid} weatherGridSize={weatherGridSize}
                latMin={bbox?.lat_min} latMax={bbox?.lat_max}
                lonMin={bbox?.lon_min} lonMax={bbox?.lon_max}
              />
            </div>
            <div style={{
              overflowY: "auto", background: "#0d1526",
              borderLeft: "1px solid #1e293b",
              padding: "14px 12px",
              display: "flex", flexDirection: "column", gap: 14,
            }}>
              <VesselStatusChart vessels={vessels} />
              <div style={{ height: 1, background: "#1e293b" }} />
              <KpiPanel history={kpiHistory} />
            </div>
          </div>
        )}

        {tab === "batch" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            <BatchPanel />
          </div>
        )}

        {tab === "stats" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            <StatsPanel />
          </div>
        )}

        {tab === "playback" && (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <div style={{ flex: 1, display: "grid", gridTemplateColumns: "1fr 220px", overflow: "hidden" }}>
              <div style={{ position: "relative", overflow: "hidden" }}>
                <SimMap
                  vessels={playback.currentTick?.vessels ?? []}
                  ports={playback.currentTick?.ports ?? []}
                  collisionEvents={playback.currentTick?.collision_events ?? []}
                  currentStep={playback.currentTick?.step ?? 0}
                  storm={playback.currentTick?.storm ?? null}
                  weatherGrid={playback.currentTick?.weather_grid ?? []}
                  weatherGridSize={playback.currentTick?.weather_grid_size ?? 0}
                  latMin={playback.currentTick?.bbox?.lat_min}
                  latMax={playback.currentTick?.bbox?.lat_max}
                  lonMin={playback.currentTick?.bbox?.lon_min}
                  lonMax={playback.currentTick?.bbox?.lon_max}
                />
              </div>
            </div>
            <PlaybackControls
              runs={playback.runs}
              onRefreshRuns={playback.refreshRuns}
              selectedRunId={playback.selectedRunId}
              onSelectRun={playback.setSelectedRunId}
              tickCount={playback.ticks.length}
              currentIdx={playback.currentIdx}
              onSeek={playback.setCurrentIdx}
              playing={playback.playing}
              onTogglePlay={() => playback.setPlaying(p => !p)}
              speedMs={playback.speedMs}
              onSetSpeed={playback.setSpeedMs}
              stride={playback.stride}
              onSetStride={playback.setStride}
              loading={playback.loading}
              currentTick={playback.currentTick}
              weatherChannel={playback.weatherChannel}
              onSetWeatherChannel={playback.setWeatherChannel}
            />
          </div>
        )}

        {tab === "telemetry" && (
          <div style={{ flex: 1, padding: "24px 28px", overflowY: "auto", background: "#0a0f1e" }}>
            <TelemetryPanel kpiHistory={kpiHistory} telemetryHistory={telemetryHistory} />
          </div>
        )}

        {tab === "paths" && (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <PathsPanel />
          </div>
        )}

        {tab === "comms" && (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
            <CommsLog messages={commsLog} />
          </div>
        )}
      </div>
    </div>
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
