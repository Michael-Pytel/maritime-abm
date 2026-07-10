import { lazy, Suspense, useMemo } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import type { RunSeries } from "../hooks/useRunSeries";
import CommsLog from "./CommsLog";
import { color, radius, shadow } from "../theme/tokens";

const TelemetryPanel = lazy(() => import("./TelemetryPanel"));

type PB = ReturnType<typeof usePlayback>;
export type IslandTab = "telemetry" | "comms";

interface Props {
  pb: PB;
  series: RunSeries;
  expanded: boolean;
  tab: IslandTab;
  onToggle: () => void;
  onTab: (t: IslandTab) => void;
}

/**
 * Floating bottom "island": a live KPI strip (from the current playback tick)
 * that expands into a Telemetry / Comms dashboard. All content is sliced to the
 * scrubber position so it grows as playback advances.
 */
export default function StatsIsland({ pb, series, expanded, tab, onToggle, onTab }: Props) {
  const k = pb.currentTick?.kpis;
  const step = pb.currentTick?.step ?? 0;
  const hasRun = pb.ticks.length > 0;

  // Slice the series up to the scrubber so charts/feed reflect "now".
  const upto = pb.currentIdx + 1;
  const kpiSlice = useMemo(() => series.kpiHistory.slice(0, upto), [series.kpiHistory, upto]);
  const telSlice = useMemo(() => series.telemetryHistory.slice(0, upto), [series.telemetryHistory, upto]);
  const commsSlice = useMemo(() => series.commsLog.filter(m => m.tick <= step), [series.commsLog, step]);

  const tiles: { label: string; value: string; c: string }[] = k ? [
    { label: "Coll /1k", value: k.collision_per_1k_hrs.toFixed(2), c: color.bad },
    { label: "Fatal /1k", value: k.fatal_per_1k_hrs.toFixed(2), c: "#fb7185" },
    { label: "Survival", value: k.survival_ratio.toFixed(3), c: color.good },
    { label: "Avg TTA h", value: k.avg_tta_hours.toFixed(2), c: "#60a5fa" },
    { label: "Evac", value: k.evac_activation_rate.toFixed(3), c: color.warn },
  ] : [];

  return (
    <div style={{
      width: expanded ? "min(880px, 78vw)" : "auto",
      background: color.panel + "f2", border: `1px solid ${color.borderBlue}`,
      borderRadius: radius.lg, boxShadow: shadow, overflow: "hidden",
    }}>
      {/* Expanded dashboard grows upward, above the strip. */}
      {expanded && (
        <div style={{ borderBottom: `1px solid ${color.border}` }}>
          <div style={{ display: "flex", gap: 2, padding: "8px 10px 0" }}>
            {(["telemetry", "comms"] as IslandTab[]).map(t => (
              <button key={t} onClick={() => onTab(t)} style={{
                padding: "5px 14px", fontSize: 11, fontWeight: tab === t ? 700 : 400,
                background: "transparent", border: "none", cursor: "pointer",
                color: tab === t ? color.text : color.muted,
                borderBottom: `2px solid ${tab === t ? color.accent : "transparent"}`,
              }}>
                {t === "telemetry" ? "≋ Telemetry" : "📡 Comms"}
              </button>
            ))}
          </div>
          <div style={{ maxHeight: "46vh", overflowY: "auto", padding: tab === "telemetry" ? 14 : 0 }}>
            {!hasRun ? (
              <div style={{ padding: 24, fontSize: 12, color: color.muted }}>Select a run and press play to see it evolve.</div>
            ) : tab === "telemetry" ? (
              <Suspense fallback={<div style={{ padding: 20, fontSize: 12, color: color.faint }}>Loading…</div>}>
                <TelemetryPanel kpiHistory={kpiSlice} telemetryHistory={telSlice} />
              </Suspense>
            ) : (
              <div style={{ height: "46vh" }}><CommsLog messages={commsSlice} /></div>
            )}
          </div>
        </div>
      )}

      {/* Collapsed strip — always visible. */}
      <div style={{ display: "flex", alignItems: "center", gap: 18, padding: "8px 12px" }}>
        {hasRun && k ? (
          tiles.map(t => (
            <div key={t.label} style={{ display: "flex", flexDirection: "column" }}>
              <span style={{ fontSize: 8.5, color: color.faint, textTransform: "uppercase", letterSpacing: "0.06em" }}>{t.label}</span>
              <span style={{ fontSize: 14, fontWeight: 700, color: t.c, fontVariantNumeric: "tabular-nums", lineHeight: 1.15 }}>{t.value}</span>
            </div>
          ))
        ) : (
          <span style={{ fontSize: 11, color: color.muted, padding: "3px 2px" }}>Select a run to view live statistics</span>
        )}
        <span style={{ flex: 1, minWidth: 12 }} />
        {hasRun && (
          <span style={{ fontSize: 9.5, color: color.faint, fontVariantNumeric: "tabular-nums" }}>tick {step}</span>
        )}
        <button
          onClick={onToggle}
          title={expanded ? "Collapse" : "Expand dashboard"}
          style={{
            width: 28, height: 28, borderRadius: radius.sm, cursor: "pointer",
            background: expanded ? color.panelSel : "transparent",
            border: `1px solid ${color.border}`, color: color.textDim, fontSize: 11,
          }}
        >{expanded ? "▾" : "▴"}</button>
      </div>
    </div>
  );
}
