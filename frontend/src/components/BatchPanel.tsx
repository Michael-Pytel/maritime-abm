import { useState, useEffect, useRef } from "react";
import type { BatchStatus, BatchResult, KpiSnapshot } from "../types";
import {
  BarChart, Bar, XAxis, YAxis, Tooltip, Legend,
  ResponsiveContainer, Cell,
} from "recharts";

const API = "http://localhost:3000";

const KPI_OPTIONS: { key: keyof KpiSnapshot; label: string; higherBetter: boolean }[] = [
  { key: "survival_ratio",       label: "Survival Ratio",       higherBetter: true  },
  { key: "fatal_per_1k_hrs",     label: "Fatalities /1k hrs",   higherBetter: false },
  { key: "mean_p_prep",          label: "Mean Preparedness",    higherBetter: true  },
  { key: "collision_per_1k_hrs", label: "Collisions /1k hrs",   higherBetter: false },
  { key: "avg_tta_hours",        label: "Avg Time-to-Rescue",   higherBetter: false },
  { key: "evac_activation_rate", label: "Evac Activation Rate", higherBetter: false },
];

const METHOD_COLORS: Record<string, string> = {
  BaselineA:      "#6b7280",
  BaselineB:      "#f59e0b",
  ProposedSystem: "#3b82f6",
};

const ALL_SCENARIOS = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"];
const ALL_METHODS    = ["BaselineA", "BaselineB", "ProposedSystem"];
// Keep SCENARIOS for aggregation helpers
const SCENARIOS = ALL_SCENARIOS;

