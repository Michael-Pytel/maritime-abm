import { useState, useEffect, useRef, useMemo } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import { SPEED_PRESETS } from "../hooks/usePlayback";
import type { RunSeries } from "../hooks/useRunSeries";
import MapGL, { type RoutePath, type MapHandle } from "./MapGL";
import MapToolbar, { type RouteFilter } from "./MapToolbar";
import StatsIsland, { type IslandTab } from "./StatsIsland";
import { color, radius, shadow, SCEN_SHORT, methodShort, METHOD_COLORS } from "../theme/tokens";

const API = "http://localhost:3000";
type PB = ReturnType<typeof usePlayback>;

interface Props {
  pb: PB;
  series: RunSeries;
  island: { expanded: boolean; tab: IslandTab };
  onIsland: (next: { expanded: boolean; tab: IslandTab }) => void;
}

/** The map stage — a full-bleed GL map with floating overlays (run card,
 *  toolbar, bottom island, scrubber). The visual centrepiece of the app. */
export default function MapStage({ pb, series, island, onIsland }: Props) {
  const mapRef = useRef<MapHandle>(null);
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

  const t = pb.currentTick;
  const m = pb.manifest;

  return (
    <div style={{ position: "absolute", inset: 0, overflow: "hidden" }}>
      <MapGL
        ref={mapRef}
        vessels={t?.vessels ?? []}
        ports={t?.ports ?? []}
        routes={shownRoutes}
        showRoutes={showRoutes}
        showLegend={showLegend}
        collisionEvents={t?.collision_events ?? []}
        currentStep={t?.step ?? 0}
        rescueAgents={t?.rescue_agents ?? []}
        wrecks={t?.wrecks ?? []}
        mobPersons={t?.mob_persons ?? []}
        mobAgents={t?.mob_agents ?? []}
        storm={t?.storm ?? null}
        weatherGrid={t?.weather_grid ?? []}
        weatherGridSize={t?.weather_grid_size ?? 0}
        showWeather={showWeather}
        latMin={t?.bbox?.lat_min}
        latMax={t?.bbox?.lat_max}
        lonMin={t?.bbox?.lon_min}
        lonMax={t?.bbox?.lon_max}
        transitionMs={pb.speedMs}
      />

      {/* Top-left run card */}
      <div style={{
        position: "absolute", top: 14, left: 14, zIndex: 2,
        background: color.panel + "e6", border: `1px solid ${m ? color.borderBlue : color.border}`,
        borderRadius: radius.md, padding: "7px 12px", boxShadow: shadow,
        display: "flex", alignItems: "center", gap: 9, maxWidth: 340,
      }}>
        {m ? (
          <>
            <span style={{ width: 8, height: 8, borderRadius: "50%", background: METHOD_COLORS[m.method] ?? color.accent, flexShrink: 0 }} />
            <span style={{ fontSize: 12, fontWeight: 700, color: color.text }}>{SCEN_SHORT[m.scenario] ?? m.scenario}</span>
            <span style={{ fontSize: 11, color: METHOD_COLORS[m.method] ?? color.accent, fontWeight: 600 }}>{methodShort(m.method)}</span>
            <span style={{ fontSize: 11, color: color.faint }}>#{m.seed}</span>
            <span style={{ fontSize: 10, color: color.muted }}>· {m.n_vessels} vessels</span>
          </>
        ) : (
          <span style={{ fontSize: 11, color: color.muted }}>No run selected — pick one in the Runs panel</span>
        )}
      </div>

      {/* Right toolbar */}
      <MapToolbar
        onFit={() => mapRef.current?.fitDomain()}
        onZoomIn={() => mapRef.current?.zoomBy(0.6)}
        onZoomOut={() => mapRef.current?.zoomBy(-0.6)}
        playing={pb.playing}
        canPlay={pb.ticks.length > 0}
        onTogglePlay={() => pb.setPlaying(p => !p)}
        showWeather={showWeather}
        onToggleWeather={() => setShowWeather(s => !s)}
        showRoutes={showRoutes}
        onToggleRoutes={() => setShowRoutes(s => !s)}
        routeFilter={routeFilter}
        onRouteFilter={setRouteFilter}
        showLegend={showLegend}
        onToggleLegend={() => setShowLegend(s => !s)}
      />

      {/* Bottom island */}
      <div style={{ position: "absolute", left: "50%", bottom: 52, transform: "translateX(-50%)", zIndex: 2, display: "flex", justifyContent: "center", maxWidth: "94vw" }}>
        <StatsIsland
          pb={pb}
          series={series}
          expanded={island.expanded}
          tab={island.tab}
          onToggle={() => onIsland({ ...island, expanded: !island.expanded })}
          onTab={tab => onIsland({ expanded: true, tab })}
        />
      </div>

      {/* Scrubber */}
      <Scrubber pb={pb} />
    </div>
  );
}

function Scrubber({ pb }: { pb: PB }) {
  const n = pb.ticks.length;
  const step = pb.currentTick?.step ?? 0;
  return (
    <div style={{
      position: "absolute", left: 0, right: 0, bottom: 0, zIndex: 3,
      background: color.bg + "f2", borderTop: `1px solid ${color.border}`,
      padding: "8px 14px", display: "flex", alignItems: "center", gap: 12,
    }}>
      <button
        onClick={() => pb.setPlaying(p => !p)}
        disabled={n === 0}
        style={{ width: 30, height: 30, borderRadius: radius.sm, background: n === 0 ? color.border : color.accentDeep, color: n === 0 ? color.faint : "#fff", border: "none", cursor: n === 0 ? "default" : "pointer", fontSize: 12 }}
      >{pb.playing ? "❚❚" : "▶"}</button>
      <input
        type="range" min={0} max={Math.max(0, n - 1)} value={pb.currentIdx}
        onChange={e => pb.setCurrentIdx(Number(e.target.value))}
        disabled={n === 0}
        style={{ flex: 1, accentColor: color.accent }}
      />
      <span style={{ fontSize: 11, color: color.sub, fontVariantNumeric: "tabular-nums", minWidth: 96, textAlign: "right" }}>
        {pb.loading ? "loading…" : n === 0 ? "—" : `tick ${step} · ${pb.currentIdx + 1}/${n}`}
      </span>
      <select
        value={pb.speedMs}
        onChange={e => pb.setSpeedMs(Number(e.target.value))}
        style={{ background: color.border, color: color.textDim, border: `1px solid ${color.hair}`, borderRadius: radius.sm, fontSize: 10, padding: "3px 6px", cursor: "pointer" }}
      >
        {SPEED_PRESETS.map(s => <option key={s.label} value={s.ms}>{s.label}</option>)}
      </select>
    </div>
  );
}
