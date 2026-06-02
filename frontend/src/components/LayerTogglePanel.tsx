import type { LayerToggles } from "../hooks/usePlayback";

interface Props {
  layers: LayerToggles;
  onToggle: (key: keyof LayerToggles) => void;
}

const LAYER_DEFS: { key: keyof LayerToggles; label: string; color: string }[] = [
  { key: "weatherOverlay",        label: "Weather overlay",       color: "#f59e0b" },
  { key: "stormCenters",          label: "Storm centres",         color: "#f97316" },
  { key: "rescueAssets",          label: "Rescue assets",         color: "#22c55e" },
  { key: "sosMarkers",            label: "SOS / evac markers",    color: "#fbbf24" },
  { key: "shoreBroadcastCircles", label: "Shore broadcast range", color: "#22d3ee" },
  { key: "radioRangeCircles",     label: "Vessel radio range",    color: "#a78bfa" },
];

export default function LayerTogglePanel({ layers, onToggle }: Props) {
  return (
    <div style={{
      background: "#060c18",
      border: "1px solid #1e293b",
      borderRadius: 8,
      padding: "10px 12px",
      display: "flex",
      flexDirection: "column",
      gap: 6,
      minWidth: 200,
    }}>
      <div style={{
        fontSize: 10, fontWeight: 700, color: "#475569",
        textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 2,
      }}>
        Layers
      </div>
      {LAYER_DEFS.map(({ key, label, color }) => {
        const on = layers[key];
        return (
          <button
            key={key}
            onClick={() => onToggle(key)}
            style={{
              display: "flex", alignItems: "center", gap: 8,
              background: "transparent", border: "none",
              cursor: "pointer", padding: "3px 0", textAlign: "left",
            }}
          >
            {/* Toggle pill */}
            <div style={{
              width: 28, height: 14, borderRadius: 7,
              background: on ? color : "#1e293b",
              border: `1px solid ${on ? color : "#334155"}`,
              position: "relative",
              transition: "all 0.15s",
              flexShrink: 0,
            }}>
              <div style={{
                position: "absolute",
                top: 2, left: on ? 14 : 2,
                width: 8, height: 8,
                borderRadius: "50%",
                background: on ? "#fff" : "#475569",
                transition: "left 0.15s",
              }} />
            </div>
            <span style={{ fontSize: 11, color: on ? "#f1f5f9" : "#64748b" }}>
              {label}
            </span>
          </button>
        );
      })}
    </div>
  );
}
