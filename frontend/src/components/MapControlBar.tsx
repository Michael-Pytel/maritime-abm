import { useState } from "react";
import {
  color, radius, shadow,
  TYPE_HEX, TYPE_LABELS, type RouteFilter,
} from "../theme/tokens";

/** Map-layer + camera controls. Floats over the map's top-right corner. */
export interface MapControls {
  showRoutes: boolean; onToggleRoutes: () => void;
  showWeather: boolean; onToggleWeather: () => void;
  showLegend: boolean; onToggleLegend: () => void;
  routeFilter: RouteFilter; onRouteFilter: (f: RouteFilter) => void;
  onFit: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
}

/** The floating toolbar in the top-right of the map: zoom, fit, layers. */
export default function MapControlBar({ map }: { map: MapControls }) {
  const [layersOpen, setLayersOpen] = useState(false);
  return (
    <div style={{
      position: "absolute", top: 14, right: 14, zIndex: 3,
      display: "flex", alignItems: "flex-start", gap: 8,
    }}>
      <IconBtn title="Zoom out" onClick={map.onZoomOut}>－</IconBtn>
      <IconBtn title="Zoom in" onClick={map.onZoomIn}>＋</IconBtn>
      <IconBtn title="Fit to simulated area" onClick={map.onFit}>⤢</IconBtn>
      <div style={{ position: "relative" }}>
        <IconBtn title="Map layers" active={layersOpen} onClick={() => setLayersOpen(o => !o)}>≣</IconBtn>
        {layersOpen && <LayersPopover map={map} />}
      </div>
    </div>
  );
}

function IconBtn({ children, onClick, title, active }: {
  children: React.ReactNode; onClick: () => void; title: string; active?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      style={{
        width: 36, height: 36, borderRadius: radius.md, flexShrink: 0,
        display: "flex", alignItems: "center", justifyContent: "center",
        background: active ? color.panelSel : color.panel + "e6",
        border: `1px solid ${active ? color.borderBlue : color.border}`,
        color: active ? color.accentText : color.textDim,
        cursor: "pointer", fontSize: 15, boxShadow: shadow,
        backdropFilter: "blur(2px)",
      }}
    >{children}</button>
  );
}

function LayersPopover({ map }: { map: MapControls }) {
  return (
    <div style={{
      position: "absolute", top: 42, right: 0, width: 190, zIndex: 20,
      background: color.panel + "f2", border: `1px solid ${color.borderBlue}`,
      borderRadius: radius.md, boxShadow: shadow, padding: 11,
      display: "flex", flexDirection: "column", gap: 9,
    }}>
      <PopTitle>Map layers</PopTitle>
      <ToggleRow label="⛈ Weather hazard" on={map.showWeather} onClick={map.onToggleWeather} />
      <ToggleRow label="〜 AIS routes" on={map.showRoutes} onClick={map.onToggleRoutes} />
      {map.showRoutes && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4, paddingLeft: 2 }}>
          {(["all", "cargo", "passenger", "tanker"] as RouteFilter[]).map(f => {
            const active = map.routeFilter === f;
            const c = f === "all" ? color.sub : TYPE_HEX[f];
            return (
              <button key={f} onClick={() => map.onRouteFilter(f)} style={{
                padding: "2px 8px", fontSize: 10, borderRadius: radius.pill, cursor: "pointer",
                border: `1px solid ${active ? c : color.hair}`,
                background: active ? c + "22" : "transparent",
                color: active ? c : color.muted, fontWeight: active ? 700 : 400,
              }}>
                {f === "all" ? "All" : TYPE_LABELS[f]}
              </button>
            );
          })}
        </div>
      )}
      <div style={{ height: 1, background: color.border }} />
      <ToggleRow label="Legend" on={map.showLegend} onClick={map.onToggleLegend} />
    </div>
  );
}

function PopTitle({ children }: { children: string }) {
  return (
    <div style={{ fontSize: 9, fontWeight: 700, color: color.faint, textTransform: "uppercase", letterSpacing: "0.08em" }}>
      {children}
    </div>
  );
}

function ToggleRow({ label, on, onClick }: { label: string; on: boolean; onClick: () => void }) {
  return (
    <button onClick={onClick} style={{
      display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8,
      background: "transparent", border: "none", cursor: "pointer", padding: "1px 0",
      color: on ? color.text : color.muted, fontSize: 11, textAlign: "left", width: "100%",
    }}>
      <span>{label}</span>
      <span style={{
        width: 26, height: 15, borderRadius: 8, flexShrink: 0, position: "relative",
        background: on ? color.accent : color.hair, transition: "background .12s",
      }}>
        <span style={{
          position: "absolute", top: 2, left: on ? 13 : 2, width: 11, height: 11,
          borderRadius: "50%", background: "#fff", transition: "left .12s",
        }} />
      </span>
    </button>
  );
}
