import { useMemo, useRef, useState, useEffect, useCallback, useImperativeHandle, forwardRef } from "react";
import DeckGL from "@deck.gl/react";
import { Map } from "react-map-gl/maplibre";
import { ScatterplotLayer, IconLayer, PathLayer, BitmapLayer, WebMercatorViewport } from "deck.gl";
import "maplibre-gl/dist/maplibre-gl.css";
import type {
  VesselSnapshot, Port, CollisionEvent, Storm,
  RescueAgentSnapshot, WreckMarker, MobPersonSnapshot, MobAgentSnapshot,
} from "../types";

// Keyless dark vector style (OpenFreeMap). Avoids raster CDN watermark tiles.
const DARK_STYLE = "https://tiles.openfreemap.org/styles/dark";

// ── Simulated domain (matches crates/simulation/src/ais.rs bbox) ──────────────
const DOMAIN = { lonMin: -5.0, latMin: 50.5, lonMax: 31.0, latMax: 66.0 };
const MARGIN = 0;             // no slack — the domain edge is the hard limit
const MAX_ZOOM = 9;
// Domain expanded by the margin — the hard clamp box for panning.
const B = {
  lonMin: DOMAIN.lonMin - (DOMAIN.lonMax - DOMAIN.lonMin) * MARGIN,
  lonMax: DOMAIN.lonMax + (DOMAIN.lonMax - DOMAIN.lonMin) * MARGIN,
  latMin: DOMAIN.latMin - (DOMAIN.latMax - DOMAIN.latMin) * MARGIN,
  latMax: DOMAIN.latMax + (DOMAIN.latMax - DOMAIN.latMin) * MARGIN,
};

interface ViewState {
  longitude: number;
  latitude: number;
  zoom: number;
  pitch?: number;
  bearing?: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * The min-zoom view: the domain COVERS the viewport (fills both axes, cropping
 * the overflow) rather than being contained inside it — so there is never any
 * blank map beyond the weather overlay. `fitBounds` gives a "contain" fit; we
 * then bump the zoom until the domain also fills the slack axis.
 */
function fitViewState(width: number, height: number): ViewState {
  const w = Math.max(1, width), h = Math.max(1, height);
  const base = new WebMercatorViewport({ width: w, height: h }).fitBounds(
    [[DOMAIN.lonMin, DOMAIN.latMin], [DOMAIN.lonMax, DOMAIN.latMax]],
    { padding: 0 },
  );
  const vp = new WebMercatorViewport({ longitude: base.longitude, latitude: base.latitude, zoom: base.zoom, width: w, height: h });
  const tl = vp.project([DOMAIN.lonMin, DOMAIN.latMax]);
  const br = vp.project([DOMAIN.lonMax, DOMAIN.latMin]);
  const spanX = Math.max(1, Math.abs(br[0] - tl[0]));
  const spanY = Math.max(1, Math.abs(br[1] - tl[1]));
  const bump = Math.max(0, Math.log2(Math.max(w / spanX, h / spanY)));
  const zoom = Math.min(MAX_ZOOM, base.zoom + bump);
  return { longitude: base.longitude, latitude: base.latitude, zoom, pitch: 0, bearing: 0 };
}

/** Clamp a proposed view so it never leaves the domain (zoom ≥ all-ports fit). */
function constrainViewState(vs: ViewState, width: number, height: number, minZoom: number): ViewState {
  const zoom = clamp(vs.zoom, minZoom, MAX_ZOOM);
  if (width < 2 || height < 2) return { ...vs, zoom, pitch: 0, bearing: 0 };
  // Guard against out-of-Mercator-range input (WebMercatorViewport asserts |lat|<85);
  // the centre is domain-clamped below regardless, this just keeps span-measurement safe.
  const safeLon = clamp(vs.longitude, -180, 180);
  const safeLat = clamp(vs.latitude, -85, 85);
  const vp = new WebMercatorViewport({ longitude: safeLon, latitude: safeLat, zoom, width, height, pitch: 0, bearing: 0 });
  const tl = vp.unproject([0, 0]);
  const br = vp.unproject([width, height]);
  const halfLon = Math.abs(br[0] - tl[0]) / 2;
  const halfLat = Math.abs(tl[1] - br[1]) / 2;
  const longitude = (B.lonMax - B.lonMin) <= 2 * halfLon
    ? (B.lonMin + B.lonMax) / 2
    : clamp(vs.longitude, B.lonMin + halfLon, B.lonMax - halfLon);
  const latitude = (B.latMax - B.latMin) <= 2 * halfLat
    ? (B.latMin + B.latMax) / 2
    : clamp(vs.latitude, B.latMin + halfLat, B.latMax - halfLat);
  return { longitude, latitude, zoom, pitch: 0, bearing: 0 };
}

const COLLISION_FADE_TICKS = 40;

type RGB = [number, number, number];
const C = {
  active: [59, 130, 246] as RGB,
  docked: [20, 184, 166] as RGB,
  evac: [251, 191, 36] as RGB,
  rescued: [52, 211, 153] as RGB,
  lost: [100, 116, 139] as RGB,
  avoiding: [245, 158, 11] as RGB,
  anchored: [34, 211, 238] as RGB,
  port: [148, 163, 184] as RGB,
  collision: [239, 68, 68] as RGB,
  wreck: [100, 116, 139] as RGB,
  rescue: [34, 197, 94] as RGB,
  mobAgent: [251, 146, 60] as RGB,
  mobPerson: [252, 165, 165] as RGB,
  route: [56, 116, 168] as RGB,
};
/** Per-vessel-type route tint (mirrors theme/tokens TYPE_HEX). */
const ROUTE_TYPE: Record<string, RGB> = {
  cargo: [59, 130, 246],
  passenger: [34, 197, 94],
  tanker: [249, 115, 22],
};

function vesselColor(v: VesselSnapshot): RGB {
  if (v.avoiding) return C.avoiding;
  switch (v.state) {
    case "Docked": return C.docked;
    case "Evac": return C.evac;
    case "Rescued": return C.rescued;
    case "Lost": return C.lost;
    default: return C.active;
  }
}

/** Hazard colour ramp: transparent (calm) → amber → deep red (severe). */
function hazardToRgba(w: number): [number, number, number, number] {
  if (w <= 0.02) return [0, 0, 0, 0];
  const t = Math.min(w, 1);
  return [
    Math.round(251 - t * (251 - 127)),
    Math.round(191 - t * (191 - 29)),
    Math.round(36 - t * (36 - 29)),
    Math.round(t * 210),
  ];
}

const RESCUE_PHASE_LABEL: Record<RescueAgentSnapshot["phase"], string> = {
  mobilising: "Mobilising at base",
  transiting: "Transiting to datum",
  searching: "Searching on scene",
  embarking: "Embarking survivors",
};
const COLLISION_TYPE_LABEL: Record<string, string> = {
  head_on: "Head-on",
  front_to_side: "Front-to-side (crossing)",
  side_to_side: "Side-to-side (overtaking)",
};

/** A white kite/arrow pointing north, tinted per-vessel via `mask: true`. */
const VESSEL_ICON =
  "data:image/svg+xml;charset=utf-8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">' +
      '<polygon points="12,1 20,23 12,18 4,23" fill="white"/></svg>',
  );
