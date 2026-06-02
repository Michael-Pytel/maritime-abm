import { useEffect, useRef } from "react";
import { useMap } from "react-leaflet";
import L from "leaflet";
import type { WeatherChannel } from "../types";

interface Props {
  grid: number[];
  gridSize: number;
  channel: WeatherChannel | string;
  opacity?: number;
}

const BOUNDS = L.latLngBounds([50.5, -5.0], [66.0, 31.0]);

// Rendered resolution: bilinearly upsampled from gridSize to RENDER_SIZE.
const RENDER_SIZE = 256;

interface ColorStop {
  v: number; // normalised value [0,1]
  r: number; g: number; b: number; a: number;
}

// Per-channel Windy-style colour ramps.  Each stop: { v, r, g, b, a }.
const RAMPS: Record<string, ColorStop[]> = {
  wind: [
    { v: 0.0, r:  30, g:  58, b: 138, a: 30 },
    { v: 0.2, r:  59, g: 130, b: 246, a: 100 },
    { v: 0.4, r:  16, g: 185, b: 129, a: 130 },
    { v: 0.6, r: 251, g: 191, b:  36, a: 150 },
    { v: 0.8, r: 239, g:  68, b:  68, a: 170 },
    { v: 1.0, r: 124, g:  45, b:  18, a: 200 },
  ],
  wave_height: [
    { v: 0.0, r: 240, g: 253, b: 244, a: 20 },
    { v: 0.3, r:  34, g: 211, b: 238, a: 100 },
    { v: 0.6, r:  30, g:  64, b: 175, a: 160 },
    { v: 1.0, r:  30, g:  27, b:  75, a: 200 },
  ],
  sea_state: [
    { v: 0.0, r:  10, g:  60, b: 160, a: 20 },
    { v: 0.5, r:  34, g: 211, b: 238, a: 120 },
    { v: 1.0, r:  30, g:  27, b:  75, a: 200 },
  ],
  visibility: [
    { v: 0.0, r:   0, g:   0, b:   0, a: 200 },
    { v: 0.3, r:  82, g:  82, b:  82, a: 140 },
    { v: 0.7, r: 212, g: 212, b: 212, a: 60  },
    { v: 1.0, r: 255, g: 255, b: 255, a: 10  },
  ],
  precipitation: [
    { v: 0.0, r: 255, g: 255, b: 255, a:  0 },
    { v: 0.3, r: 103, g: 232, b: 249, a: 80 },
    { v: 0.6, r:  37, g:  99, b: 235, a: 160 },
    { v: 1.0, r:  88, g:  28, b: 135, a: 220 },
  ],
  pressure: [
    { v: 0.0, r: 127, g:  29, b:  29, a: 180 },
    { v: 0.5, r: 250, g: 250, b: 250, a: 40  },
    { v: 1.0, r:  30, g:  58, b: 138, a: 180 },
  ],
  tide: [
    { v: 0.0, r: 239, g:  68, b:  68, a: 180 },
    { v: 0.5, r: 250, g: 250, b: 250, a: 30  },
    { v: 1.0, r:  59, g: 130, b: 246, a: 180 },
  ],
  surge: [
    { v: 0.0, r: 250, g: 250, b: 250, a: 20  },
    { v: 0.3, r: 254, g: 243, b: 199, a: 80  },
    { v: 0.6, r: 239, g:  68, b:  68, a: 160 },
    { v: 1.0, r: 127, g:  29, b:  29, a: 220 },
  ],
  total_water_level: [
    { v: 0.0, r: 250, g: 250, b: 250, a: 20  },
    { v: 0.4, r:  34, g: 211, b: 238, a: 100 },
    { v: 0.8, r:  30, g:  64, b: 175, a: 180 },
    { v: 1.0, r: 127, g:  29, b:  29, a: 220 },
  ],
  hazard: [
    { v: 0.0, r:   0, g:  50, b: 200, a: 20  },
    { v: 0.4, r: 255, g: 180, b:   0, a: 120 },
    { v: 0.7, r: 220, g:  40, b:  40, a: 170 },
    { v: 1.0, r: 180, g:  20, b: 180, a: 200 },
  ],
};

function lerp(a: number, b: number, t: number) {
  return a + (b - a) * t;
}

function sampleRamp(stops: ColorStop[], v: number): [number, number, number, number] {
  const clamped = Math.max(0, Math.min(1, v));
  if (clamped <= stops[0].v) return [stops[0].r, stops[0].g, stops[0].b, stops[0].a];
  const last = stops[stops.length - 1];
  if (clamped >= last.v) return [last.r, last.g, last.b, last.a];
  for (let i = 1; i < stops.length; i++) {
    if (clamped <= stops[i].v) {
      const prev = stops[i - 1];
      const curr = stops[i];
      const t = (clamped - prev.v) / (curr.v - prev.v);
      return [
        Math.round(lerp(prev.r, curr.r, t)),
        Math.round(lerp(prev.g, curr.g, t)),
        Math.round(lerp(prev.b, curr.b, t)),
        Math.round(lerp(prev.a, curr.a, t)),
      ];
    }
  }
  return [last.r, last.g, last.b, last.a];
}

