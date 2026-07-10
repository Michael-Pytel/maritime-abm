import { useState, Component, lazy, Suspense } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { usePlayback } from "./hooks/usePlayback";
import { useRunSeries } from "./hooks/useRunSeries";
import NavRail, { type Drawer } from "./components/NavRail";
import MapStage from "./components/MapStage";
import RunsPanel from "./components/RunsPanel";
import type { IslandTab } from "./components/StatsIsland";
import { color, shadow } from "./theme/tokens";

// StatisticsPanel pulls in recharts (via StatsPanel) — lazy so the charts bundle
// stays out of the initial payload; the map shell loads without it.
const StatisticsPanel = lazy(() => import("./components/StatisticsPanel"));

export default function App() {
  const pb = usePlayback();
  const series = useRunSeries(pb.ticks);
  const [drawer, setDrawer] = useState<Drawer | null>("runs");
  const [island, setIsland] = useState<{ expanded: boolean; tab: IslandTab }>({ expanded: false, tab: "telemetry" });

  const selectDrawer = (d: Drawer) => setDrawer(prev => (prev === d ? null : d));

  return (
    <div style={{
      display: "flex", height: "100vh", overflow: "hidden",
      background: color.bg, color: color.text,
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    }}>
      <NavRail active={drawer} onSelect={selectDrawer} onComms={() => setIsland({ expanded: true, tab: "comms" })} />

      <div style={{ position: "relative", flex: 1, overflow: "hidden" }}>
        {/* The map is always the stage. */}
        <MapStage pb={pb} series={series} island={island} onIsland={setIsland} />

        {/* Left drawer overlays the map; the map stays full-bleed behind it. */}
        {drawer && (
          <div style={{
            position: "absolute", top: 0, bottom: 46, left: 0, width: 340, zIndex: 4,
            background: color.panel, borderRight: `1px solid ${color.border}`,
            boxShadow: shadow, display: "flex", flexDirection: "column", overflow: "hidden",
          }}>
            {drawer === "runs"
              ? <RunsPanel pb={pb} />
              : (
                <Suspense fallback={<div style={{ padding: 16, fontSize: 12, color: color.faint }}>Loading…</div>}>
                  <StatisticsPanel pb={pb} />
                </Suspense>
              )}
          </div>
        )}
      </div>
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