/** Single-entry static atlas mapping so the icon is packed once, not per frame. */
const VESSEL_ICON_MAPPING = {
  v: { x: 0, y: 0, width: 24, height: 24, anchorX: 12, anchorY: 12, mask: true },
};

/** A route as an ordered list of [lon, lat] pairs. */
export interface RoutePath {
  name: string;
  vessel_type: string;
  path: [number, number][];
}

/** Imperative camera controls exposed to the map toolbar. */
export interface MapHandle {
  fitDomain: () => void;
  zoomBy: (delta: number) => void;
}

interface Props {
  vessels: VesselSnapshot[];
  ports?: Port[];
  routes?: RoutePath[];
  collisionEvents?: CollisionEvent[];
  rescueAgents?: RescueAgentSnapshot[];
  wrecks?: WreckMarker[];
  mobPersons?: MobPersonSnapshot[];
  mobAgents?: MobAgentSnapshot[];
  currentStep?: number;
  storm?: Storm | null;
  showRoutes?: boolean;
  showLegend?: boolean;
  /** Flat row-major hazard grid W∈[0,1] and its side length, for the overlay. */
  weatherGrid?: number[];
  weatherGridSize?: number;
  showWeather?: boolean;
  latMin?: number;
  latMax?: number;
  lonMin?: number;
  lonMax?: number;
  /** Duration (ms) deck.gl glides each moving object between playback frames.
   *  Match it to the playback frame interval so motion is continuous. */
  transitionMs?: number;
}

