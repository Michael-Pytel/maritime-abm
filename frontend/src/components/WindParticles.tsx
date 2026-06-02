import { useEffect, useRef, memo } from "react";

interface Props {
  windGrid: number[];           // normalized magnitude [0,1]
  windDirGrid: number[];        // radians
  gridSize: number;
  bboxLatMin: number;
  bboxLatMax: number;
  bboxLonMin: number;
  bboxLonMax: number;
  particleCount?: number;
  particleSeed?: number;
}

interface Particle {
  x: number;    // canvas pixel
  y: number;
  px: number;   // previous position
  py: number;
  age: number;
  maxAge: number;
}

// Fast seeded RNG (xorshift32).
function makeRng(seed: number) {
  let s = (seed | 0) || 1;
  return () => {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    return (s >>> 0) / 0x100000000;
  };
}

// Bilinear sample of a gridded field at fractional pixel position.
function sampleGrid(
  grid: number[],
  gridSize: number,
  gx: number,
  gy: number,
): number {
  const ix0 = Math.max(0, Math.min(gridSize - 1, Math.floor(gx)));
  const iy0 = Math.max(0, Math.min(gridSize - 1, Math.floor(gy)));
  const ix1 = Math.min(gridSize - 1, ix0 + 1);
  const iy1 = Math.min(gridSize - 1, iy0 + 1);
  const fx = gx - ix0;
  const fy = gy - iy0;
  // Row 0 in grid = south; canvas row 0 = north → flip y.
  const r0 = gridSize - 1 - iy0;
  const r1 = gridSize - 1 - iy1;
  const v00 = grid[Math.max(0, r0) * gridSize + ix0] ?? 0;
  const v01 = grid[Math.max(0, r0) * gridSize + ix1] ?? 0;
  const v10 = grid[Math.max(0, r1) * gridSize + ix0] ?? 0;
  const v11 = grid[Math.max(0, r1) * gridSize + ix1] ?? 0;
  return (v00 * (1 - fx) + v01 * fx) * (1 - fy) + (v10 * (1 - fx) + v11 * fx) * fy;
}

// Wind speed → RGBA colour (blue → cyan → green → yellow → red).
function windColour(mag: number): string {
  const v = Math.max(0, Math.min(1, mag));
  if (v < 0.25) {
    const t = v / 0.25;
    return `rgba(${Math.round(59 + t * (16 - 59))},${Math.round(130 + t * (185 - 130))},${Math.round(246 + t * (129 - 246))},0.85)`;
  } else if (v < 0.5) {
    const t = (v - 0.25) / 0.25;
    return `rgba(${Math.round(16 + t * (251 - 16))},${Math.round(185 + t * (191 - 185))},${Math.round(129 + t * (36 - 129))},0.85)`;
  } else if (v < 0.75) {
    const t = (v - 0.5) / 0.25;
    return `rgba(${Math.round(251 + t * (239 - 251))},${Math.round(191 + t * (68 - 191))},${Math.round(36 + t * (68 - 36))},0.85)`;
  } else {
    const t = (v - 0.75) / 0.25;
    return `rgba(${Math.round(239 + t * (124 - 239))},${Math.round(68 + t * (45 - 68))},${Math.round(68 + t * (18 - 68))},0.85)`;
  }
}

const WindParticles = memo(function WindParticles({
  windGrid,
  windDirGrid,
  gridSize,
  bboxLatMin, bboxLatMax, bboxLonMin, bboxLonMax,
  particleCount = 2000,
  particleSeed  = 42,
}: Props) {
  const canvasRef    = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const frameRef     = useRef<number>(0);
  const particlesRef = useRef<Particle[]>([]);
  const rngRef       = useRef<() => number>(() => 0);

  // Initialise particles when seed or count changes.
  useEffect(() => {
    const rng = makeRng(particleSeed);
    rngRef.current = rng;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const w = canvas.width;
    const h = canvas.height;
    particlesRef.current = Array.from({ length: particleCount }, () => {
      const x = rng() * w;
      const y = rng() * h;
      const maxAge = 30 + Math.floor(rng() * 30);
      return { x, y, px: x, py: y, age: Math.floor(rng() * maxAge), maxAge };
    });
  }, [particleSeed, particleCount]);

  // Animation loop — runs at display refresh rate, independent of sim tick.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const w = canvas.width;
    const h = canvas.height;
    const rng = rngRef.current;

    // Dt: pixels per frame at 60 fps (wind speed ×0.4 of canvas width per minute)
    const DT = 0.5;

    function frame() {
      // Fade trail: semi-transparent black fill preserves recent streaks.
      ctx!.fillStyle = "rgba(0,0,0,0.88)";
      ctx!.fillRect(0, 0, w, h);

      for (const p of particlesRef.current) {
        // Map canvas pixel to fractional grid coordinates.
        const gx = (p.x / w) * (gridSize - 1);
        const gy = (p.y / h) * (gridSize - 1);

        const mag = sampleGrid(windGrid,    gridSize, gx, gy);
        const dir = sampleGrid(windDirGrid, gridSize, gx, gy);

        // Advance particle.
        p.px = p.x;
        p.py = p.y;
        p.x += Math.cos(dir) * mag * DT * (w / 500);
        p.y -= Math.sin(dir) * mag * DT * (h / 400); // canvas y flipped
        p.age += 1;

        // Respawn if expired or off-canvas.
        if (p.age >= p.maxAge || p.x < 0 || p.x > w || p.y < 0 || p.y > h) {
          p.x = p.px = rng() * w;
          p.y = p.py = rng() * h;
          p.age = 0;
          p.maxAge = 30 + Math.floor(rng() * 30);
          continue;
        }

        // Draw streak from previous to current position.
        ctx!.beginPath();
        ctx!.moveTo(p.px, p.py);
        ctx!.lineTo(p.x, p.y);
        ctx!.strokeStyle = windColour(mag);
        ctx!.lineWidth = 1;
        ctx!.stroke();
      }

      frameRef.current = requestAnimationFrame(frame);
    }

    frameRef.current = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(frameRef.current);
  }, [windGrid, windDirGrid, gridSize]);

  // Resize canvas to match container.
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;
    const obs = new ResizeObserver(entries => {
      for (const e of entries) {
        canvas.width  = Math.round(e.contentRect.width);
        canvas.height = Math.round(e.contentRect.height);
        // Re-seed particles on resize.
        const rng = rngRef.current;
        const w = canvas.width;
        const h = canvas.height;
        particlesRef.current = particlesRef.current.map(() => {
          const x = rng() * w;
          const y = rng() * h;
          const maxAge = 30 + Math.floor(rng() * 30);
          return { x, y, px: x, py: y, age: 0, maxAge };
        });
      }
    });
    obs.observe(container);
    return () => obs.disconnect();
  }, []);

  return (
    <div
      ref={containerRef}
      style={{ position: "absolute", inset: 0, pointerEvents: "none", zIndex: 400 }}
    >
      <canvas
        ref={canvasRef}
        style={{ position: "absolute", inset: 0, width: "100%", height: "100%" }}
      />
    </div>
  );
});

export default WindParticles;
