import { useState, useRef, useEffect, useMemo, Component } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { usePlayback } from "./hooks/usePlayback";
import { useRunSeries } from "./hooks/useRunSeries";
import LeftPanel, { type LeftTab } from "./components/LeftPanel";
import MapPane from "./components/MapPane";
import MapControlBar, { type MapControls } from "./components/MapControlBar";
import type { MapHandle, RoutePath } from "./components/MapGL";
import { color, type RouteFilter } from "./theme/tokens";

const API = "http://localhost:3000";

/**
 * App shell: a permanent left panel (tabs + pinned playback dock) beside a
 * full-height, chrome-free deck.gl map. The map is shifted fully to the right;
 * every control lives in the left panel.
 */
export default function App() {
  const pb = usePlayback();
  const series = useRunSeries(pb.ticks);
  const [tab, setTab] = useState<LeftTab>("runs");
  const mapRef = useRef<MapHandle>(null);

  // Map layer state lives here so both the left-panel controls and the map read it.
  const [routes, setRoutes] = useState<RoutePath[]>([]);
  const [showRoutes, setShowRoutes] = useState(true);
  const [showWeather, setShowWeather] = useState(true);
  const [showLegend, setShowLegend] = useState(true);
  const [routeFilter, setRouteFilter] = useState<RouteFilter>("all");

  // Faint AIS route network for map context (fetched once).
  useEffect(() => {
    fetch(`${API}/sim/ais-paths`)
      .then(r => r.json() as Promise<{ name: string; vessel_type: string; waypoints: { lat: number; lon: number }[] }[]>)
      .then(data => setRoutes(data.map(d => ({
        name: d.name, vessel_type: d.vessel_type,
        path: d.waypoints.map(w => [w.lon, w.lat] as [number, number]),
      }))))
      .catch(() => {});
  }, []);

  const shownRoutes = useMemo(
    () => routeFilter === "all" ? routes : routes.filter(r => r.vessel_type.toLowerCase() === routeFilter),
    [routes, routeFilter],
  );

  const map: MapControls = {
    showRoutes, onToggleRoutes: () => setShowRoutes(s => !s),
    showWeather, onToggleWeather: () => setShowWeather(s => !s),
    showLegend, onToggleLegend: () => setShowLegend(s => !s),
    routeFilter, onRouteFilter: setRouteFilter,
    onFit: () => mapRef.current?.fitDomain(),
    onZoomIn: () => mapRef.current?.zoomBy(0.6),
    onZoomOut: () => mapRef.current?.zoomBy(-0.6),
  };

  return (
    <div style={{
      display: "flex", height: "100vh", overflow: "hidden",
      background: color.bg, color: color.text,
      fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
    }}>
      <LeftPanel pb={pb} series={series} tab={tab} onTab={setTab} />

      {/* The map fills the rest — data overlays + legend, with the camera/layer
          controls floating in the top-right corner. */}
      <div style={{ position: "relative", flex: 1, minWidth: 0, overflow: "hidden" }}>
        <MapPane
          ref={mapRef}
          pb={pb}
          routes={shownRoutes}
          showRoutes={showRoutes}
          showWeather={showWeather}
          showLegend={showLegend}
        />
        <MapControlBar map={map} />
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
          background: color.bg, color: color.text,
          fontFamily: "monospace", padding: 32, gap: 16, overflowY: "auto",
        }}>
          <div style={{ fontSize: 28 }}>⚠</div>
          <div style={{ fontWeight: 700, fontSize: 15, color: color.bad }}>Render error</div>
          <pre style={{
            background: color.panel, border: `1px solid ${color.bad}`, borderRadius: 8,
            padding: "12px 16px", fontSize: 12, color: "#fca5a5",
            maxWidth: 800, width: "100%", overflowX: "auto", whiteSpace: "pre-wrap",
          }}>
            {this.state.error.message}{"\n\n"}{this.state.error.stack}
          </pre>
          <button
            onClick={() => this.setState({ error: null, componentStack: null })}
            style={{
              padding: "6px 18px", background: color.accentDeep, color: "#fff",
              border: `1px solid ${color.accent}`, borderRadius: 6, cursor: "pointer",
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
