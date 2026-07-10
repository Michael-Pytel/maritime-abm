import { useState } from "react";
import { color, radius, shadow, TYPE_HEX, TYPE_LABELS } from "../theme/tokens";

export type RouteFilter = "all" | "cargo" | "passenger" | "tanker";

interface Props {
  onFit: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  playing: boolean;
  canPlay: boolean;
  onTogglePlay: () => void;
  showWeather: boolean;
  onToggleWeather: () => void;
  showRoutes: boolean;
  onToggleRoutes: () => void;
  routeFilter: RouteFilter;
  onRouteFilter: (f: RouteFilter) => void;
  showLegend: boolean;
  onToggleLegend: () => void;
}

export default function MapToolbar(p: Props) {
  const [layersOpen, setLayersOpen] = useState(false);

  return (
    <div style={{ position: "absolute", top: 14, right: 14, zIndex: 3, display: "flex", flexDirection: "column", gap: 8, alignItems: "flex-end" }}>
      <ToolButton title="Fit to simulated area" onClick={p.onFit}>⤢</ToolButton>
      <div style={{ display: "flex", flexDirection: "column", background: color.panel + "ee", border: `1px solid ${color.borderBlue}`, borderRadius: radius.md, overflow: "hidden" }}>
        <ToolButton title="Zoom in" onClick={p.onZoomIn} flush>＋</ToolButton>
        <div style={{ height: 1, background: color.border }} />
        <ToolButton title="Zoom out" onClick={p.onZoomOut} flush>－</ToolButton>
      </div>

      <div style={{ position: "relative" }}>
        <ToolButton title="Layers" onClick={() => setLayersOpen(o => !o)} active={layersOpen}>≣</ToolButton>
        {layersOpen && (
          <div style={{
            position: "absolute", top: 0, right: 44, width: 176,
            background: color.panel + "f2", border: `1px solid ${color.borderBlue}`,
            borderRadius: radius.md, boxShadow: shadow, padding: 10,
            display: "flex", flexDirection: "column", gap: 8,
          }}>
            <PopTitle>Layers</PopTitle>
            <ToggleRow label="⛈ Weather hazard" on={p.showWeather} onClick={p.onToggleWeather} />
            <ToggleRow label="〜 AIS routes" on={p.showRoutes} onClick={p.onToggleRoutes} />
            {p.showRoutes && (
              <div style={{ display: "flex", flexWrap: "wrap", gap: 4, paddingLeft: 4 }}>
                {(["all", "cargo", "passenger", "tanker"] as RouteFilter[]).map(f => {
                  const active = p.routeFilter === f;
                  const c = f === "all" ? color.sub : TYPE_HEX[f];
                  return (
                    <button key={f} onClick={() => p.onRouteFilter(f)} style={{
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
            <ToggleRow label="Legend" on={p.showLegend} onClick={p.onToggleLegend} />
          </div>
        )}
      </div>

      <ToolButton title={p.playing ? "Pause" : "Play"} onClick={p.onTogglePlay} disabled={!p.canPlay} accent>
        {p.playing ? "❚❚" : "▶"}
      </ToolButton>
    </div>
  );
}

function ToolButton({
  children, onClick, title, active, accent, disabled, flush,
}: {
  children: React.ReactNode; onClick: () => void; title: string;
  active?: boolean; accent?: boolean; disabled?: boolean; flush?: boolean;
}) {
  const bg = disabled ? color.panel + "aa" : accent ? color.accentDeep : active ? color.panelSel : color.panel + "ee";
  const fg = disabled ? color.faint : accent ? "#fff" : active ? color.accentText : color.textDim;
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      style={{
        width: 36, height: 36, display: "flex", alignItems: "center", justifyContent: "center",
        background: bg, color: fg, fontSize: 15,
        border: flush ? "none" : `1px solid ${color.borderBlue}`,
        borderRadius: flush ? 0 : radius.md,
        cursor: disabled ? "default" : "pointer",
      }}
    >
      {children}
    </button>
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
