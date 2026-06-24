import { memo } from "react";
import {
  AreaChart, Area,
  LineChart, Line,
  BarChart, Bar,
  XAxis, YAxis, Tooltip, ResponsiveContainer, Legend,
} from "recharts";
import type { KpiSnapshot, TelemetrySnapshot } from "../types";

type TelemetryPoint = TelemetrySnapshot & { step: number };

interface Props {
  kpiHistory: KpiSnapshot[];
  telemetryHistory: TelemetryPoint[];
}

const CHART_STYLE = {
  background: "#0d1526",
  border: "1px solid #1e293b",
  borderRadius: 8,
  padding: "14px 16px",
};

const AXIS_STYLE = { fontSize: 9, fill: "#475569" };
const TOOLTIP_STYLE = {
  contentStyle: { background: "#0d1526", border: "1px solid #1e293b", fontSize: 11, color: "#f1f5f9" },
  labelStyle: { color: "#94a3b8" },
};

function ChartTitle({ children }: { children: string }) {
  return (
    <div style={{
      fontSize: 10, fontWeight: 700, color: "#475569",
      textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 10,
    }}>
      {children}
    </div>
  );
}

const TelemetryPanel = memo(function TelemetryPanel({ kpiHistory, telemetryHistory }: Props) {
  const noData = kpiHistory.length === 0 && telemetryHistory.length === 0;

  if (noData) {
    return (
      <div style={{
        display: "flex", alignItems: "center", justifyContent: "center",
        height: 300, color: "#475569", fontSize: 13,
      }}>
        No live data — start a simulation to see telemetry.
      </div>
    );
  }

  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "1fr 1fr",
      gap: 16,
    }}>
      {/* 1. Cumulative outcomes */}
      <div style={CHART_STYLE}>
        <ChartTitle>Cumulative Outcomes</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <AreaChart data={telemetryHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={28} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Area
              type="monotone" dataKey="rescued_cumulative" name="Rescued"
              stroke="#22c55e" fill="#22c55e" fillOpacity={0.15} strokeWidth={1.5} dot={false}
            />
            <Area
              type="monotone" dataKey="lost_cumulative" name="Lost"
              stroke="#64748b" fill="#64748b" fillOpacity={0.15} strokeWidth={1.5} dot={false}
            />
            <Area
              type="monotone" dataKey="fatal_cumulative" name="Fatal"
              stroke="#ef4444" fill="#ef4444" fillOpacity={0.15} strokeWidth={1.5} dot={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>

      {/* 2. Vessel state timeline */}
      <div style={CHART_STYLE}>
        <ChartTitle>Vessel States</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <LineChart data={telemetryHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={28} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Line
              type="monotone" dataKey="active_count" name="Active"
              stroke="#3b82f6" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="evac_count" name="Evac"
              stroke="#f59e0b" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="anchored_count" name="Anchored"
              stroke="#22d3ee" strokeWidth={1.2} dot={false} strokeDasharray="4 3"
            />
          </LineChart>
        </ResponsiveContainer>
      </div>

      {/* 3. KPI trends */}
      <div style={CHART_STYLE}>
        <ChartTitle>KPI Trends</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <LineChart data={kpiHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={36} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Line
              type="monotone" dataKey="survival_ratio" name="Survival ratio"
              stroke="#34d399" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="fatal_per_1k_hrs" name="Fatal/1k hrs"
              stroke="#f87171" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="mean_p_prep" name="Mean P_prep"
              stroke="#60a5fa" strokeWidth={1.2} dot={false} strokeDasharray="4 3"
            />
          </LineChart>
        </ResponsiveContainer>
      </div>

      {/* 4. Rescue activity */}
      <div style={CHART_STYLE}>
        <ChartTitle>Active Rescue Assets</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <BarChart data={telemetryHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={28} allowDecimals={false} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Bar
              dataKey="rescue_agent_count" name="Rescue assets"
              fill="#22c55e" fillOpacity={0.75} radius={[2, 2, 0, 0]}
            />
          </BarChart>
        </ResponsiveContainer>
      </div>

      {/* 5. Collision geometry breakdown (cumulative) */}
      <div style={CHART_STYLE}>
        <ChartTitle>Collision Types (cumulative)</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <LineChart data={telemetryHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={28} allowDecimals={false} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Line
              type="monotone" dataKey="collisions_head_on" name="Head-on"
              stroke="#ef4444" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="collisions_front_to_side" name="Front-to-side"
              stroke="#f59e0b" strokeWidth={1.5} dot={false}
            />
            <Line
              type="monotone" dataKey="collisions_side_to_side" name="Side-to-side"
              stroke="#60a5fa" strokeWidth={1.5} dot={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>

      {/* 6. Man-overboard outcomes */}
      <div style={CHART_STYLE}>
        <ChartTitle>Man-Overboard Outcomes</ChartTitle>
        <ResponsiveContainer width="100%" height={180}>
          <AreaChart data={telemetryHistory} margin={{ top: 4, right: 8, bottom: 0, left: 0 }}>
            <XAxis dataKey="step" tick={AXIS_STYLE} />
            <YAxis tick={AXIS_STYLE} width={28} allowDecimals={false} />
            <Tooltip {...TOOLTIP_STYLE} />
            <Legend wrapperStyle={{ fontSize: 10 }} />
            <Area
              type="monotone" dataKey="mob_recovered_cumulative" name="Recovered"
              stroke="#22c55e" fill="#22c55e" fillOpacity={0.15} strokeWidth={1.5} dot={false}
            />
            <Area
              type="monotone" dataKey="mob_lost_cumulative" name="Lost"
              stroke="#ef4444" fill="#ef4444" fillOpacity={0.15} strokeWidth={1.5} dot={false}
            />
            <Area
              type="monotone" dataKey="mob_in_water" name="In water"
              stroke="#fb923c" fill="#fb923c" fillOpacity={0.1} strokeWidth={1.2} dot={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
});

export default TelemetryPanel;
