import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import type { TickMessage, RunManifest, WeatherChannels } from "../types";

const API = "http://localhost:3000";

export const SPEED_PRESETS = [
  { label: "Very Slow", ms: 1000 },
  { label: "Slow",      ms: 500  },
  { label: "Normal",    ms: 250  },
  { label: "Fast",      ms: 120  },
  { label: "Very Fast", ms: 50   },
] as const;

export const STRIDE_OPTIONS = [1, 2, 4, 8] as const;

export interface LayerToggles {
  rescueAssets: boolean;
  sosMarkers: boolean;
  radioRangeCircles: boolean;
  shoreBroadcastCircles: boolean;
  weatherOverlay: boolean;
  stormCenters: boolean;
}

export function usePlayback() {
  const [runs, setRuns] = useState<RunManifest[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [ticks, setTicks] = useState<TickMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [currentIdx, setCurrentIdx] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speedMs, setSpeedMs] = useState(250);
  const [stride, setStride] = useState(1);
  const [weatherChannel, setWeatherChannel] = useState<keyof WeatherChannels>("hazard");
  const [layers, setLayers] = useState<LayerToggles>({
    rescueAssets: true,
    sosMarkers: true,
    radioRangeCircles: false,
    shoreBroadcastCircles: true,
    weatherOverlay: true,
    stormCenters: true,
  });

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refreshRuns = useCallback(() => {
    fetch(`${API}/sim/runs`)
      .then(r => r.json() as Promise<RunManifest[]>)
      .then(setRuns)
      .catch(() => {});
  }, []);

  useEffect(() => { refreshRuns(); }, [refreshRuns]);

  useEffect(() => {
    if (!selectedRunId) return;
    setLoading(true);
    setTicks([]);
    setCurrentIdx(0);
    setPlaying(false);
    fetch(`${API}/sim/runs/${selectedRunId}/log`)
      .then(r => r.text())
      .then(text => {
        const parsed: TickMessage[] = text
          .split("\n")
          .filter(l => l.trim().length > 0)
          .flatMap(l => {
            try { return [JSON.parse(l) as TickMessage]; }
            catch { return []; }
          });
        setTicks(parsed);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [selectedRunId]);

  useEffect(() => {
    if (intervalRef.current) clearInterval(intervalRef.current);
    if (playing && ticks.length > 0) {
      intervalRef.current = setInterval(() => {
        setCurrentIdx(i => {
          const next = i + stride;
          if (next >= ticks.length) {
            setPlaying(false);
            return ticks.length - 1;
          }
          return next;
        });
      }, speedMs);
    }
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [playing, speedMs, stride, ticks.length]);

  const toggleLayer = useCallback((key: keyof LayerToggles) => {
    setLayers(prev => ({ ...prev, [key]: !prev[key] }));
  }, []);

  const currentTick = ticks[currentIdx] ?? null;
  const manifest = selectedRunId
    ? (runs.find(r => r.run_id === selectedRunId) ?? null)
    : null;

  // Use real weather_channels from the log if available, otherwise approximate from hazard grid.
  const derivedWeatherChannels = useMemo(() => {
    const channels = currentTick?.weather_channels;
    if (channels && channels.wind?.length) return channels;
    const grid = currentTick?.weather_grid;
    if (!grid?.length) return null;
    return {
      hazard:          grid,
      sea_state:       grid.map((h: number) => h * 0.8),
      wind:            grid.map((h: number) => h * 0.6),
      wind_direction:  grid.map(() => Math.PI / 4),
      visibility:      grid.map((h: number) => Math.max(0, 1 - h * 0.7)),
      precipitation:   grid.map((h: number) => h * 0.5),
      pressure:        grid.map((h: number) => 1013 - h * 30),
      tide:            grid.map(() => 0),
      surge:           grid.map((h: number) => h * 0.3),
      total_water_level: grid.map((h: number) => h * 0.3),
      wave_height:     grid.map((h: number) => h * 8),
      wave_period:     grid.map(() => 8),
    };
  }, [currentTick?.weather_grid, currentTick?.weather_channels]);

  return {
    runs,
    refreshRuns,
    selectedRunId,
    setSelectedRunId,
    ticks,
    loading,
    currentIdx,
    setCurrentIdx,
    playing,
    setPlaying,
    speedMs,
    setSpeedMs,
    stride,
    setStride,
    weatherChannel,
    setWeatherChannel,
    layers,
    toggleLayer,
    currentTick,
    manifest,
    derivedWeatherChannels,
  };
}
