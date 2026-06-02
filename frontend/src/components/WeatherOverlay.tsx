import { memo, useMemo } from "react";
import { ImageOverlay } from "react-leaflet";

interface Props {
  grid: number[];      // flat row-major W ∈ [0,1], length = gridSize²
  gridSize: number;    // cells per side (50)
  latMin: number;
  latMax: number;
  lonMin: number;
  lonMax: number;
  opacity?: number;
}

// Colour ramp: transparent (W=0) → amber → deep red (W=1)
function hazardToRgba(w: number): [number, number, number, number] {
  if (w <= 0.02) return [0, 0, 0, 0];
  const t = Math.min(w, 1);
  const r = Math.round(251 - t * (251 - 127));
  const g = Math.round(191 - t * (191 - 29));
  const b = Math.round(36  - t * (36  - 29));
  const a = Math.round(t * 210);
  return [r, g, b, a];
}

const WeatherOverlay = memo(function WeatherOverlay({
  grid,
  gridSize,
  latMin,
  latMax,
  lonMin,
  lonMax,
  opacity = 0.55,
}: Props) {
  const imageUrl = useMemo(() => {
    if (!grid.length || !gridSize) return null;
    const canvas = document.createElement("canvas");
    canvas.width  = gridSize;
    canvas.height = gridSize;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    const img = ctx.createImageData(gridSize, gridSize);
    const d = img.data;
    for (let row = 0; row < gridSize; row++) {
      for (let col = 0; col < gridSize; col++) {
        // grid row 0 = south (field y=0); Leaflet row 0 = north → flip Y
        const srcRow = gridSize - 1 - row;
        const w = grid[srcRow * gridSize + col] ?? 0;
        const [r, g, b, a] = hazardToRgba(w);
        const i = (row * gridSize + col) * 4;
        d[i]     = r;
        d[i + 1] = g;
        d[i + 2] = b;
        d[i + 3] = a;
      }
    }
    ctx.putImageData(img, 0, 0);
    return canvas.toDataURL("image/png");
  }, [grid, gridSize]);

  if (!imageUrl) return null;

  return (
    <ImageOverlay
      url={imageUrl}
      bounds={[[latMin, lonMin], [latMax, lonMax]]}
      opacity={opacity}
      zIndex={150}
    />
  );
});

export default WeatherOverlay;