export default function BatchPanel() {
  const [nSeeds, setNSeeds] = useState(30);
  const [nTicks, setNTicks] = useState(2880);
  const [selScenarios, setSelScenarios] = useState<string[]>([...ALL_SCENARIOS]);
  const [selMethods,   setSelMethods]   = useState<string[]>([...ALL_METHODS]);
  const [status, setStatus] = useState<BatchStatus | null>(null);
  const [results, setResults] = useState<BatchResult[]>([]);
  const [posting, setPosting] = useState(false);
  const [selectedKpi, setSelectedKpi] = useState<keyof KpiSnapshot>("survival_ratio");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const isRunning = status?.running === true;
  const disabled  = posting || isRunning;

  function toggle<T>(arr: T[], val: T): T[] {
    return arr.includes(val) ? arr.filter(x => x !== val) : [...arr, val];
  }

  function stopPoll() {
    if (pollRef.current) clearInterval(pollRef.current);
  }

  async function startBatch() {
    setPosting(true);
    setResults([]);
    setStatus(null);
    await fetch(`${API}/sim/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        n_seeds: nSeeds,
        n_ticks: nTicks,
        scenarios: selScenarios,
        methods: selMethods,
      }),
    });
    setPosting(false);
    stopPoll();
    pollRef.current = setInterval(async () => {
      const s = await fetch(`${API}/sim/batch/status`).then(r => r.json() as Promise<BatchStatus>);
      setStatus(s);
      if (!s.running) {
        stopPoll();
        const r = await fetch(`${API}/sim/batch/results`).then(r => r.json() as Promise<BatchResult[]>);
        setResults(r);
      }
    }, 2000);
  }

  useEffect(() => () => stopPoll(), []);

  const chartData = aggregateByMethod(results, selectedKpi);
  const kpiDef = KPI_OPTIONS.find(k => k.key === selectedKpi)!;
  const summaryTable = buildSummaryTable(results);
  const totalRuns = selScenarios.length * selMethods.length * nSeeds;

  return (
    <div style={{ color: "#f1f5f9", display: "flex", flexDirection: "column", gap: 20, maxWidth: 900 }}>
      {/* Title */}
      <div>
        <h2 style={{ margin: "0 0 6px", fontSize: 20, fontWeight: 700, letterSpacing: "-0.01em" }}>
          Batch Experiment
        </h2>
        <p style={{ margin: 0, fontSize: 12, color: "#64748b" }}>
          {totalRuns} runs ({selScenarios.length} scenarios × {selMethods.length} methods × {nSeeds} seeds).
        </p>
      </div>

      {/* Scenario selector */}
      <ChipGroup
        label="Scenarios"
        options={ALL_SCENARIOS}
        selected={selScenarios}
        onToggle={s => setSelScenarios(prev => toggle(prev, s))}
        disabled={disabled}
      />

      {/* Method selector */}
      <ChipGroup
        label="Methods"
        options={ALL_METHODS}
        selected={selMethods}
        onToggle={m => setSelMethods(prev => toggle(prev, m))}
        disabled={disabled}
        colors={METHOD_COLORS}
      />

      {/* Config row */}
      <div style={{ display: "flex", gap: 12, alignItems: "flex-end", flexWrap: "wrap" }}>
        <LabeledNum label="Seeds per cell" value={nSeeds} onChange={setNSeeds} min={1} max={100} />
        <LabeledNum label="Ticks per run" value={nTicks} onChange={setNTicks} min={100} max={10000} step={100} />
        <button
          onClick={startBatch}
          disabled={disabled}
          style={{
            padding: "8px 20px",
            background: disabled ? "#1e293b" : "#1d4ed8",
            color: disabled ? "#475569" : "#fff",
            border: `1px solid ${disabled ? "#334155" : "#3b82f6"}`,
            borderRadius: 8,
            cursor: disabled ? "default" : "pointer",
            fontSize: 13,
            fontWeight: 600,
            transition: "all 0.15s",
          }}
        >
          {posting ? "Starting…" : isRunning ? "Running…" : "▶ Run Batch"}
        </button>
      </div>

      {/* Progress */}
      {status && (
        <div style={{ background: "#0c1220", borderRadius: 10, padding: "14px 16px", border: "1px solid #1e293b" }}>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 8 }}>
            <span style={{ fontSize: 13, color: "#94a3b8" }}>
              {status.completed} / {status.total} runs
              {status.running ? " — running…" : " — complete"}
              {(status.failed ?? 0) > 0 && (
                <span style={{ color: "#f87171", marginLeft: 8 }}>
                  {status.failed} failed
                </span>
              )}
            </span>
            <span style={{ fontSize: 13, fontWeight: 700, color: status.running ? "#60a5fa" : "#22c55e" }}>
              {status.progress_pct.toFixed(1)}%
            </span>
          </div>
          <div style={{ background: "#1e293b", borderRadius: 6, height: 10, overflow: "hidden" }}>
            <div style={{
              height: "100%",
              background: status.running
                ? "linear-gradient(90deg, #3b82f6, #60a5fa)"
                : "#22c55e",
              width: `${status.progress_pct}%`,
              transition: "width 0.4s ease",
              borderRadius: 6,
            }} />
          </div>
        </div>
      )}

      {/* Results charts */}
      {chartData.length > 0 && (
        <>
          {/* KPI selector */}
          <div>
            <div style={{ fontSize: 10, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 8, fontWeight: 700 }}>
              KPI to compare
            </div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              {KPI_OPTIONS.map(({ key, label }) => (
                <button key={key as string} onClick={() => setSelectedKpi(key)} style={{
                  padding: "4px 12px",
                  background: selectedKpi === key ? "#1d4ed8" : "#1e293b",
                  color: selectedKpi === key ? "#fff" : "#94a3b8",
                  border: `1px solid ${selectedKpi === key ? "#3b82f6" : "#334155"}`,
                  borderRadius: 20,
                  cursor: "pointer",
                  fontSize: 11,
                  fontWeight: selectedKpi === key ? 600 : 400,
                  transition: "all 0.15s",
                }}>
                  {label}
                </button>
              ))}
            </div>
          </div>

          {/* Bar chart */}
          <div style={{ background: "#0c1220", borderRadius: 10, padding: "16px", border: "1px solid #1e293b" }}>
            <div style={{ fontSize: 12, color: "#94a3b8", marginBottom: 12 }}>
              Mean <strong style={{ color: "#f1f5f9" }}>{kpiDef.label}</strong> by method
              {" "}
              <span style={{ color: "#475569", fontSize: 11 }}>
                ({kpiDef.higherBetter ? "higher is better" : "lower is better"})
              </span>
            </div>
            <ResponsiveContainer width="100%" height={240}>
              <BarChart data={chartData} barGap={4} barCategoryGap="25%">
                <XAxis
                  dataKey="scenario"
                  tick={{ fontSize: 11, fill: "#64748b" }}
                  axisLine={{ stroke: "#1e293b" }}
                  tickLine={false}
                />
                <YAxis
                  domain={["auto", "auto"]}
                  width={44}
                  tick={{ fontSize: 10, fill: "#64748b" }}
                  axisLine={false}
                  tickLine={false}
                />
                <Tooltip
                  formatter={(v: unknown) => [typeof v === "number" ? v.toFixed(4) : String(v), ""]}
                  contentStyle={{ background: "#0f172a", border: "1px solid #334155", fontSize: 11, borderRadius: 8 }}
                  cursor={{ fill: "#1e293b" }}
                />
                <Legend
                  wrapperStyle={{ fontSize: 11, paddingTop: 8 }}
                  formatter={(v) => <span style={{ color: METHOD_COLORS[v] ?? "#94a3b8" }}>{v}</span>}
                />
                {["BaselineA", "BaselineB", "ProposedSystem"].map(method => (
                  <Bar
                    key={method}
                    dataKey={method}
                    fill={METHOD_COLORS[method]}
                    radius={[3, 3, 0, 0]}
                    isAnimationActive={false}
                  />
                ))}
              </BarChart>
            </ResponsiveContainer>
          </div>

          {/* Summary table */}
          {summaryTable.length > 0 && (
            <div style={{ background: "#0c1220", borderRadius: 10, padding: "16px", border: "1px solid #1e293b" }}>
              <div style={{ fontSize: 11, color: "#64748b", marginBottom: 12, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.07em" }}>
                Mean values — all KPIs × scenarios
              </div>
              <div style={{ overflowX: "auto" }}>
                <table style={{ borderCollapse: "collapse", width: "100%", fontSize: 11 }}>
                  <thead>
                    <tr style={{ borderBottom: "1px solid #1e293b" }}>
                      <th style={th}>Scenario</th>
                      <th style={th}>Method</th>
                      {KPI_OPTIONS.map(k => (
                        <th key={k.key as string} style={th}>{k.label}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {summaryTable.map((row, i) => (
                      <tr key={i} style={{ borderBottom: "1px solid #0f172a", background: i % 2 === 0 ? "#0a0f1a" : "transparent" }}>
                        <td style={stTd}>{row.scenario}</td>
                        <td style={{ ...stTd, color: METHOD_COLORS[row.method] ?? "#94a3b8", fontWeight: 600 }}>
                          {row.method}
                        </td>
                        {KPI_OPTIONS.map(k => (
                          <td key={k.key as string} style={{ ...stTd, fontVariantNumeric: "tabular-nums", color: "#94a3b8" }}>
                            {row.kpis[k.key]?.toFixed(4) ?? "—"}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div style={{ fontSize: 11, color: "#475569", marginTop: 8 }}>
                {results.length} total runs collected.
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}

function aggregateByMethod(results: BatchResult[], kpi: keyof KpiSnapshot) {
  const methods = ["BaselineA", "BaselineB", "ProposedSystem"];
  return SCENARIOS.map(scenario => {
    const row: Record<string, unknown> = { scenario };
    for (const method of methods) {
      const vals = results
        .filter(r => r.scenario === scenario && r.method === method)
        .map(r => r.kpi[kpi] as number);
      row[method] = vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : 0;
    }
    return row;
  });
}

function buildSummaryTable(results: BatchResult[]) {
  const methods = ["BaselineA", "BaselineB", "ProposedSystem"];
  const rows: { scenario: string; method: string; kpis: Partial<Record<keyof KpiSnapshot, number>> }[] = [];
  for (const scenario of SCENARIOS) {
    for (const method of methods) {
      const matching = results.filter(r => r.scenario === scenario && r.method === method);
      if (matching.length === 0) continue;
      const kpis: Partial<Record<keyof KpiSnapshot, number>> = {};
      for (const { key } of KPI_OPTIONS) {
        const vals = matching.map(r => r.kpi[key] as number).filter(v => isFinite(v));
        kpis[key] = vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : undefined;
      }
      rows.push({ scenario, method, kpis });
    }
  }
  return rows;
}

function ChipGroup({
  label, options, selected, onToggle, disabled, colors,
}: {
  label: string;
  options: string[];
  selected: string[];
  onToggle: (v: string) => void;
  disabled: boolean;
  colors?: Record<string, string>;
}) {
  return (
    <div>
      <div style={{ fontSize: 10, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 6, fontWeight: 700 }}>
        {label}
      </div>
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
        {options.map(opt => {
          const active = selected.includes(opt);
          const col = colors?.[opt] ?? "#3b82f6";
          return (
            <button
              key={opt}
              onClick={() => !disabled && onToggle(opt)}
              style={{
                padding: "4px 12px",
                background: active ? `${col}22` : "#1e293b",
                color: active ? col : "#64748b",
                border: `1px solid ${active ? col : "#334155"}`,
                borderRadius: 20,
                cursor: disabled ? "default" : "pointer",
                fontSize: 11,
                fontWeight: active ? 600 : 400,
                transition: "all 0.15s",
                opacity: disabled ? 0.5 : 1,
              }}
            >
              {opt}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function LabeledNum({
  label, value, onChange, min, max, step,
}: {
  label: string; value: number; onChange: (v: number) => void;
  min?: number; max?: number; step?: number;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span style={{ fontSize: 10, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>
        {label}
      </span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step ?? 1}
        onChange={e => onChange(Number(e.target.value))}
        style={{
          padding: "6px 10px",
          background: "#1e293b",
          color: "#f1f5f9",
          border: "1px solid #334155",
          borderRadius: 6,
          fontSize: 13,
          width: 110,
          outline: "none",
        }}
      />
    </div>
  );
}

const th: React.CSSProperties = {
  padding: "6px 8px",
  textAlign: "left",
  color: "#64748b",
  fontWeight: 600,
  fontSize: 10,
  textTransform: "uppercase",
  letterSpacing: "0.05em",
  whiteSpace: "nowrap",
};
const stTd: React.CSSProperties = { padding: "6px 8px", fontSize: 11 };