const MapGL = forwardRef<MapHandle, Props>(function MapGL({
  vessels,
  ports = [],
  routes = [],
  collisionEvents = [],
  rescueAgents = [],
  wrecks = [],
  mobPersons = [],
  mobAgents = [],
  currentStep = 0,
  storm = null,
  showRoutes = true,
  showLegend = true,
  weatherGrid = [],
  weatherGridSize = 0,
  showWeather = true,
  latMin = DOMAIN.latMin,
  latMax = DOMAIN.latMax,
  lonMin = DOMAIN.lonMin,
  lonMax = DOMAIN.lonMax,
  transitionMs = 250,
}: Props, ref) {
  // ── Controlled + domain-constrained camera ───────────────────────────────
  const containerRef = useRef<HTMLDivElement>(null);
  const dimsRef = useRef({ width: 0, height: 0 });
  const minZoomRef = useRef(3.6);
  const initedRef = useRef(false);
  const [viewState, setViewState] = useState<ViewState>({ longitude: 12.5, latitude: 58.3, zoom: 4.1, pitch: 0, bearing: 0 });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const apply = () => {
      const w = el.clientWidth, h = el.clientHeight;
      if (w < 2 || h < 2) return;
      dimsRef.current = { width: w, height: h };
      const fit = fitViewState(w, h);
      minZoomRef.current = fit.zoom;
      setViewState(prev => {
        if (!initedRef.current) { initedRef.current = true; return fit; }
        return constrainViewState(prev, w, h, fit.zoom);
      });
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const handleViewStateChange = useCallback((params: { viewState: ViewState }) => {
    const { width, height } = dimsRef.current;
    setViewState(constrainViewState(params.viewState, width, height, minZoomRef.current));
  }, []);

  useImperativeHandle(ref, () => ({
    fitDomain: () => {
      const { width, height } = dimsRef.current;
      setViewState(fitViewState(width, height));
    },
    zoomBy: (delta: number) => {
      const { width, height } = dimsRef.current;
      setViewState(prev => constrainViewState({ ...prev, zoom: prev.zoom + delta }, width, height, minZoomRef.current));
    },
  }), []);

  // Hazard field as an RGBA texture (deck.gl bilinearly smooths it on the GPU).
  const weatherImage = useMemo(() => {
    if (!showWeather || !weatherGrid.length || !weatherGridSize) return null;
    const s = weatherGridSize;
    const data = new Uint8ClampedArray(s * s * 4);
    for (let row = 0; row < s; row++) {
      for (let col = 0; col < s; col++) {
        const w = weatherGrid[(s - 1 - row) * s + col] ?? 0; // grid row 0 = south
        const [r, g, b, a] = hazardToRgba(w);
        const i = (row * s + col) * 4;
        data[i] = r; data[i + 1] = g; data[i + 2] = b; data[i + 3] = a;
      }
    }
    return new ImageData(data, s, s);
  }, [weatherGrid, weatherGridSize, showWeather]);
  // GPU position-tween applied to every moving layer (stable across renders).
  const move = useMemo(
    () => ({ getPosition: { type: "interpolation" as const, duration: transitionMs } }),
    [transitionMs],
  );
  const layers = useMemo(() => {
    const ls: unknown[] = [];

    // Weather hazard field — under everything else.
    if (weatherImage) {
      ls.push(
        new BitmapLayer({
          id: "weather",
          image: weatherImage,
          bounds: [lonMin, latMin, lonMax, latMax],
          opacity: 0.5,
        }),
      );
    }

    // Faint AIS route network (context, like MarineTraffic), tinted by type.
    if (showRoutes && routes.length > 0) {
      ls.push(
        new PathLayer<RoutePath>({
          id: "routes",
          data: routes,
          getPath: (d) => d.path,
          getColor: (d) => [...(ROUTE_TYPE[d.vessel_type.toLowerCase()] ?? C.route), 80] as [number, number, number, number],
          getWidth: 1,
          widthUnits: "pixels",
          widthMinPixels: 1,
          jointRounded: true,
          capRounded: true,
          pickable: true,
        }),
      );
    }

    // Storm zone — outer haze + inner core (radii in metres).
    if (storm) {
      const m = storm.radius_nm * 1852;
      ls.push(
        new ScatterplotLayer({
          id: "storm-haze",
          data: [storm],
          getPosition: (d: Storm) => [d.lon, d.lat],
          getRadius: m,
          radiusUnits: "meters",
          getFillColor: [124, 58, 237, 28],
          getLineColor: [167, 139, 250, 130],
          lineWidthMinPixels: 1,
          stroked: true,
          filled: true,
          transitions: move,
        }),
        new ScatterplotLayer({
          id: "storm-core",
          data: [storm],
          getPosition: (d: Storm) => [d.lon, d.lat],
          getRadius: m * 0.45,
          radiusUnits: "meters",
          getFillColor: [109, 40, 217, 46],
          stroked: false,
          transitions: move,
        }),
      );
    }

    // Ports.
    ls.push(
      new ScatterplotLayer<Port>({
        id: "ports",
        data: ports,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 4,
        radiusUnits: "pixels",
        getFillColor: [...C.port, 165] as [number, number, number, number],
        getLineColor: [71, 85, 105, 255],
        lineWidthMinPixels: 1,
        stroked: true,
        pickable: true,
      }),
    );

    // Collision events — red, fading with age.
    ls.push(
      new ScatterplotLayer<CollisionEvent>({
        id: "collisions",
        data: collisionEvents,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 9,
        radiusUnits: "pixels",
        getFillColor: (d) => {
          const a = Math.max(0, 1 - (currentStep - d.tick) / COLLISION_FADE_TICKS);
          return [...C.collision, Math.round(a * 140)] as [number, number, number, number];
        },
        stroked: true,
        getLineColor: [252, 165, 165, 200],
        lineWidthMinPixels: 2,
        updateTriggers: { getFillColor: currentStep },
        pickable: true,
      }),
    );

    // Wrecks.
    ls.push(
      new ScatterplotLayer<WreckMarker>({
        id: "wrecks",
        data: wrecks,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 5,
        radiusUnits: "pixels",
        getFillColor: [30, 41, 59, 210],
        getLineColor: [100, 116, 139, 255],
        lineWidthMinPixels: 1,
        stroked: true,
        pickable: true,
      }),
    );

    // SAR assets + MOB searchers + persons in water.
    ls.push(
      new ScatterplotLayer<RescueAgentSnapshot>({
        id: "rescue",
        data: rescueAgents,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 7,
        radiusUnits: "pixels",
        getFillColor: (d) => [...C.rescue, d.phase === "mobilising" ? 120 : 235] as [number, number, number, number],
        getLineColor: [187, 247, 208, 255],
        lineWidthMinPixels: 2,
        stroked: true,
        pickable: true,
        transitions: move,
      }),
      new ScatterplotLayer<MobAgentSnapshot>({
        id: "mob-agents",
        data: mobAgents,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 6,
        radiusUnits: "pixels",
        getFillColor: (d) => [...C.mobAgent, d.phase === "mobilising" ? 120 : 235] as [number, number, number, number],
        getLineColor: [254, 215, 170, 255],
        lineWidthMinPixels: 2,
        stroked: true,
        pickable: true,
        transitions: move,
      }),
      new ScatterplotLayer<MobPersonSnapshot>({
        id: "mob-persons",
        data: mobPersons,
        getPosition: (d) => [d.lon, d.lat],
        getRadius: 3,
        radiusUnits: "pixels",
        getFillColor: [...C.mobPerson, 240] as [number, number, number, number],
        getLineColor: [127, 29, 29, 255],
        lineWidthMinPixels: 1,
        stroked: true,
        pickable: true,
        transitions: move,
      }),
    );

    // Halo ring for avoiding / anchored vessels (drawn under the icons).
    // No position tween — interpolating a filtered subset makes rings slide
    // between unrelated vessels when the avoiding set changes each frame.
    const flagged = vessels.filter((v) => v.avoiding || v.anchored);
    if (flagged.length > 0) {
      ls.push(
        new ScatterplotLayer<VesselSnapshot>({
          id: "vessel-halo",
          data: flagged,
          getPosition: (d) => [d.lon, d.lat],
          getRadius: 11,
          radiusUnits: "pixels",
          filled: false,
          stroked: true,
          getLineColor: (d) => (d.avoiding ? [...C.avoiding, 230] : [...C.anchored, 230]) as [number, number, number, number],
          lineWidthMinPixels: 2,
        }),
      );
    }

    // Vessels — directional triangles tinted by state. A STATIC icon atlas
    // (one shared icon, `getIcon` returns a constant key) avoids deck.gl
    // re-packing the atlas every frame — the difference between smooth and laggy.
    ls.push(
      new IconLayer<VesselSnapshot>({
        id: "vessels",
        data: vessels,
        iconAtlas: VESSEL_ICON,
        iconMapping: VESSEL_ICON_MAPPING,
        getIcon: () => "v",
        getPosition: (d) => [d.lon, d.lat],
        getSize: (d) => (d.state === "Docked" ? 13 : 17),
        sizeUnits: "pixels",
        getAngle: (d) => -d.heading_deg,
        getColor: (d) => vesselColor(d),
        pickable: true,
        transitions: move,
      }),
    );

    return ls;
  }, [vessels, ports, routes, collisionEvents, rescueAgents, wrecks, mobPersons, mobAgents, storm, showRoutes, currentStep, move, weatherImage, lonMin, latMin, lonMax, latMax]);

  return (
    <div ref={containerRef} style={{ position: "absolute", inset: 0 }}>
      <DeckGL
        viewState={viewState as never}
        onViewStateChange={handleViewStateChange as never}
        controller={{ dragRotate: false, touchRotate: false } as never}
        layers={layers as never}
        getTooltip={getTooltip}
        style={{ position: "absolute", inset: "0" }}
      >
        <Map reuseMaps mapStyle={DARK_STYLE} />
      </DeckGL>
      {showLegend && <MapLegend />}
    </div>
  );
});

export default MapGL;

function rgb(c: RGB, a = 1) {
  return `rgba(${c[0]},${c[1]},${c[2]},${a})`;
}

function LegendDot({ color, label, ring }: { color: RGB; label: string; ring?: RGB }) {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 5, whiteSpace: "nowrap" }}>
      <span style={{
        width: 9, height: 9, borderRadius: "50%",
        background: ring ? "transparent" : rgb(color),
        border: ring ? `2px solid ${rgb(ring)}` : "none",
      }} />
      {label}
    </span>
  );
}

