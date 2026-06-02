import { memo, useMemo } from "react";
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from "recharts";
import type { KpiSnapshot } from "../types";

interface Props {
  history: KpiSnapshot[];
}

type KpiKey = keyof KpiSnapshot;

const KPI_DEFS: {
  key: KpiKey;
  label: string;
  unit: string;
  color: string;
  bg: string;
  higherBetter: boolean;
}[] = [
  { key: "survival_ratio",       label: "Survival Ratio",      unit: "",        color: "#34d399", bg: "#022c22", higherBetter: true  },
  { key: "fatal_per_1k_hrs",     label: "Fatalities",          unit: "/1k hrs", color: "#f87171", bg: "#450a0a", higherBetter: false },
  { key: "mean_p_prep",          label: "Mean Preparedness",   unit: "",        color: "#60a5fa", bg: "#0c1a35", higherBetter: true  },
  { key: "collision_per_1k_hrs", label: "Collisions",          unit: "/1k hrs", color: "#fb923c", bg: "#431407", higherBetter: false },
  { key: "avg_tta_hours",        label: "Alert Lead Time",      unit: "hrs",     color: "#fbbf24", bg: "#451a03", higherBetter: true  },
  { key: "evac_activation_rate", label: "Evac Activation",     unit: "rate",    color: "#c084fc", bg: "#2e1065", higherBetter: false },
];

const KpiPanel = memo(function KpiPanel({ history }: Props) {
  const latest = useMemo(() => history[history.length - 1], [history]);
  const prev   = useMemo(
    () => history.length >= 20 ? history[history.length - 20] : history[0],
    [history],
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{
        fontSize: 10,
        fontWeight: 700,
        color: "#475569",
        letterSpacing: "0.1em",
        textTransform: "uppercase",
      }}>
        Performance Indicators
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        {KPI_DEFS.map(({ key, label, unit, color, bg, higherBetter }) => {
          const val = latest?.[key];
          const prevVal = prev?.[key];
          const delta =
            typeof val === "number" && typeof prevVal === "number" && history.length >= 2
              ? val - prevVal
              : null;
          const displayVal = typeof val === "number" ? val.toFixed(3) : "—";
          const hasDelta = delta !== null && Math.abs(delta) > 1e-6;
          const isGood = hasDelta ? (higherBetter ? delta! > 0 : delta! < 0) : null;
          const deltaColor = isGood === null ? "#64748b" : isGood ? "#34d399" : "#f87171";
          const deltaStr = hasDelta ? `${delta! > 0 ? "+" : ""}${delta!.toFixed(3)}` : null;

          return (
            <div
              key={key}
              style={{
                background: bg,
                borderRadius: 8,
                padding: "10px 10px 6px",
                border: `1px solid ${color}18`,
                borderLeft: `3px solid ${color}`,
              }}
            >
              <div style={{ fontSize: 9, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 3 }}>
                {label}
                {unit && <span style={{ marginLeft: 3, color: "#475569" }}>({unit})</span>}
              </div>
              <div style={{ display: "flex", alignItems: "baseline", gap: 5, marginBottom: 4 }}>
                <span style={{
                  fontSize: 19,
                  fontWeight: 700,
                  color,
                  lineHeight: 1,
                  fontVariantNumeric: "tabular-nums",
                }}>
                  {displayVal}
                </span>
                {deltaStr && (
                  <span style={{ fontSize: 9, color: deltaColor, lineHeight: 1 }}>
                    {deltaStr}
                  </span>
                )}
              </div>
              <ResponsiveContainer width="100%" height={34}>
                <AreaChart data={history} margin={{ top: 2, right: 0, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id={`grad_${key as string}`} x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor={color} stopOpacity={0.45} />
                      <stop offset="100%" stopColor={color} stopOpacity={0.02} />
                    </linearGradient>
                  </defs>
                  <XAxis dataKey="step" hide />
                  <YAxis hide domain={["auto", "auto"]} />
                  <Tooltip
                    contentStyle={{
                      background: "#0f172a",
                      border: `1px solid ${color}50`,
                      fontSize: 10,
                      padding: "3px 8px",
                      borderRadius: 6,
                    }}
                    formatter={(v: unknown) => [
                      typeof v === "number" ? v.toFixed(4) : String(v),
                      label,
                    ]}
                    labelFormatter={(l) => `Step ${l}`}
                  />
                  <Area
                    type="monotone"
                    dataKey={key as string}
                    stroke={color}
                    strokeWidth={1.5}
                    fill={`url(#grad_${key as string})`}
                    dot={false}
                    isAnimationActive={false}
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          );
        })}
      </div>
    </div>
  );
});

export default KpiPanel;
