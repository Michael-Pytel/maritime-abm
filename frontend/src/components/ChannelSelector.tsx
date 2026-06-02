import { memo } from "react";
import type { WeatherChannel } from "../types";

interface ChannelDef {
  id: WeatherChannel;
  label: string;
  icon: string;
}

const CHANNELS: ChannelDef[] = [
  { id: "wind",            label: "Wind",        icon: "💨" },
  { id: "wave_height",     label: "Waves",       icon: "🌊" },
  { id: "sea_state",       label: "Sea State",   icon: "⛵" },
  { id: "visibility",      label: "Visibility",  icon: "👁" },
  { id: "precipitation",   label: "Rain",        icon: "🌧" },
  { id: "pressure",        label: "Pressure",    icon: "🌡" },
  { id: "tide",            label: "Tide",        icon: "🌙" },
  { id: "surge",           label: "Surge",       icon: "📈" },
  { id: "total_water_level", label: "Water Lvl", icon: "💧" },
  { id: "hazard",          label: "Hazard",      icon: "⚠" },
];

interface Props {
  active: WeatherChannel;
  onChange: (ch: WeatherChannel) => void;
}

const ChannelSelector = memo(function ChannelSelector({ active, onChange }: Props) {
  return (
    <div style={{
      display: "flex",
      flexWrap: "wrap",
      gap: 4,
      padding: "8px 12px",
      background: "#0d1526cc",
      borderBottom: "1px solid #1e293b",
    }}>
      {CHANNELS.map(({ id, label, icon }) => {
        const isActive = active === id;
        return (
          <button
            key={id}
            onClick={() => onChange(id)}
            title={label}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 4,
              padding: "4px 10px",
              borderRadius: 16,
              border: isActive ? "1px solid #3b82f6" : "1px solid #334155",
              background: isActive ? "#1e3a8a" : "#1e293b",
              color: isActive ? "#93c5fd" : "#94a3b8",
              fontSize: 11,
              fontWeight: isActive ? 600 : 400,
              cursor: "pointer",
              transition: "all 0.12s",
              whiteSpace: "nowrap",
            }}
          >
            <span style={{ fontSize: 12 }}>{icon}</span>
            {label}
          </button>
        );
      })}
    </div>
  );
});

export default ChannelSelector;
export { CHANNELS };
export type { ChannelDef };
