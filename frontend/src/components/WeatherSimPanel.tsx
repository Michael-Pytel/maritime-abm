import { useState, useCallback, useEffect, memo } from "react";
import { MapContainer, TileLayer, useMap } from "react-leaflet";
import "leaflet/dist/leaflet.css";
import L from "leaflet";
import type { TickMessage, WeatherChannel, WeatherChannels } from "../types";
import GradientOverlay from "./GradientOverlay";
import WindParticles from "./WindParticles";
import IsobarLayer from "./IsobarLayer";
import ChannelSelector from "./ChannelSelector";
import ProbePopup from "./ProbePopup";

const BBOX_LAT_MIN = 50.5;
const BBOX_LAT_MAX = 66.0;
const BBOX_LON_MIN = -5.0;
const BBOX_LON_MAX = 31.0;

interface Props {
  snapshot: TickMessage | null;
  onClose?: () => void;
  /** When true the panel fills its parent container instead of using a fixed overlay. */
  inline?: boolean;
}

interface ProbeState {
  lat: number;
  lon: number;
}

// Rendered inside MapContainer so useMap() is available.
function MapContent({
  channels,
  gridSize,
  activeChannel,
  onProbe,
}: {
  channels: WeatherChannels;
  gridSize: number;
  activeChannel: WeatherChannel;
  onProbe: (lat: number, lon: number) => void;
}) {
  const map = useMap();

  useEffect(() => {
    const handler = (e: L.LeafletMouseEvent) => {
      onProbe(e.latlng.lat, e.latlng.lng);
    };
    map.on("click", handler);
    return () => { map.off("click", handler); };
  }, [map, onProbe]);

  const activeGrid   = channels[activeChannel] ?? channels.hazard ?? [];
  const pressureGrid = channels.pressure ?? [];

  return (
    <>
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org">OSM</a> &copy; <a href="https://carto.com">CARTO</a>'
        url="https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png"
      />

      {activeGrid.length > 0 && (
        <GradientOverlay
          grid={activeGrid}
          gridSize={gridSize}
          channel={activeChannel}
          opacity={0.72}
        />
      )}

      {pressureGrid.length > 0 && (
        <IsobarLayer pressureGrid={pressureGrid} gridSize={gridSize} />
      )}
    </>
  );
}

const WeatherSimPanel = memo(function WeatherSimPanel({ snapshot, onClose, inline = false }: Props) {
  const [activeChannel, setActiveChannel] = useState<WeatherChannel>("wind");
  const [probe, setProbe] = useState<ProbeState | null>(null);

  const handleProbe = useCallback((lat: number, lon: number) => {
    setProbe({ lat, lon });
  }, []);

  const handleCloseProbe = useCallback(() => setProbe(null), []);

  const channels  = snapshot?.weather_channels as WeatherChannels | undefined;
  const meta      = snapshot?.weather_meta;
  const gridSize  = snapshot?.weather_grid_size ?? 32;
  const step      = snapshot?.step ?? 0;
  const windGrid    = channels?.wind ?? [];
  const windDirGrid = channels?.wind_direction ?? [];

  const rootStyle = inline ? inlineStyle : overlayStyle;

  if (!channels) {
    return (
      <div style={rootStyle}>
        <div style={headerStyle}>
          <span style={{ color: "#93c5fd", fontWeight: 700 }}>Weather Simulation</span>
          {!inline && onClose && <button onClick={onClose} style={closeBtn}>✕ Close</button>}
        </div>
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "#475569" }}>
          Waiting for weather data…
        </div>
      </div>
    );
  }

  return (
    <div style={rootStyle}>
      {/* Header */}
      <div style={headerStyle}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontSize: 16 }}>⛈</span>
          <span style={{ color: "#f1f5f9", fontWeight: 700, fontSize: 13 }}>
            Weather Simulation
          </span>
          {meta && (
            <span style={{
              padding: "2px 8px",
              borderRadius: 10,
              fontSize: 10,
              fontWeight: 600,
              background: meta.regime === "Storm" ? "#450a0a" : meta.regime === "Transition" ? "#451a03" : "#022c22",
              color:     meta.regime === "Storm" ? "#f87171" : meta.regime === "Transition" ? "#fb923c" : "#34d399",
              border: "1px solid currentColor",
            }}>
              {meta.regime}
            </span>
          )}
          <span style={{ fontSize: 10, color: "#475569" }}>tick {step.toLocaleString()}</span>
        </div>
        {!inline && onClose && <button onClick={onClose} style={closeBtn}>✕ Close</button>}
      </div>

      {/* Channel selector */}
      <ChannelSelector active={activeChannel} onChange={setActiveChannel} />

      {/* Map area */}
      <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
        <MapContainer
          center={[58.25, 13.0]}
          zoom={5}
          preferCanvas={true}
          style={{ height: "100%", width: "100%", minHeight: 200 }}
          zoomControl={true}
        >
          <MapContent
            channels={channels}
            gridSize={gridSize}
            activeChannel={activeChannel}
            onProbe={handleProbe}
          />
        </MapContainer>

        {/* Wind particles — rendered outside MapContainer to avoid z-index issues */}
        {windGrid.length > 0 && (
          <WindParticles
            windGrid={windGrid}
            windDirGrid={windDirGrid}
            gridSize={gridSize}
            bboxLatMin={BBOX_LAT_MIN}
            bboxLatMax={BBOX_LAT_MAX}
            bboxLonMin={BBOX_LON_MIN}
            bboxLonMax={BBOX_LON_MAX}
            particleCount={2000}
            particleSeed={meta?.particle_seed ?? 42}
          />
        )}

        {probe && (
          <ProbePopup
            lat={probe.lat}
            lon={probe.lon}
            channels={channels}
            meta={meta}
            gridSize={gridSize}
            worldWidthNm={snapshot?.world_width_nm ?? 1136}
            worldHeightNm={snapshot?.world_height_nm ?? 930}
            bboxLatMin={BBOX_LAT_MIN}
            bboxLatMax={BBOX_LAT_MAX}
            bboxLonMin={BBOX_LON_MIN}
            bboxLonMax={BBOX_LON_MAX}
            onClose={handleCloseProbe}
          />
        )}
      </div>
    </div>
  );
});

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 2000,
  background: "#0a0f1e",
  display: "flex",
  flexDirection: "column",
  color: "#f1f5f9",
  fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
};

const inlineStyle: React.CSSProperties = {
  position: "relative",
  flex: 1,
  background: "#0a0f1e",
  display: "flex",
  flexDirection: "column",
  color: "#f1f5f9",
  fontFamily: "'Inter', 'Segoe UI', system-ui, sans-serif",
  overflow: "hidden",
  height: "100%",
};

const headerStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "8px 14px",
  background: "#060c18",
  borderBottom: "1px solid #1e3a5f",
  flexShrink: 0,
  minHeight: 44,
};

const closeBtn: React.CSSProperties = {
  background: "#1e293b",
  border: "1px solid #334155",
  borderRadius: 6,
  color: "#94a3b8",
  padding: "4px 12px",
  fontSize: 11,
  cursor: "pointer",
  fontWeight: 600,
};

export default WeatherSimPanel;
