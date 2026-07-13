import { useMemo, lazy, Suspense } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import type { RunSeries } from "../hooks/useRunSeries";
import RunsPanel from "./RunsPanel";
import CommsLog from "./CommsLog";
import PlaybackDock from "./PlaybackDock";
import { color, brandGradient } from "../theme/tokens";

const TelemetryPanel = lazy(() => import("./TelemetryPanel"));
const StatisticsPanel = lazy(() => import("./StatisticsPanel"));

type PB = ReturnType<typeof usePlayback>;
export type LeftTab = "runs" | "stats" | "telemetry" | "comms";

interface Props {
  pb: PB;
  series: RunSeries;
  tab: LeftTab;
  onTab: (t: LeftTab) => void;
}

const TABS: { id: LeftTab; label: string }[] = [
  { id: "runs", label: "Runs" },
  { id: "stats", label: "Statistics" },
  { id: "telemetry", label: "Telemetry" },
  { id: "comms", label: "Comms" },
];

/** The permanent left panel: brand + map controls, a tab strip, the active
 *  tab's content, and a pinned playback dock at the bottom. */
export default function LeftPanel({ pb, series, tab, onTab }: Props) {
  const hasRun = pb.ticks.length > 0;
  const step = pb.currentTick?.step ?? 0;
  const upto = pb.currentIdx + 1;

  // Slice the derived series to the scrubber position so panels grow with playback.
  const kpiSlice = useMemo(() => series.kpiHistory.slice(0, upto), [series.kpiHistory, upto]);
  const telSlice = useMemo(() => series.telemetryHistory.slice(0, upto), [series.telemetryHistory, upto]);
  const commsSlice = useMemo(() => series.commsLog.filter(m => m.tick <= step), [series.commsLog, step]);

  return (
    <div style={{
      width: 404, flexShrink: 0, height: "100vh",
      display: "flex", flexDirection: "column",
      background: color.panel, borderRight: `1px solid ${color.border}`,
      position: "relative", zIndex: 5,
    }}>
      <Header />
      <TabStrip tab={tab} onTab={onTab} />

      <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column" }}>
        {tab === "runs" && <RunsPanel pb={pb} />}

        {tab === "stats" && (
          <Suspense fallback={<Loading />}>
            <StatisticsPanel pb={pb} />
          </Suspense>
        )}

        {tab === "telemetry" && (
          <div style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: 14 }}>
            {hasRun ? (
              <Suspense fallback={<Loading />}>
                <TelemetryPanel kpiHistory={kpiSlice} telemetryHistory={telSlice} />
              </Suspense>
            ) : (
              <Empty icon="≋" text="Select a run and press play — the telemetry charts grow as it advances." />
            )}
          </div>
        )}

        {tab === "comms" && <CommsLog messages={commsSlice} />}
      </div>

      <PlaybackDock pb={pb} />
    </div>
  );
}

// ── Header: brand mark ────────────────────────────────────────────────────────

function Header() {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 10,
      padding: "13px 14px", borderBottom: `1px solid ${color.border}`,
    }}>
      <div style={{
        width: 32, height: 32, borderRadius: 9, flexShrink: 0,
        background: brandGradient, display: "flex", alignItems: "center", justifyContent: "center",
        boxShadow: `0 2px 12px ${color.accent}44`,
      }}>
        <RadarGlyph />
      </div>
      <div style={{ display: "flex", flexDirection: "column", lineHeight: 1.15 }}>
        <span style={{ fontSize: 13.5, fontWeight: 700, color: color.text, letterSpacing: "-0.01em" }}>Maritime ABM</span>
        <span style={{ fontSize: 8.5, color: color.muted, letterSpacing: "0.16em", textTransform: "uppercase" }}>Safety Simulation</span>
      </div>
    </div>
  );
}

function RadarGlyph() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="#04252c" strokeWidth="2" strokeLinecap="round">
      <circle cx="12" cy="12" r="9" opacity="0.9" />
      <circle cx="12" cy="12" r="4" opacity="0.55" />
      <path d="M12 12 L12 3" />
      <path d="M12 12 L19 15.5" opacity="0.7" />
    </svg>
  );
}

// ── Tab strip ─────────────────────────────────────────────────────────────────

function TabStrip({ tab, onTab }: { tab: LeftTab; onTab: (t: LeftTab) => void }) {
  return (
    <div style={{ display: "flex", padding: "0 8px", borderBottom: `1px solid ${color.border}`, gap: 2 }}>
      {TABS.map(t => {
        const active = tab === t.id;
        return (
          <button
            key={t.id}
            onClick={() => onTab(t.id)}
            style={{
              flex: 1, padding: "10px 4px", fontSize: 11,
              fontWeight: active ? 700 : 500,
              background: "transparent", border: "none", cursor: "pointer",
              color: active ? color.text : color.muted,
              borderBottom: `2px solid ${active ? color.accent : "transparent"}`,
              marginBottom: -1,
            }}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

function Loading() {
  return <div style={{ padding: 18, fontSize: 12, color: color.faint }}>Loading…</div>;
}

function Empty({ icon, text }: { icon: string; text: string }) {
  return (
    <div style={{
      height: "100%", display: "flex", flexDirection: "column",
      alignItems: "center", justifyContent: "center", gap: 10,
      color: color.faint, textAlign: "center", padding: 24,
    }}>
      <span style={{ fontSize: 26, opacity: 0.7 }}>{icon}</span>
      <span style={{ fontSize: 12, maxWidth: 240, lineHeight: 1.5 }}>{text}</span>
    </div>
  );
}
