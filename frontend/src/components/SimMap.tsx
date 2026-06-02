import { memo } from "react";
import { MapContainer, TileLayer, CircleMarker, Circle, Popup } from "react-leaflet";
import "leaflet/dist/leaflet.css";
import type { VesselSnapshot, Port, CollisionEvent, Storm } from "../types";
import WeatherOverlay from "./WeatherOverlay";

/** How many ticks a collision marker takes to fully fade out. */
const COLLISION_FADE_TICKS = 40;

/** Returns the fill colour for a vessel marker. */
function vesselColor(v: VesselSnapshot): string {
  if (v.avoiding)           return "#f59e0b"; // amber — executing avoidance manoeuvre
  if (v.state === "Docked") return "#14b8a6"; // teal
  return "#3b82f6";                           // blue — normal active
}

const NM_TO_METRES = 1852;

interface Props {
  vessels: VesselSnapshot[];
  ports?: Port[];
  collisionEvents?: CollisionEvent[];
  currentStep?: number;
  storm?: Storm | null;
  weatherGrid?: number[];
  weatherGridSize?: number;
  latMin?: number;
  latMax?: number;
  lonMin?: number;
  lonMax?: number;
}

const SimMap = memo(function SimMap({
  vessels,
  ports = [],
  collisionEvents = [],
  currentStep = 0,
  storm = null,
  weatherGrid = [],
  weatherGridSize = 0,
  latMin = 50.5,
  latMax = 66.0,
  lonMin = -5.0,
  lonMax = 31.0,
}: Props) {
  return (
    <MapContainer
      center={[56, 12]}
      zoom={5}
      preferCanvas={true}
      style={{ height: "100%", width: "100%", minHeight: 400 }}
    >
      <TileLayer
        attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
        url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
      />

      {weatherGrid.length > 0 && (
        <WeatherOverlay
          grid={weatherGrid}
          gridSize={weatherGridSize}
          latMin={latMin} latMax={latMax}
          lonMin={lonMin} lonMax={lonMax}
          opacity={0.50}
        />
      )}

      {storm && (
        <>
          {/* Outer haze — wide, very transparent */}
          <Circle
            center={[storm.lat, storm.lon]}
            radius={storm.radius_nm * NM_TO_METRES}
            pathOptions={{
              fillColor: "#7c3aed",
              color: "#a78bfa",
              weight: 1.5,
              fillOpacity: 0.10,
              opacity: 0.50,
            }}
          >
            <Popup>
              <strong style={{ color: "#a78bfa" }}>⛈ Storm Zone</strong><br />
              <small>Comms degraded — collision risk elevated</small><br />
              <small>Radius: {storm.radius_nm.toFixed(0)} nm</small>
            </Popup>
          </Circle>
          {/* Inner core — denser fill */}
          <Circle
            center={[storm.lat, storm.lon]}
            radius={storm.radius_nm * 0.45 * NM_TO_METRES}
            pathOptions={{
              fillColor: "#6d28d9",
              color: "transparent",
              weight: 0,
              fillOpacity: 0.18,
            }}
          />
        </>
      )}

      {ports.map(p => (
        <CircleMarker
          key={`port-${p.id}`}
          center={[p.lat, p.lon]}
          radius={4}
          pathOptions={{ fillColor: "#94a3b8", color: "#475569", weight: 1, fillOpacity: 0.65 }}
        >
          <Popup>
            <strong>{p.name}</strong><br />
            <small>Port</small>
          </Popup>
        </CircleMarker>
      ))}

      {collisionEvents.map((ev, i) => {
        const age = currentStep - ev.tick;
        const opacity = Math.max(0, 1 - age / COLLISION_FADE_TICKS);
        if (opacity <= 0) return null;
        return (
          <CircleMarker
            key={`collision-${ev.tick}-${i}`}
            center={[ev.lat, ev.lon]}
            radius={10}
            pathOptions={{
              fillColor: "#ef4444",
              color: "#fca5a5",
              weight: 2,
              fillOpacity: opacity * 0.55,
              opacity,
            }}
          >
            <Popup>
              <strong style={{ color: "#ef4444" }}>⚠ Collision</strong><br />
              <small>Tick {ev.tick}</small>
            </Popup>
          </CircleMarker>
        );
      })}

      {vessels.map(v => {
        const fill = vesselColor(v);
        return (
          <CircleMarker
            key={v.id}
            center={[v.lat, v.lon]}
            radius={v.state === "Docked" ? 5 : 6}
            pathOptions={{
              fillColor: fill,
              // Avoiding ships get a bright amber ring so they stand out clearly.
              color: v.avoiding ? "#fbbf24" : (v.state === "Docked" ? "#0f766e" : "#1e293b"),
              weight: v.avoiding ? 2 : 1,
              fillOpacity: v.state === "Docked" ? 0.75 : 0.9,
            }}
          >
            <Popup>
              <strong>{v.name}</strong><br />
              State: <b>{v.state}</b>
              {v.avoiding && <><br /><span style={{ color: "#f59e0b", fontWeight: 600 }}>⚠ Collision avoidance active</span></>}<br />
              Crew: {v.n_crew}<br />
              {v.state === "Docked" && v.dock_until_tick != null && (
                <>Departs at tick {v.dock_until_tick}</>
              )}
            </Popup>
          </CircleMarker>
        );
      })}
    </MapContainer>
  );
});

export default SimMap;
