import { useMemo } from "react";
import type { TickMessage, KpiSnapshot, TelemetrySnapshot, VesselMsg } from "../types";

export type TelemetryPoint = TelemetrySnapshot & { step: number };

export interface RunSeries {
  kpiHistory: KpiSnapshot[];
  telemetryHistory: TelemetryPoint[];
  commsLog: VesselMsg[];
}

/**
 * Derives the per-tick time-series that drive the Telemetry charts, live KPI
 * island and Comms feed from a playback run's ticks. Previously inline in App;
 * extracted so both the bottom island and the Statistics panel can share it.
 */
export function useRunSeries(ticks: TickMessage[]): RunSeries {
  const kpiHistory = useMemo<KpiSnapshot[]>(
    () => ticks.map(t => t.kpis).filter(Boolean),
    [ticks],
  );
  const telemetryHistory = useMemo<TelemetryPoint[]>(
    () => ticks.filter(t => t.telemetry).map(t => ({ ...(t.telemetry as TelemetrySnapshot), step: t.step })),
    [ticks],
  );
  const commsLog = useMemo<VesselMsg[]>(() => ticks.flatMap(t => t.comms_log ?? []), [ticks]);

  return { kpiHistory, telemetryHistory, commsLog };
}
