import { useEffect, useRef, memo } from "react";
import { useMap } from "react-leaflet";
import L from "leaflet";

interface Props {
  pressureGrid: number[];   // hPa values, length = gridSize²
  gridSize: number;
  intervalHPa?: number;
  updateIntervalMs?: number;
}

// Manual marching-squares contour tracing.
// We avoid importing d3-contour to keep the bundle small.
// This implementation finds crossing segments for each isoline level
// and emits them as SVG path strings in Leaflet container space.

function marchingSquares(
  grid: number[],
  gs: number,
  level: number,
): Array<[number, number][]> {
  // Returns array of polyline coordinate pairs [col, row] (fractional).
  const lines: Array<[number, number][]> = [];

  const idx = (r: number, c: number) => r * gs + c;

  // Edge interpolation: where does the isoline cross the edge from (r0,c0) to (r1,c1)?
  function interp(r0: number, c0: number, r1: number, c1: number): [number, number] {
    const v0 = grid[idx(r0, c0)] ?? 0;
    const v1 = grid[idx(r1, c1)] ?? 0;
    const t = v0 === v1 ? 0.5 : (level - v0) / (v1 - v0);
    return [c0 + (c1 - c0) * t, r0 + (r1 - r0) * t];
  }

  for (let r = 0; r < gs - 1; r++) {
    for (let c = 0; c < gs - 1; c++) {
      const v00 = grid[idx(r,   c  )] ?? 0;
      const v10 = grid[idx(r+1, c  )] ?? 0;
      const v01 = grid[idx(r,   c+1)] ?? 0;
      const v11 = grid[idx(r+1, c+1)] ?? 0;

      const code =
        (v00 >= level ? 8 : 0) |
        (v01 >= level ? 4 : 0) |
        (v11 >= level ? 2 : 0) |
        (v10 >= level ? 1 : 0);

      if (code === 0 || code === 15) continue;

      // Top edge, Right edge, Bottom edge, Left edge crossings.
      const top    = () => interp(r,   c,   r,   c+1);
      const right  = () => interp(r,   c+1, r+1, c+1);
      const bottom = () => interp(r+1, c,   r+1, c+1);
      const left   = () => interp(r,   c,   r+1, c  );

      const segs: Array<[[number,number], [number,number]]> = [];

      switch (code) {
        case 1:  case 14: segs.push([bottom(), left()]); break;
        case 2:  case 13: segs.push([right(),  bottom()]); break;
        case 3:  case 12: segs.push([right(),  left()]); break;
        case 4:  case 11: segs.push([top(),    right()]); break;
        case 5:           segs.push([top(), left()], [right(), bottom()]); break;
        case 6:  case 9:  segs.push([top(),    bottom()]); break;
        case 7:  case 8:  segs.push([top(),    left()]); break;
        case 10:          segs.push([top(), right()], [bottom(), left()]); break;
      }

      for (const [a, b] of segs) {
        lines.push([a, b]);
      }
    }
  }
  return lines;
}

// Convert fractional grid col/row to lat/lon.
function gridToLatLon(
  col: number, row: number,
  gs: number,
  latMin: number, latMax: number,
  lonMin: number, lonMax: number,
): [number, number] {
  // row 0 in grid = south (latMin); canvas convention is also south at row 0.
  const lat = latMin + (row / (gs - 1)) * (latMax - latMin);
  const lon = lonMin + (col / (gs - 1)) * (lonMax - lonMin);
  return [lat, lon];
}

const LAT_MIN = 50.5;
const LAT_MAX = 66.0;
const LON_MIN = -5.0;
const LON_MAX = 31.0;

const IsobarLayer = memo(function IsobarLayer({
  pressureGrid,
  gridSize,
  intervalHPa = 4,
  updateIntervalMs = 5000,
}: Props) {
  const map     = useRef<L.Map | null>(null);
  const mapHook = useMap();
  const svgRef  = useRef<SVGSVGElement | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Keep a stable reference to the map.
  useEffect(() => { map.current = mapHook; }, [mapHook]);

  function redraw() {
    const m = map.current;
    if (!m || !pressureGrid.length) return;

    const minP = Math.floor(pressureGrid.reduce((a, b) => Math.min(a, b), Infinity) / intervalHPa) * intervalHPa;
    const maxP = Math.ceil(pressureGrid.reduce((a, b) => Math.max(a, b), -Infinity) / intervalHPa) * intervalHPa;

    const levels: number[] = [];
    for (let p = minP; p <= maxP; p += intervalHPa) {
      levels.push(p);
    }

    // Build SVG paths.
    const paths: string[] = [];
    for (const level of levels) {
      const segs = marchingSquares(pressureGrid, gridSize, level);
      for (const seg of segs) {
        const [a, b] = seg;
        const [lat0, lon0] = gridToLatLon(a[0], a[1], gridSize, LAT_MIN, LAT_MAX, LON_MIN, LON_MAX);
        const [lat1, lon1] = gridToLatLon(b[0], b[1], gridSize, LAT_MIN, LAT_MAX, LON_MIN, LON_MAX);
        const p0 = m.latLngToLayerPoint([lat0, lon0]);
        const p1 = m.latLngToLayerPoint([lat1, lon1]);
        const isHigh = level >= 1013;
        const stroke = isHigh ? "#7f9cf5" : "#94a3b8";
        const dash   = isHigh ? "" : "4,4";
        paths.push(
          `<line x1="${p0.x}" y1="${p0.y}" x2="${p1.x}" y2="${p1.y}" stroke="${stroke}" stroke-width="1" opacity="0.7"${dash ? ` stroke-dasharray="${dash}"` : ""}/>`,
        );
      }
    }

    if (svgRef.current) {
      const size = m.getSize();
      svgRef.current.setAttribute("width", String(size.x));
      svgRef.current.setAttribute("height", String(size.y));
      svgRef.current.innerHTML = paths.join("");
    }
  }

  useEffect(() => {
    const m = mapHook;
    if (!m) return;

    // Create an SVG element in the overlay pane.
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.style.position = "absolute";
    svg.style.top = "0";
    svg.style.left = "0";
    svg.style.pointerEvents = "none";
    svg.style.zIndex = "420";
    svgRef.current = svg;

    const pane = m.getPane("overlayPane");
    pane?.appendChild(svg);

    const onMoveEnd = () => redraw();
    m.on("moveend zoomend", onMoveEnd);

    redraw();
    timerRef.current = setInterval(redraw, updateIntervalMs);

    return () => {
      m.off("moveend zoomend", onMoveEnd);
      if (timerRef.current) clearInterval(timerRef.current);
      pane?.removeChild(svg);
    };
  }, [mapHook]); // eslint-disable-line react-hooks/exhaustive-deps

  // Redraw when data changes.
  useEffect(() => { redraw(); }, [pressureGrid, gridSize]); // eslint-disable-line react-hooks/exhaustive-deps

  return null;
});

export default IsobarLayer;
