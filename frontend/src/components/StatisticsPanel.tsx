import type { usePlayback } from "../hooks/usePlayback";
import type { KpiSnapshot, TelemetrySnapshot } from "../types";
import StatsPanel from "./StatsPanel";
import { color, radius, KPI_META, METHOD_COLORS, SCEN_SHORT, methodShort } from "../theme/tokens";

type PB = ReturnType<typeof usePlayback>;

/** Left-drawer "Statistics": selected-run summary on top, batch aggregate below. */
export default function StatisticsPanel({ pb }: { pb: PB }) {
  const m = pb.manifest;
  const finalTel = pb.ticks.length ? pb.ticks[pb.ticks.length - 1]?.telemetry : undefined;

  return (
    <div style={{ height: "100%", overflowY: "auto", padding: "14px 16px" }}>
      <RunSummary manifest={m} kpis={m?.kpis} telemetry={finalTel} />
      <div style={{ height: 1, background: color.border, margin: "18px 0" }} />
      <StatsPanel defaultScenario={m?.scenario} />
    </div>
  );
}

function RunSummary({
  manifest, kpis, telemetry,
}: {
  manifest: PB["manifest"];
  kpis?: KpiSnapshot;
  telemetry?: TelemetrySnapshot;
}) {
  if (!manifest || !kpis) {
    return (
      <div style={{ fontSize: 12, color: color.muted, lineHeight: 1.6 }}>
        Select a run to see its summary. Aggregate statistics across all seeds are shown below.
      </div>
    );
  }
  const col = METHOD_COLORS[manifest.method] ?? color.accent;
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 3 }}>
        <span style={{ width: 8, height: 8, borderRadius: "50%", background: col }} />
        <span style={{ fontSize: 15, fontWeight: 700, color: color.text }}>
          {SCEN_SHORT[manifest.scenario] ?? manifest.scenario}
        </span>
        <span style={{ fontSize: 12, color: col, fontWeight: 600 }}>{methodShort(manifest.method)}</span>
        <span style={{ fontSize: 12, color: color.faint }}>#{manifest.seed}</span>
      </div>
      <div style={{ fontSize: 10, color: color.faint, marginBottom: 12 }}>
        {manifest.n_vessels} vessels · {manifest.n_ticks} ticks
      </div>

      {/* KPI tiles */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        {Object.entries(KPI_META).map(([key, meta]) => {
          const v = (kpis as unknown as Record<string, number>)[key];
          return (
            <div key={key} style={{ background: color.panel2, border: `1px solid ${color.border}`, borderRadius: radius.md, padding: "8px 10px" }}>
              <div style={{ fontSize: 9, color: color.faint, textTransform: "uppercase", letterSpacing: "0.05em" }}>{meta.label}</div>
              <div style={{ fontSize: 16, fontWeight: 700, color: meta.color, fontVariantNumeric: "tabular-nums", lineHeight: 1.3 }}>
                {typeof v === "number" && isFinite(v) ? v.toFixed(3) : "—"}
                {meta.unit && <span style={{ fontSize: 9, color: color.muted, marginLeft: 3 }}>{meta.unit}</span>}
              </div>
            </div>
          );
        })}
      </div>

      {/* Final outcome counts */}
      {telemetry && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 14, marginTop: 12 }}>
          <Outcome label="Rescued" value={telemetry.rescued_cumulative} c={color.good} />
          <Outcome label="Lost" value={telemetry.lost_cumulative} c={color.muted} />
          <Outcome label="Fatal" value={telemetry.fatal_cumulative} c={color.bad} />
          <Outcome label="MOB recov." value={telemetry.mob_recovered_cumulative} c={color.good} />
          <Outcome label="Wrecks" value={telemetry.wreck_count} c={color.faint} />
        </div>
      )}
    </div>
  );
}

function Outcome({ label, value, c }: { label: string; value?: number; c: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      <span style={{ fontSize: 9, color: color.faint, textTransform: "uppercase", letterSpacing: "0.05em" }}>{label}</span>
      <span style={{ fontSize: 15, fontWeight: 700, color: c, fontVariantNumeric: "tabular-nums" }}>{value ?? 0}</span>
    </div>
  );
}