// Bilinear upsample from gridSize→RENDER_SIZE, with y-flip (row0=south).
function fillImageData(
  data: Uint8ClampedArray,
  grid: number[],
  gridSize: number,
  stops: ColorStop[],
) {
  const out = RENDER_SIZE;
  for (let row = 0; row < out; row++) {
    for (let col = 0; col < out; col++) {
      // Map render pixel → grid coordinates.
      const gx = col * (gridSize - 1) / (out - 1);
      const gy = row * (gridSize - 1) / (out - 1);
      const gx0 = Math.floor(gx);
      const gy0 = Math.floor(gy);
      const gx1 = Math.min(gx0 + 1, gridSize - 1);
      const gy1 = Math.min(gy0 + 1, gridSize - 1);
      const fx = gx - gx0;
      const fy = gy - gy0;

      // y-flip: row 0 in grid = south, but canvas row 0 = north.
      const sg = gridSize;
      const r0 = gridSize - 1 - gy0;
      const r1 = gridSize - 1 - gy1;
      const v00 = grid[Math.max(0, r0) * sg + gx0];
      const v01 = grid[Math.max(0, r0) * sg + gx1];
      const v10 = grid[Math.max(0, r1) * sg + gx0];
      const v11 = grid[Math.max(0, r1) * sg + gx1];

      const top    = (v00 ?? 0) * (1 - fx) + (v01 ?? 0) * fx;
      const bottom = (v10 ?? 0) * (1 - fx) + (v11 ?? 0) * fx;
      const v = top * (1 - fy) + bottom * fy;

      const [r, g, b, a] = sampleRamp(stops, v);
      const idx = (row * out + col) * 4;
      data[idx]     = r;
      data[idx + 1] = g;
      data[idx + 2] = b;
      data[idx + 3] = a;
    }
  }
}

// Normalise raw channel values to [0,1] for ramp lookup.
// For channels measured in physical units (hPa, m), map to a display range.
function normalise(channel: string, v: number): number {
  switch (channel) {
    case "pressure":        return ((v || 1013) - 950) / 80;  // 950–1030 hPa
    case "tide":            return ((v || 0) + 2) / 4;         // -2..+2 m → 0..1
    case "surge":           return ((v || 0) + 1) / 4;         // -1..+3 m → 0..1
    case "total_water_level": return ((v || 0) + 2) / 6;       // -2..+4 m → 0..1
    case "wave_height":     return Math.min((v || 0) / 12, 1); // 0..12 m → 0..1
    case "wave_period":     return Math.min((v || 0) / 20, 1); // 0..20 s → 0..1
    default: return v || 0;  // already [0,1]
  }
}

export default function GradientOverlay({
  grid,
  gridSize,
  channel = "hazard",
  opacity = 0.65,
}: Props) {
  const map = useMap();
  const overlayRef  = useRef<L.ImageOverlay | null>(null);
  const canvasRef   = useRef<HTMLCanvasElement | null>(null);
  const imgDataRef  = useRef<ImageData | null>(null);

  useEffect(() => {
    if (!grid || grid.length === 0) return;

    if (!canvasRef.current) {
      const canvas = document.createElement("canvas");
      canvas.width  = RENDER_SIZE;
      canvas.height = RENDER_SIZE;
      canvasRef.current = canvas;
    }
    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    if (!imgDataRef.current) {
      imgDataRef.current = ctx.createImageData(RENDER_SIZE, RENDER_SIZE);
    }
    const imageData = imgDataRef.current;

    const stops = RAMPS[channel] ?? RAMPS.hazard;

    // Normalise grid if needed then fill.
    let workGrid = grid;
    if (
      channel === "pressure" || channel === "tide" || channel === "surge" ||
      channel === "total_water_level" || channel === "wave_height" || channel === "wave_period"
    ) {
      workGrid = grid.map(v => normalise(channel, v));
    }

    fillImageData(imageData.data, workGrid, gridSize, stops);
    ctx.putImageData(imageData, 0, 0);

    // Use createObjectURL instead of toDataURL to avoid base64 encoding overhead.
    canvas.toBlob(blob => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      if (overlayRef.current) {
        const oldUrl = (overlayRef.current as any)._url;
        overlayRef.current.setUrl(url);
        overlayRef.current.setOpacity(opacity);
        if (oldUrl?.startsWith("blob:")) URL.revokeObjectURL(oldUrl);
      } else {
        overlayRef.current = L.imageOverlay(url, BOUNDS, {
          opacity,
          interactive: false,
          className: "gradient-overlay",
        }).addTo(map);
      }
    });
  }, [grid, gridSize, channel, opacity, map]);

  useEffect(() => {
    return () => {
      const url = (overlayRef.current as any)?._url;
      overlayRef.current?.remove();
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    };
  }, [map]);

  return null;
}
