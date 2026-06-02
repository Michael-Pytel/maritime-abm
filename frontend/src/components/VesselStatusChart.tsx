import { memo, useMemo } from "react";
import { PieChart, Pie, Cell, Tooltip } from "recharts";
import type { VesselSnapshot } from "../types";

const STATE_CONFIG = [
  { key: "Active",   label: "Active",      color: "#60a5fa" },
  { key: "Evac",     label: "Evacuating",  color: "#fbbf24" },
  { key: "Sunk",     label: "Sunk",        color: "#f87171" },
  { key: "Rescued",  label: "Rescued",     color: "#34d399" },
];

interface Props {
  vessels: VesselSnapshot[];
}

const VesselStatusChart = memo(function VesselStatusChart({ vessels }: Props) {
  const total = vessels.length;

  const { data, pieData } = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const v of vessels) {
      counts[v.state] = (counts[v.state] ?? 0) + 1;
    }
    const data = STATE_CONFIG.map(({ key, label, color }) => ({
      key, label, color, value: counts[key] ?? 0,
    }));
    return { data, pieData: data.filter(d => d.value > 0) };
  }, [vessels]);

  return (
    <div>
      <div style={{
        fontSize: 10,
        fontWeight: 700,
        color: "#475569",
        letterSpacing: "0.1em",
        textTransform: "uppercase",
        marginBottom: 10,
      }}>
        Fleet Status — {total} vessel{total !== 1 ? "s" : ""}
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
        {/* Donut chart */}
        <div style={{ flexShrink: 0 }}>
          <PieChart width={100} height={100}>
            <Pie
              data={pieData.length > 0 ? pieData : [{ key: "empty", label: "No vessels", color: "#1e293b", value: 1 }]}
              cx={48}
              cy={48}
              innerRadius={28}
              outerRadius={46}
              dataKey="value"
              strokeWidth={0}
            >
              {(pieData.length > 0 ? pieData : [{ key: "empty", color: "#1e293b" }]).map(({ key, color }) => (
                <Cell key={key} fill={color} />
              ))}
            </Pie>
            <Tooltip
              formatter={(v: unknown) => [String(v), "vessels"]}
              contentStyle={{ background: "#0f172a", border: "1px solid #334155", fontSize: 11, borderRadius: 6 }}
            />
          </PieChart>
        </div>

        {/* Stat list */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
          {data.map(({ key, label, color, value }) => (
            <div key={key}>
              <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 3, alignItems: "center" }}>
                <span style={{ fontSize: 11, color: "#94a3b8", display: "flex", alignItems: "center", gap: 5 }}>
                  <span style={{
                    width: 7,
                    height: 7,
                    borderRadius: "50%",
                    background: color,
                    display: "inline-block",
                    flexShrink: 0,
                  }} />
                  {label}
                </span>
                <span style={{ fontSize: 11, fontWeight: 700, color, display: "flex", alignItems: "baseline", gap: 4 }}>
                  {value}
                  {total > 0 && (
                    <span style={{ color: "#475569", fontWeight: 400, fontSize: 9 }}>
                      {((value / total) * 100).toFixed(0)}%
                    </span>
                  )}
                </span>
              </div>
              <div style={{ height: 3, background: "#1e293b", borderRadius: 2, overflow: "hidden" }}>
                <div style={{
                  height: "100%",
                  width: `${total > 0 ? (value / total) * 100 : 0}%`,
                  background: color,
                  borderRadius: 2,
                  transition: "width 0.5s ease",
                }} />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
});

export default VesselStatusChart;
