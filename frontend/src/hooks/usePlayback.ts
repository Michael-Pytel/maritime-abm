import { useState, useEffect, useRef, useCallback } from "react";
import type { TickMessage, RunManifest } from "../types";

const API = "http://localhost:3000";

export const SPEED_PRESETS = [
  { label: "Very Slow", ms: 1000 },
  { label: "Slow",      ms: 500  },
  { label: "Normal",    ms: 250  },
  { label: "Fast",      ms: 120  },
  { label: "Very Fast", ms: 50   },
] as const;

export function usePlayback() {
  const [runs, setRuns] = useState<RunManifest[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [ticks, setTicks] = useState<TickMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [currentIdx, setCurrentIdx] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speedMs, setSpeedMs] = useState(250);

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
    let cancelled = false;

    // Start the fetch immediately, but defer state resets to a microtask so
    // no setState is called synchronously in the effect body.
    const fetchPromise = fetch(`${API}/sim/runs/${selectedRunId}/log`).then(r => r.text());

    Promise.resolve().then(() => {
      if (cancelled) return;
      setLoading(true);
      setTicks([]);
      setCurrentIdx(0);
      setPlaying(false);
    });

    fetchPromise
      .then(text => {
        if (cancelled) return;
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
      .finally(() => { if (!cancelled) setLoading(false); });

    return () => { cancelled = true; };
  }, [selectedRunId]);

  useEffect(() => {
    if (intervalRef.current) clearInterval(intervalRef.current);
    if (playing && ticks.length > 0) {
      intervalRef.current = setInterval(() => {
        setCurrentIdx(i => {
          const next = i + 1;
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
  }, [playing, speedMs, ticks.length]);

  const currentTick = ticks[currentIdx] ?? null;
  const manifest = selectedRunId
    ? (runs.find(r => r.run_id === selectedRunId) ?? null)
    : null;

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
    currentTick,
    manifest,
  };
}
