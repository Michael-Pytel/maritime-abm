import { useState, useEffect, useMemo } from "react";
import DeckGL from "@deck.gl/react";
import { Map } from "react-map-gl/maplibre";
import { PathLayer } from "deck.gl";
import "maplibre-gl/dist/maplibre-gl.css";

const API = "http://localhost:3000";

// ── MarineTraffic-style dark basemap (keyless CARTO raster), shared visual
//    language with MapGL. ──────────────────────────────────────────────────────
const DARK_STYLE = {
  version: 8 as const,
  sources: {
    carto: {
      type: "raster" as const,
      tiles: [
        "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
        "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
        "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png",
      ],
      tileSize: 256,
      attribution: "© OpenStreetMap © CARTO",
    },
  },
  layers: [
    { id: "bg", type: "background" as const, paint: { "background-color": "#0a1420" } },
    { id: "carto", type: "raster" as const, source: "carto" },
  ],
};

const INITIAL_VIEW = { longitude: 13, latitude: 58, zoom: 4.4, pitch: 0, bearing: 0 };

interface AisWaypoint {
  lat: number;
  lon: number;
  timestamp_utc?: string; // optional — new keypoint-only format omits it
}

interface AisRecord {
  mmsi: number;
  name: string;
  vessel_type: string;
  waypoints: AisWaypoint[];
}

/** A route prepared for deck.gl: [lon, lat] path + its source record. */
interface RouteFeature extends AisRecord {
  path: [number, number][];
}

type RGB = [number, number, number];
const TYPE_COLOR: Record<string, RGB> = {
  cargo:     [59, 130, 246],
  passenger: [34, 197, 94],
  tanker:    [249, 115, 22],
};
const DEFAULT_COLOR: RGB = [148, 163, 184];

const TYPE_HEX: Record<string, string> = {
  cargo:     "#3b82f6",
  passenger: "#22c55e",
  tanker:    "#f97316",
};

const TYPE_LABELS: Record<string, string> = {
  cargo:     "Cargo",
  passenger: "Passenger",
  tanker:    "Tanker",
};

const FILTER_TYPES = ["all", "cargo", "passenger", "tanker"] as const;
type Filter = typeof FILTER_TYPES[number];

export default function PathsPanel() {
  const [routes, setRoutes]   = useState<AisRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [filter, setFilter]   = useState<Filter>("all");

  useEffect(() => {
    fetch(`${API}/sim/ais-paths`)
      .then(r => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json() as Promise<AisRecord[]>;
      })
      .then(data => { setRoutes(data); setLoading(false); })
      .catch(e  => { setError(String(e)); setLoading(false); });
  }, []);

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    for (const r of routes) {
      const t = r.vessel_type.toLowerCase();
      c[t] = (c[t] ?? 0) + 1;
    }
    return c;
  }, [routes]);

  const displayed = useMemo<RouteFeature[]>(
    () =>
      routes
        .filter(r => filter === "all" || r.vessel_type.toLowerCase() === filter)
        .map(r => ({ ...r, path: r.waypoints.map(wp => [wp.lon, wp.lat] as [number, number]) })),
    [routes, filter],
  );

  const layers = useMemo(
    () => [
      new PathLayer<RouteFeature>({
        id: "ais-paths",
        data: displayed,
        getPath: d => d.path,
        getColor: d => [...(TYPE_COLOR[d.vessel_type.toLowerCase()] ?? DEFAULT_COLOR), 165] as [number, number, number, number],
        getWidth: 2,
        widthUnits: "pixels",
        widthMinPixels: 1.5,
        jointRounded: true,
        capRounded: true,
        pickable: true,
        autoHighlight: true,
        highlightColor: [255, 255, 255, 220],
      }),
    ],
    [displayed],
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>

      {/* ── Filter bar ── */}
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        padding: "8px 16px",
        background: "#0d1526",
        borderBottom: "1px solid #1e293b",
        flexShrink: 0,
      }}>
        <span style={{
          fontSize: 10, fontWeight: 700, color: "#475569",
          textTransform: "uppercase", letterSpacing: "0.08em",
        }}>
          AIS Paths
        </span>

        <div style={{ display: "flex", gap: 6 }}>
          {FILTER_TYPES.map(t => {
            const color  = TYPE_HEX[t];
            const active = filter === t;
            const label  = t === "all"
              ? `All (${routes.length})`
              : `${TYPE_LABELS[t]} (${counts[t] ?? 0})`;
            return (
              <button
                key={t}
                onClick={() => setFilter(t)}
                style={{
                  padding: "3px 11px", borderRadius: 12, cursor: "pointer",
                  fontSize: 11, fontWeight: active ? 700 : 400,
                  border: `1px solid ${active ? (color ?? "#64748b") : "#1e3a5f"}`,
                  background: active ? (color ? color + "22" : "#1e293b") : "transparent",
                  color: active ? (color ?? "#f1f5f9") : "#475569",
                  transition: "all 0.15s",
                }}
              >
                {label}
              </button>
            );
          })}
        </div>

        {/* Legend dots */}
        <div style={{ marginLeft: "auto", display: "flex", gap: 12, alignItems: "center" }}>
          {Object.entries(TYPE_HEX).map(([type, color]) => (
            <span key={type} style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 10, color: "#94a3b8" }}>
              <span style={{ width: 20, height: 2, background: color, display: "inline-block", borderRadius: 1 }} />
              {TYPE_LABELS[type]}
            </span>
          ))}
        </div>
      </div>

      {/* ── Map ── */}
      <div style={{ flex: 1, position: "relative" }}>
        {loading && (
          <div style={{
            position: "absolute", inset: 0, zIndex: 1000,
            display: "flex", alignItems: "center", justifyContent: "center",
            background: "#0a0f1ecc", color: "#94a3b8", fontSize: 13,
          }}>
            Loading AIS paths…
          </div>
        )}
        {error && (
          <div style={{
            position: "absolute", inset: 0, zIndex: 1000,
            display: "flex", alignItems: "center", justifyContent: "center",
            background: "#0a0f1ecc", color: "#f87171", fontSize: 13,
          }}>
            Failed to load: {error}
          </div>
        )}

        <DeckGL
          initialViewState={INITIAL_VIEW}
          controller
          layers={layers as never}
          getTooltip={getTooltip}
          style={{ position: "absolute", inset: "0" }}
        >
          <Map reuseMaps mapStyle={DARK_STYLE as never} />
        </DeckGL>
      </div>
    </div>
  );
}

const TOOLTIP_STYLE: Record<string, string> = {
  background: "#0d1526",
  color: "#e2e8f0",
  fontSize: "11px",
  padding: "6px 9px",
  borderRadius: "6px",
  border: "1px solid #1e3a5f",
  boxShadow: "0 4px 14px rgba(0,0,0,.5)",
};

function getTooltip(info: { object?: RouteFeature }): { html: string; style: Record<string, string> } | null {
  const r = info.object;
  if (!r) return null;
  const typeKey = r.vessel_type.toLowerCase();
  return {
    html:
      `<div style="font-weight:700;margin-bottom:2px">${r.name}</div>` +
      `<div style="opacity:.65">MMSI ${r.mmsi}</div>` +
      `<div style="opacity:.85">${TYPE_LABELS[typeKey] ?? r.vessel_type} · ${r.waypoints.length} waypoints</div>`,
    style: TOOLTIP_STYLE,
  };
}
