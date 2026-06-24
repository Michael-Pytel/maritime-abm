import { memo } from "react";
import { MapContainer, TileLayer, CircleMarker, Circle, Popup } from "react-leaflet";
import "leaflet/dist/leaflet.css";
import type {
  VesselSnapshot, Port, CollisionEvent, Storm,
  RescueAgentSnapshot, WreckMarker, MobPersonSnapshot, MobAgentSnapshot,
} from "../types";
import WeatherOverlay from "./WeatherOverlay";

/** How many ticks a collision marker takes to fully fade out. */
const COLLISION_FADE_TICKS = 40;

/** Returns the fill colour for a vessel marker. */
function vesselColor(v: VesselSnapshot): string {
  if (v.avoiding)           return "#f59e0b"; // amber — executing avoidance manoeuvre
  switch (v.state) {
    case "Docked":  return "#14b8a6"; // teal — in port
    case "Evac":    return "#fbbf24"; // yellow — foundered, survivors in liferaft
    case "Rescued": return "#34d399"; // green — survivors recovered
    case "Lost":    return "#64748b"; // slate — lost with no survivors
    default:        return "#3b82f6"; // blue — normal active
  }
}

/** Rescue-asset marker styling, keyed by SAR phase. */
const RESCUE_PHASE_LABEL: Record<RescueAgentSnapshot["phase"], string> = {
  mobilising: "Mobilising at base",
  transiting: "Transiting to datum",
  searching:  "Searching on scene",
  embarking:  "Embarking survivors",
};

/** Human-readable collision geometry labels. */
const COLLISION_TYPE_LABEL: Record<string, string> = {
  head_on:       "Head-on",
  front_to_side: "Front-to-side (crossing)",
  side_to_side:  "Side-to-side (overtaking)",
};

const NM_TO_METRES = 1852;

interface Props {
  vessels: VesselSnapshot[];
  ports?: Port[];
  collisionEvents?: CollisionEvent[];
  rescueAgents?: RescueAgentSnapshot[];
  wrecks?: WreckMarker[];
  mobPersons?: MobPersonSnapshot[];
  mobAgents?: MobAgentSnapshot[];
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
  rescueAgents = [],
  wrecks = [],
  mobPersons = [],
  mobAgents = [],
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
              {ev.type && (
                <><small>{COLLISION_TYPE_LABEL[ev.type] ?? ev.type}</small><br /></>
              )}
              {typeof ev.angle_deg === "number" && (
                <><small>Angle: {ev.angle_deg.toFixed(0)}°</small><br /></>
              )}
              <small>Tick {ev.tick}</small>
            </Popup>
          </CircleMarker>
        );
      })}

      {/* Wreck (loss) markers — persistent, ✕ over a dark disc. */}
      {wrecks.map((w, i) => (
        <CircleMarker
          key={`wreck-${w.tick}-${i}`}
          center={[w.lat, w.lon]}
          radius={5}
          pathOptions={{
            fillColor: "#1e293b",
            color: "#64748b",
            weight: 1.5,
            fillOpacity: 0.8,
          }}
        >
          <Popup>
            <strong style={{ color: "#94a3b8" }}>☠ Wreck</strong><br />
            <small>Lost with no survivors — tick {w.tick}</small>
          </Popup>
        </CircleMarker>
      ))}

      {/* Dispatched SAR assets — helicopters and patrol boats. */}
      {rescueAgents.map(r => {
        const helo = r.kind === "helicopter";
        return (
          <CircleMarker
            key={`rescue-${r.id}`}
            center={[r.lat, r.lon]}
            radius={7}
            pathOptions={{
              fillColor: "#22c55e",
              color: "#bbf7d0",
              weight: 2,
              fillOpacity: 0.9,
              // Mobilising assets are still at base — render dimmer.
              opacity: r.phase === "mobilising" ? 0.5 : 1,
            }}
          >
            <Popup>
              <strong style={{ color: "#22c55e" }}>
                {helo ? "🚁 Rescue Helicopter" : "🚤 Patrol Boat"}
              </strong><br />
              <small>{RESCUE_PHASE_LABEL[r.phase]}</small>
              {r.target_id != null && (
                <><br /><small>Tasked to vessel #{r.target_id}</small></>
              )}
            </Popup>
          </CircleMarker>
        );
      })}

      {/* Man-overboard searchers — orange helo/patrol markers. */}
      {mobAgents.map(a => (
        <CircleMarker
          key={`mob-agent-${a.id}`}
          center={[a.lat, a.lon]}
          radius={6}
          pathOptions={{
            fillColor: "#fb923c",
            color: "#fed7aa",
            weight: 2,
            fillOpacity: 0.9,
            opacity: a.phase === "mobilising" ? 0.5 : 1,
          }}
        >
          <Popup>
            <strong style={{ color: "#fb923c" }}>
              {a.kind === "helicopter" ? "🚁 MOB Searcher (helo)" : "🚤 MOB Searcher (patrol)"}
            </strong><br />
            <small>{RESCUE_PHASE_LABEL[a.phase]}</small><br />
            <small>Incident #{a.incident_id}</small>
          </Popup>
        </CircleMarker>
      ))}

      {/* Persons in the water — small red life-ring markers. */}
      {mobPersons.map(p => (
        <CircleMarker
          key={`mob-${p.id}`}
          center={[p.lat, p.lon]}
          radius={3}
          pathOptions={{
            fillColor: "#fca5a5",
            color: "#7f1d1d",
            weight: 1,
            fillOpacity: 0.95,
          }}
        >
          <Popup>
            <strong style={{ color: "#fca5a5" }}>🛟 Person in water</strong><br />
            <small>Man-overboard — awaiting rescue</small><br />
            <small>Incident #{p.incident_id}</small>
          </Popup>
        </CircleMarker>
      ))}

      {vessels.map(v => {
        const fill = vesselColor(v);
        // Anchored followers get a cyan ring; avoiding ships an amber ring.
        const ring = v.avoiding ? "#fbbf24"
          : v.anchored ? "#22d3ee"
          : v.state === "Docked" ? "#0f766e"
          : "#1e293b";
        return (
          <CircleMarker
            key={v.id}
            center={[v.lat, v.lon]}
            radius={v.state === "Docked" ? 5 : 6}
            pathOptions={{
              fillColor: fill,
              color: ring,
              weight: (v.avoiding || v.anchored) ? 2 : 1,
              fillOpacity: v.state === "Docked" ? 0.75 : 0.9,
            }}
          >
            <Popup>
              <strong>{v.name}</strong><br />
              State: <b>{v.state}</b>
              {v.avoiding && <><br /><span style={{ color: "#f59e0b", fontWeight: 600 }}>⚠ Collision avoidance active</span></>}
              {v.anchored && <><br /><span style={{ color: "#22d3ee", fontWeight: 600 }}>⚓ Holding station (keep distance)</span></>}<br />
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