function MapLegend() {
  return (
    <div style={{
      position: "absolute", bottom: 12, left: 12, zIndex: 1, pointerEvents: "none",
      background: "#10151ee6", border: "1px solid #1b3a45", borderRadius: 10,
      padding: "9px 12px", fontSize: 9.5, color: "#c7d0db",
      display: "flex", gap: 18, lineHeight: 1.9,
      boxShadow: "0 6px 24px rgba(0,0,0,.45)",
    }}>
      <div style={{ display: "flex", flexDirection: "column" }}>
        <LegendDot color={C.active} label="Active" />
        <LegendDot color={C.docked} label="Docked" />
        <LegendDot color={C.evac} label="Evac (liferaft)" />
        <LegendDot color={C.rescued} label="Rescued" />
        <LegendDot color={C.lost} label="Lost" />
      </div>
      <div style={{ display: "flex", flexDirection: "column" }}>
        <LegendDot color={C.active} ring={C.avoiding} label="Avoiding" />
        <LegendDot color={C.active} ring={C.anchored} label="Holding station" />
        <LegendDot color={C.rescue} label="SAR asset" />
        <LegendDot color={C.mobPerson} label="Person in water" />
        <LegendDot color={C.collision} label="Collision" />
      </div>
    </div>
  );
}

interface PickInfo {
  object?: unknown;
  layer?: { id: string } | null;
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

function getTooltip(info: PickInfo): { html: string; style: Record<string, string> } | null {
  const { object, layer } = info;
  if (!object || !layer) return null;
  const box = (title: string, body: string) => ({
    html: `<div style="font-weight:700;margin-bottom:2px">${title}</div><div style="opacity:.8">${body}</div>`,
    style: TOOLTIP_STYLE,
  });
  switch (layer.id) {
    case "vessels": {
      const v = object as VesselSnapshot;
      const flags = [
        v.avoiding ? "⚠ avoiding" : "",
        v.anchored ? "⚓ holding station" : "",
      ].filter(Boolean).join(" · ");
      return box(v.name, `${v.state}${flags ? " · " + flags : ""} · crew ${v.n_crew}`);
    }
    case "ports": return box((object as Port).name, "Port");
    case "collisions": {
      const e = object as CollisionEvent;
      const t = e.type ? COLLISION_TYPE_LABEL[e.type] ?? e.type : "";
      return box("⚠ Collision", `${t}${e.type ? " · " : ""}tick ${e.tick}`);
    }
    case "wrecks": return box("☠ Wreck", `Lost with no survivors · tick ${(object as WreckMarker).tick}`);
    case "rescue": {
      const r = object as RescueAgentSnapshot;
      return box(r.kind === "helicopter" ? "🚁 Rescue helicopter" : "🚤 Patrol boat", RESCUE_PHASE_LABEL[r.phase]);
    }
    case "mob-agents": {
      const a = object as MobAgentSnapshot;
      return box("MOB searcher", `${RESCUE_PHASE_LABEL[a.phase]} · incident #${a.incident_id}`);
    }
    case "mob-persons": return box("🛟 Person in water", `Incident #${(object as MobPersonSnapshot).incident_id}`);
    case "routes": {
      const r = object as RoutePath;
      return box(r.name, r.vessel_type);
    }
    default: return null;
  }
}
