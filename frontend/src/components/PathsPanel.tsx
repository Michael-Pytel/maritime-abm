import { useState, useEffect, useMemo } from "react";
import { MapContainer, TileLayer, Polyline, Tooltip } from "react-leaflet";
import "leaflet/dist/leaflet.css";

const API = "http://localhost:3000";

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

const TYPE_COLOR: Record<string, string> = {
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

  const displayed = useMemo(
    () => filter === "all" ? routes : routes.filter(r => r.vessel_type.toLowerCase() === filter),
    [routes, filter],
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
            const color  = TYPE_COLOR[t];
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
          {Object.entries(TYPE_COLOR).map(([type, color]) => (
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

        {/* preferCanvas omitted (defaults false) so SVG is used — required for
            per-polyline mouse events and Tooltip hover to work correctly. */}
        <MapContainer
          center={[58, 14]}
          zoom={5}
          style={{ height: "100%", width: "100%" }}
        >
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
            url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
          />

          {displayed.map(record => {
            const typeKey = record.vessel_type.toLowerCase();
            return (
              <Polyline
                key={record.mmsi}
                positions={record.waypoints.map(wp => [wp.lat, wp.lon] as [number, number])}
                pathOptions={{
                  color:   TYPE_COLOR[typeKey] ?? "#94a3b8",
                  weight:  2,
                  opacity: 0.65,
                }}
              >
                <Tooltip sticky>
                  <div style={{ lineHeight: 1.5 }}>
                    <strong style={{ fontSize: 12 }}>{record.name}</strong><br />
                    <span style={{ color: "#64748b", fontSize: 11 }}>MMSI {record.mmsi}</span><br />
                    <span style={{ fontSize: 11 }}>
                      {TYPE_LABELS[typeKey] ?? record.vessel_type}
                      &nbsp;·&nbsp;{record.waypoints.length} waypoints
                    </span>
                  </div>
                </Tooltip>
              </Polyline>
            );
          })}
        </MapContainer>
      </div>
    </div>
  );
}
