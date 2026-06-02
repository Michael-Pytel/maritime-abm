import { memo } from "react";
import type { WeatherChannels, WeatherMeta } from "../types";

interface Props {
  lat: number;
  lon: number;
  channels: WeatherChannels;
  meta: WeatherMeta | undefined;
  gridSize: number;
  worldWidthNm: number;
  worldHeightNm: number;
  bboxLatMin: number;
  bboxLatMax: number;
  bboxLonMin: number;
  bboxLonMax: number;
  onClose: () => void;
}

// Bilinear sample of a flat grid at fractional grid coordinates (gx, gy).
// Row 0 = south (lat_min) in the backend, so we flip y.
function bilinearSample(grid: number[], gridSize: number, gx: number, gy: number): number {
  const ix0 = Math.max(0, Math.min(gridSize - 1, Math.floor(gx)));
  const iy0 = Math.max(0, Math.min(gridSize - 1, Math.floor(gy)));
  const ix1 = Math.min(gridSize - 1, ix0 + 1);
  const iy1 = Math.min(gridSize - 1, iy0 + 1);
  const fx = gx - ix0;
  const fy = gy - iy0;
  // y-flip: row 0 in the grid = south = lat_min
  const r0 = gridSize - 1 - iy0;
  const r1 = gridSize - 1 - iy1;
  const v00 = grid[Math.max(0, r0) * gridSize + ix0] ?? 0;
  const v01 = grid[Math.max(0, r0) * gridSize + ix1] ?? 0;
  const v10 = grid[Math.max(0, r1) * gridSize + ix0] ?? 0;
  const v11 = grid[Math.max(0, r1) * gridSize + ix1] ?? 0;
  const top    = v00 * (1 - fx) + v01 * fx;
  const bottom = v10 * (1 - fx) + v11 * fx;
  return top * (1 - fy) + bottom * fy;
}

function sampleAt(channels: WeatherChannels, key: keyof WeatherChannels, gridSize: number, gx: number, gy: number): number {
  return bilinearSample(channels[key] ?? [], gridSize, gx, gy);
}

const SEA_STATE_LABELS = ["Calm", "Rippled", "Wavelets", "Slight", "Moderate", "Rough", "Very Rough", "High", "Very High", "Phenomenal"];

const ProbePopup = memo(function ProbePopup({
  lat, lon, channels, meta, gridSize,
  worldWidthNm, worldHeightNm,
  bboxLatMin, bboxLatMax, bboxLonMin, bboxLonMax,
  onClose,
}: Props) {
  // Map lat/lon to grid coordinates.
  const gx = (lon - bboxLonMin) / (bboxLonMax - bboxLonMin) * (gridSize - 1);
  const gy = (lat - bboxLatMin) / (bboxLatMax - bboxLatMin) * (gridSize - 1);

  const windNorm   = sampleAt(channels, "wind", gridSize, gx, gy);
  const windDirRad = sampleAt(channels, "wind_direction", gridSize, gx, gy);
  const windKn     = windNorm * 30 * 1.944;
  const gustKn     = windKn * 1.35;
  const windDeg    = ((windDirRad * 180 / Math.PI) + 360) % 360;

  const waveHs   = sampleAt(channels, "wave_height", gridSize, gx, gy);
  const waveTp   = sampleAt(channels, "wave_period", gridSize, gx, gy);
  const seaState = sampleAt(channels, "sea_state", gridSize, gx, gy);
  const seaLabel = SEA_STATE_LABELS[Math.min(9, Math.round(seaState * 9))];

  const visNorm   = sampleAt(channels, "visibility", gridSize, gx, gy);
  const visNm     = visNorm * 30;

  const precip    = sampleAt(channels, "precipitation", gridSize, gx, gy);
  const rainMmh   = precip * 20;

  const pressure  = sampleAt(channels, "pressure", gridSize, gx, gy);
  const tide      = sampleAt(channels, "tide", gridSize, gx, gy);
  const surge     = sampleAt(channels, "surge", gridSize, gx, gy);
  const waterLvl  = tide + surge;

  const regime    = meta?.regime ?? "—";

  const rows: [string, string][] = [
    ["Wind",         `${windKn.toFixed(1)} kn, from ${windDeg.toFixed(0)}°`],
    ["Gusts",        `${gustKn.toFixed(1)} kn (est.)`],
    ["Waves Hs",     `${waveHs.toFixed(2)} m`],
    ["Wave period",  `${waveTp.toFixed(1)} s`],
    ["Sea state",    `${(seaState * 9).toFixed(0)} — ${seaLabel}`],
    ["Visibility",   `${visNm.toFixed(1)} nm`],
    ["Precip",       `${rainMmh.toFixed(1)} mm/h`],
    ["Pressure",     `${pressure.toFixed(0)} hPa`],
    ["Tide",         `${tide >= 0 ? "+" : ""}${tide.toFixed(2)} m`],
    ["Surge",        `${surge >= 0 ? "+" : ""}${surge.toFixed(2)} m`],
    ["Water level",  `${waterLvl >= 0 ? "+" : ""}${waterLvl.toFixed(2)} m`],
    ["Regime",       regime],
  ];

  return (
    <div style={{
      position: "absolute",
      top: 70,
      right: 12,
      zIndex: 1000,
      background: "#0d1526ee",
      border: "1px solid #1e3a8a",
      borderRadius: 10,
      padding: "12px 16px",
      minWidth: 220,
      maxWidth: 280,
      backdropFilter: "blur(8px)",
      boxShadow: "0 4px 24px #0008",
    }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
        <span style={{ fontSize: 11, fontWeight: 700, color: "#93c5fd", letterSpacing: "0.05em" }}>
          PROBE — {lat.toFixed(2)}°N {lon.toFixed(2)}°E
        </span>
        <button
          onClick={onClose}
          style={{ background: "none", border: "none", color: "#475569", cursor: "pointer", fontSize: 14, lineHeight: 1 }}
        >
          ✕
        </button>
      </div>
      <table style={{ borderCollapse: "collapse", width: "100%", fontSize: 11 }}>
        <tbody>
          {rows.map(([label, value]) => (
            <tr key={label}>
              <td style={{ color: "#64748b", paddingRight: 10, paddingBottom: 3 }}>{label}</td>
              <td style={{ color: "#e2e8f0", fontVariantNumeric: "tabular-nums", textAlign: "right", paddingBottom: 3 }}>{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
});

export default ProbePopup;
