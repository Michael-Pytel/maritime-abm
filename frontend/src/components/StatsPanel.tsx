import { useState, useEffect, useMemo } from "react";
import {
  BarChart, Bar, XAxis, YAxis, Tooltip,
  ResponsiveContainer, Cell, RadarChart, Radar,
  PolarGrid, PolarAngleAxis,
} from "recharts";
import type { HypothesisResult, BatchResult } from "../types";

const API = "http://localhost:3000";

const KPI_META: Record<string, { label: string; unit: string; higherBetter: boolean; color: string }> = {
  survival_ratio:       { label: "Survival Ratio",      unit: "",        higherBetter: true,  color: "#34d399" },
  fatal_per_1k_hrs:     { label: "Fatalities /1k hrs",  unit: "/1k hrs", higherBetter: false, color: "#f87171" },
  mean_p_prep:          { label: "Mean Preparedness",   unit: "",        higherBetter: true,  color: "#60a5fa" },
  collision_per_1k_hrs: { label: "Collisions /1k hrs",  unit: "/1k hrs", higherBetter: false, color: "#fb923c" },
  avg_tta_hours:        { label: "Avg Time-to-Rescue",  unit: "hrs",     higherBetter: false, color: "#fbbf24" },
  evac_activation_rate: { label: "Evac Activation Rate", unit: "",       higherBetter: false, color: "#c084fc" },
};

const SCENARIOS = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"];
const METHODS   = ["ProposedSystem", "BaselineA", "BaselineB"];

const METHOD_COLORS: Record<string, string> = {
  ProposedSystem: "#60a5fa",
  BaselineA:      "#94a3b8",
  BaselineB:      "#fbbf24",
};

type SortKey = "effect" | "pvalue" | "kpi";
type View = "table" | "radar" | "results";

// ── Statistical helpers ──────────────────────────────────────────────────────

interface BoxStats {
  min: number; q1: number; median: number; q3: number; max: number; mean: number; std: number; n: number;
}

function computeBoxStats(data: number[]): BoxStats {
  if (data.length === 0) return { min: 0, q1: 0, median: 0, q3: 0, max: 0, mean: 0, std: 0, n: 0 };
  const s = [...data].sort((a, b) => a - b);
  const n = s.length;
  const q = (p: number) => {
    const idx = p * (n - 1);
    const lo = Math.floor(idx), hi = Math.ceil(idx);
    return s[lo] + (idx - lo) * (s[hi] - s[lo]);
  };
  const mean = data.reduce((a, b) => a + b, 0) / n;
  const variance = data.reduce((a, v) => a + (v - mean) ** 2, 0) / Math.max(n - 1, 1);
  const q1 = q(0.25), q3 = q(0.75);
  const iqr = q3 - q1;
  const whiskerMin = Math.max(s[0], q1 - 1.5 * iqr);
  const whiskerMax = Math.min(s[n - 1], q3 + 1.5 * iqr);
  return { min: whiskerMin, q1, median: q(0.5), q3, max: whiskerMax, mean, std: Math.sqrt(variance), n };
}

// ── SVG Box-whisker component ─────────────────────────────────────────────────

function BoxPlotChart({ series, height = 140 }: {
  series: { label: string; color: string; stats: BoxStats }[];
  height?: number;
}) {
  const allVals = series.flatMap(s => [s.stats.min, s.stats.max]);
  const gMin = Math.min(...allVals);
  const gMax = Math.max(...allVals);
  const range = gMax - gMin || 1;
  const padY = 12, padX = 8;
  const bw = 32, gap = 16;
  const svgW = series.length * (bw + gap) + padX * 2;
  const scaleY = (v: number) => padY + (1 - (v - gMin) / range) * (height - 2 * padY);

  return (
    <svg width={svgW} height={height} style={{ overflow: "visible" }}>
      {series.map((s, i) => {
        const cx = padX + i * (bw + gap) + bw / 2;
        const x = cx - bw / 2;
        const { min, q1, median, q3, max } = s.stats;
        if (s.stats.n === 0) return null;
        return (
          <g key={s.label}>
            {/* Whisker lines */}
            <line x1={cx} y1={scaleY(min)} x2={cx} y2={scaleY(max)} stroke={s.color} strokeWidth={1.5} opacity={0.5} />
            <line x1={cx - 6} y1={scaleY(min)} x2={cx + 6} y2={scaleY(min)} stroke={s.color} strokeWidth={1.5} opacity={0.5} />
            <line x1={cx - 6} y1={scaleY(max)} x2={cx + 6} y2={scaleY(max)} stroke={s.color} strokeWidth={1.5} opacity={0.5} />
            {/* IQR box */}
            <rect
              x={x} y={scaleY(q3)}
              width={bw} height={Math.abs(scaleY(q1) - scaleY(q3))}
              fill={s.color} fillOpacity={0.15}
              stroke={s.color} strokeWidth={1.5}
            />
            {/* Median */}
            <line x1={x} y1={scaleY(median)} x2={x + bw} y2={scaleY(median)} stroke={s.color} strokeWidth={2.5} />
            {/* Label */}
            <text x={cx} y={height - 2} textAnchor="middle" fontSize={8} fill="#64748b">
              {s.label === "ProposedSystem" ? "Proposed" : s.label.replace("Baseline", "B")}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

export default function StatsPanel() {
  const [tests, setTests]     = useState<HypothesisResult[]>([]);
  const [results, setResults] = useState<BatchResult[]>([]);
  const [filterSig, setFilterSig] = useState(false);
  const [scenario, setScenario]   = useState("CalmPassage");
  const [sortBy, setSortBy]       = useState<SortKey>("effect");
  const [loading, setLoading]     = useState(true);
  const [view, setView]           = useState<View>("table");
  const [outlierKpi, setOutlierKpi] = useState("survival_ratio");

  async function load() {
    try {
      const [hyp, res] = await Promise.all([
        fetch(`${API}/sim/batch/hypothesis`).then(r => r.json() as Promise<HypothesisResult[]>),
        fetch(`${API}/sim/batch/results`).then(r => r.json() as Promise<BatchResult[]>),
      ]);
      setTests(Array.isArray(hyp) ? hyp : []);
      setResults(Array.isArray(res) ? res : []);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { load(); }, []);

  const scenarioTests = tests.filter(t => t.scenario === scenario);
  const filtered = filterSig ? scenarioTests.filter(t => t.significant) : scenarioTests;
  const sorted = useMemo(() => {
    return [...filtered].sort((a, b) => {
      if (sortBy === "effect") return b.cliffs_delta - a.cliffs_delta;
      if (sortBy === "pvalue") return a.p_corrected - b.p_corrected;
      return a.kpi.localeCompare(b.kpi);
    });
  }, [filtered, sortBy]);

  const sigCount     = scenarioTests.filter(t => t.significant).length;
  const confirmedCnt = scenarioTests.filter(t => t.confirmed).length;

  // Radar data: ProposedSystem advantage over BaselineA (from method_a=BaselineA, method_b=ProposedSystem)
  const radarData = Object.keys(KPI_META).map(kpi => {
    const test = scenarioTests.find(
      t => t.kpi === kpi && t.method_a === "BaselineA" && t.method_b === "ProposedSystem"
    );
    const meta = KPI_META[kpi];
    if (!test || !meta) return { kpi: meta?.label ?? kpi, advantage: 0 };
    const rawDiff = test.mean_b - test.mean_a; // Proposed minus BaselineA
    const advantage = meta.higherBetter ? rawDiff : -rawDiff;
    return { kpi: meta.label, advantage: Math.round(advantage * 1000) / 1000 };
  });

  // Results tab: per-method KPI statistics for the selected scenario
  const scenarioResults = results.filter(r => r.scenario === scenario);
  const methodStats = useMemo(() => {
    return METHODS.map(m => {
      const rows = scenarioResults.filter(r => r.method === m);
      const n = rows.length;
      return {
        method: m,
        n,
        kpis: Object.fromEntries(Object.keys(KPI_META).map(kpi => {
          const vals = rows.map(r => {
            const v = r.kpi as Record<string, number>;
            return v[kpi] ?? 0;
          });
          const stats = computeBoxStats(vals);
          const ci95 = n > 1 ? 1.96 * stats.std / Math.sqrt(n) : 0;
          return [kpi, { ...stats, ci95 }];
        })),
      };
    });
  }, [scenarioResults]);

  // Seed outlier table
  const outlierData = useMemo(() => {
    return METHODS.map(m => {
      const rows = scenarioResults
        .filter(r => r.method === m)
        .map(r => ({ seed: r.seed, value: (r.kpi as Record<string, number>)[outlierKpi] ?? 0 }))
        .sort((a, b) => a.value - b.value);
      return { method: m, top3: rows.slice(-3).reverse(), bottom3: rows.slice(0, 3) };
    });
  }, [scenarioResults, outlierKpi]);

  const noData = tests.length === 0 && results.length === 0 && !loading;

  return (
    <div style={{ color: "#f1f5f9", display: "flex", flexDirection: "column", gap: 20, maxWidth: 1100 }}>
      {/* Title */}
      <div>
        <h2 style={{ margin: "0 0 6px", fontSize: 20, fontWeight: 700, letterSpacing: "-0.01em" }}>
          Statistical Analysis
        </h2>
        <p style={{ margin: 0, fontSize: 12, color: "#64748b" }}>
          Pairwise Mann-Whitney U tests with Holm-Bonferroni correction (α=0.05). Cliff's Δ: small≥0.147, medium≥0.33, large≥0.474.
        </p>
      </div>

      {/* Scenario selector + refresh */}
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
        <span style={{ fontSize: 11, color: "#64748b", marginRight: 4 }}>Scenario:</span>
        {SCENARIOS.map(s => (
          <button key={s} onClick={() => setScenario(s)} style={{
            padding: "5px 14px",
            background: scenario === s ? "#1d4ed8" : "#1e293b",
            color: scenario === s ? "#fff" : "#94a3b8",
            border: `1px solid ${scenario === s ? "#3b82f6" : "#334155"}`,
            borderRadius: 20, cursor: "pointer", fontSize: 12,
            fontWeight: scenario === s ? 600 : 400, transition: "all 0.15s",
          }}>
            {s}
          </button>
        ))}
        <div style={{ flex: 1 }} />
        <button onClick={load} disabled={loading} style={{
          padding: "5px 14px", background: "#1e293b",
          color: loading ? "#475569" : "#f1f5f9",
          border: "1px solid #334155", borderRadius: 6,
          cursor: loading ? "default" : "pointer", fontSize: 12,
        }}>
          {loading ? "Loading…" : "↻ Refresh"}
        </button>
      </div>

      {/* Summary cards */}
      {scenarioTests.length > 0 && (
        <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
          <SummaryCard label="Tests run"          value={scenarioTests.length} color="#94a3b8" />
          <SummaryCard label="Significant (Holm)" value={sigCount}             color="#22c55e" />
          <SummaryCard label="Confirmed (p+Δ≥0.2)" value={confirmedCnt}       color="#f59e0b" />
          <SummaryCard label="Seeds per method"   value={methodStats[0]?.n ?? 0} color="#60a5fa" />
        </div>
      )}

      {/* View selector */}
      {(scenarioTests.length > 0 || scenarioResults.length > 0) && (
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <div style={{ display: "flex", gap: 2, background: "#1e293b", borderRadius: 6, padding: 2, border: "1px solid #334155" }}>
            {(["table", "radar", "results"] as View[]).map(v => (
              <button key={v} onClick={() => setView(v)} style={{
                padding: "4px 12px",
                background: view === v ? "#334155" : "transparent",
                color: view === v ? "#f1f5f9" : "#64748b",
                border: "none", borderRadius: 4, cursor: "pointer",
                fontSize: 11, fontWeight: view === v ? 600 : 400,
              }}>
                {v === "table" ? "Table" : v === "radar" ? "Radar" : "Results"}
              </button>
            ))}
          </div>

          {view === "table" && (
            <>
              <span style={{ fontSize: 11, color: "#64748b" }}>Sort:</span>
              {(["effect", "pvalue", "kpi"] as SortKey[]).map(s => (
                <button key={s} onClick={() => setSortBy(s)} style={{
                  padding: "3px 10px",
                  background: sortBy === s ? "#334155" : "transparent",
                  color: sortBy === s ? "#f1f5f9" : "#64748b",
                  border: "1px solid #334155", borderRadius: 4,
                  cursor: "pointer", fontSize: 11,
                }}>
                  {s === "effect" ? "Cliff's Δ" : s === "pvalue" ? "p-value" : "KPI name"}
                </button>
              ))}
              <label style={{ fontSize: 12, color: "#94a3b8", display: "flex", alignItems: "center", gap: 5, marginLeft: 8 }}>
                <input type="checkbox" checked={filterSig} onChange={e => setFilterSig(e.target.checked)} />
                Significant only
              </label>
            </>
          )}
        </div>
      )}

      {noData && (
        <div style={{ textAlign: "center", padding: "60px 0", color: "#334155", fontSize: 14 }}>
          <div style={{ fontSize: 32, marginBottom: 12 }}>📊</div>
          No results yet — run a batch experiment first.
        </div>
      )}

      {/* ── Table view ── */}
      {view === "table" && sorted.length > 0 && (
        <>
          <div style={{ overflowX: "auto" }}>
            <table style={{ borderCollapse: "separate", borderSpacing: "0 3px", width: "100%", fontSize: 12 }}>
              <thead>
                <tr>
                  {["KPI", "Comparison", "Mean A", "Mean B", "Cliff's Δ", "p (raw)", "p (Holm)", ""].map(h => (
                    <th key={h} style={{
                      padding: "6px 10px", textAlign: "left",
                      color: "#475569", fontSize: 10, textTransform: "uppercase",
                      letterSpacing: "0.07em", fontWeight: 600,
                      borderBottom: "1px solid #1e293b", whiteSpace: "nowrap",
                    }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {sorted.map((t, i) => {
                  const meta = KPI_META[t.kpi];
                  const deltaAbs = t.cliffs_delta.abs ? Math.abs(t.cliffs_delta) : Math.abs(t.cliffs_delta);
                  const deltaColor =
                    deltaAbs >= 0.474 ? "#f97316" :
                    deltaAbs >= 0.33  ? "#f59e0b" :
                    deltaAbs >= 0.147 ? "#3b82f6" : "#334155";
                  const confirmed = t.confirmed;

                  return (
                    <tr key={i} style={{ background: confirmed ? "#0a1f14" : t.significant ? "#0d1a2d" : "#0c1220" }}>
                      <td style={{ ...td, borderLeft: `3px solid ${meta?.color ?? "#334155"}` }}>
                        <div style={{ fontWeight: 600, color: "#e2e8f0" }}>{meta?.label ?? t.kpi}</div>
                      </td>
                      <td style={td}>
                        <span style={{ color: METHOD_COLORS[t.method_a] ?? "#94a3b8", fontWeight: 500 }}>{t.method_a}</span>
                        <span style={{ color: "#334155", margin: "0 5px" }}>vs</span>
                        <span style={{ color: METHOD_COLORS[t.method_b] ?? "#94a3b8", fontWeight: 500 }}>{t.method_b}</span>
                      </td>
                      <td style={{ ...td, fontVariantNumeric: "tabular-nums", color: "#94a3b8" }}>{t.mean_a.toFixed(4)}</td>
                      <td style={{ ...td, fontVariantNumeric: "tabular-nums", color: "#94a3b8" }}>{t.mean_b.toFixed(4)}</td>
                      <td style={{ ...td, minWidth: 130 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                          <div style={{ flex: 1, height: 5, background: "#1e293b", borderRadius: 3, overflow: "hidden" }}>
                            <div style={{
                              height: "100%",
                              width: `${Math.min(100, (deltaAbs / 0.474) * 100)}%`,
                              background: deltaColor, borderRadius: 3,
                            }} />
                          </div>
                          <span style={{ fontSize: 10, color: deltaColor, width: 40, textAlign: "right", fontVariantNumeric: "tabular-nums", fontWeight: 600 }}>
                            {t.cliffs_delta >= 0 ? "+" : ""}{t.cliffs_delta.toFixed(3)}
                          </span>
                        </div>
                      </td>
                      <td style={{ ...td, color: "#64748b", fontVariantNumeric: "tabular-nums" }}>{t.p_value.toFixed(4)}</td>
                      <td style={{ ...td, color: t.significant ? "#22c55e" : "#64748b", fontVariantNumeric: "tabular-nums", fontWeight: t.significant ? 600 : 400 }}>
                        {t.p_corrected.toFixed(4)}
                      </td>
                      <td style={{ ...td, textAlign: "center" }}>
                        {confirmed ? (
                          <span style={{ display: "inline-block", width: 18, height: 18, background: "#052e16", border: "1px solid #22c55e", borderRadius: "50%", color: "#22c55e", fontSize: 11, lineHeight: "18px", textAlign: "center" }}>✓</span>
                        ) : t.significant ? (
                          <span style={{ display: "inline-block", width: 18, height: 18, background: "#0d1a2d", border: "1px solid #3b82f6", borderRadius: "50%", color: "#60a5fa", fontSize: 11, lineHeight: "18px", textAlign: "center" }}>~</span>
                        ) : (
                          <span style={{ color: "#1e293b", fontSize: 11 }}>—</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          {/* Per-KPI mean chart */}
          <div style={{ marginTop: 8 }}>
            <div style={{ fontSize: 11, color: "#64748b", marginBottom: 10, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.07em" }}>
              Mean values per KPI × method
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
              {Object.entries(KPI_META).map(([kpiKey, meta]) => {
                const vals: { method: string; mean: number }[] = [];
                for (const m of METHODS) {
                  const t = scenarioTests.find(t => t.kpi === kpiKey && (t.method_a === m || t.method_b === m));
                  if (!t) continue;
                  const mean = t.method_a === m ? t.mean_a : t.mean_b;
                  vals.push({ method: m, mean: Math.round(mean * 10000) / 10000 });
                }
                if (vals.length === 0) return null;
                return (
                  <div key={kpiKey} style={{ background: "#0c1220", borderRadius: 8, padding: "10px 12px", border: `1px solid ${meta.color}18` }}>
                    <div style={{ fontSize: 10, color: meta.color, fontWeight: 600, marginBottom: 8, textTransform: "uppercase", letterSpacing: "0.06em" }}>
                      {meta.label}
                    </div>
                    <ResponsiveContainer width="100%" height={70}>
                      <BarChart data={vals} margin={{ top: 0, right: 0, left: 0, bottom: 0 }}>
                        <XAxis dataKey="method" tick={{ fontSize: 8, fill: "#64748b" }} tickFormatter={m => m === "ProposedSystem" ? "Proposed" : m.replace("Baseline", "B")} />
                        <YAxis hide domain={["auto", "auto"]} />
                        <Tooltip formatter={(v: unknown) => [typeof v === "number" ? v.toFixed(4) : String(v), meta.label]} contentStyle={{ background: "#0f172a", border: "1px solid #334155", fontSize: 10 }} />
                        <Bar dataKey="mean" radius={[3, 3, 0, 0]}>
                          {vals.map(d => <Cell key={d.method} fill={METHOD_COLORS[d.method] ?? "#475569"} />)}
                        </Bar>
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                );
              })}
            </div>
          </div>
        </>
      )}

      {/* ── Radar view ── */}
      {view === "radar" && scenarioTests.length > 0 && (
        <div>
          <div style={{ fontSize: 12, color: "#64748b", marginBottom: 8 }}>
            ProposedSystem advantage over BaselineA (positive = better)
          </div>
          <ResponsiveContainer width="100%" height={320}>
            <RadarChart data={radarData}>
              <PolarGrid stroke="#1e293b" />
              <PolarAngleAxis dataKey="kpi" tick={{ fontSize: 10, fill: "#94a3b8" }} />
              <Radar name="Advantage" dataKey="advantage" stroke="#3b82f6" fill="#3b82f6" fillOpacity={0.25} />
              <Tooltip formatter={(v: unknown) => [typeof v === "number" ? v.toFixed(4) : String(v), "Advantage"]} contentStyle={{ background: "#0f172a", border: "1px solid #334155", fontSize: 11 }} />
            </RadarChart>
          </ResponsiveContainer>
        </div>
      )}

      {/* ── Results view ── */}
      {view === "results" && (
        <>
          {scenarioResults.length === 0 ? (
            <div style={{ color: "#334155", fontSize: 13, padding: "40px 0", textAlign: "center" }}>
              No batch results for this scenario yet.
            </div>
          ) : (
            <>
              {/* KPI ranking table */}
              <div>
                <div style={{ fontSize: 11, color: "#64748b", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 10 }}>
                  KPI Ranking — {scenario}
                </div>
                <div style={{ overflowX: "auto" }}>
                  <table style={{ borderCollapse: "separate", borderSpacing: "0 3px", fontSize: 11, width: "100%" }}>
                    <thead>
                      <tr>
                        <th style={th}>KPI</th>
                        {methodStats.map(ms => (
                          <th key={ms.method} colSpan={2} style={{ ...th, color: METHOD_COLORS[ms.method] ?? "#94a3b8" }}>
                            {ms.method === "ProposedSystem" ? "Proposed" : ms.method} (n={ms.n})
                          </th>
                        ))}
                        <th style={th}>Δ vs BaselineA</th>
                      </tr>
                      <tr>
                        <th style={th} />
                        {methodStats.map(ms => (
                          <>
                            <th key={`${ms.method}-mean`} style={{ ...th, fontSize: 9 }}>mean ± std</th>
                            <th key={`${ms.method}-ci`}   style={{ ...th, fontSize: 9 }}>CI95</th>
                          </>
                        ))}
                        <th style={th} />
                      </tr>
                    </thead>
                    <tbody>
                      {Object.entries(KPI_META).map(([kpiKey, meta]) => {
                        const perMethod = methodStats.map(ms => ({
                          method: ms.method,
                          stats: (ms.kpis[kpiKey] as ReturnType<typeof computeBoxStats> & { ci95: number }),
                        }));
                        const proposedStats = perMethod.find(m => m.method === "ProposedSystem")?.stats;
                        const baselineAStats = perMethod.find(m => m.method === "BaselineA")?.stats;
                        const delta = proposedStats && baselineAStats ? proposedStats.mean - baselineAStats.mean : null;
                        const deltaPct = delta != null && baselineAStats && baselineAStats.mean !== 0
                          ? (delta / Math.abs(baselineAStats.mean)) * 100 : null;
                        const deltaGood = delta != null && (meta.higherBetter ? delta > 0 : delta < 0);

                        return (
                          <tr key={kpiKey} style={{ background: "#0c1220" }}>
                            <td style={{ ...td, borderLeft: `3px solid ${meta.color}` }}>
                              <span style={{ fontWeight: 600, color: "#e2e8f0" }}>{meta.label}</span>
                            </td>
                            {perMethod.map(({ method, stats }) => (
                              <>
                                <td key={`${method}-v`} style={{ ...td, fontVariantNumeric: "tabular-nums", color: "#94a3b8" }}>
                                  {stats.mean.toFixed(4)} ± {stats.std.toFixed(4)}
                                </td>
                                <td key={`${method}-ci`} style={{ ...td, fontVariantNumeric: "tabular-nums", color: "#64748b", fontSize: 10 }}>
                                  ±{stats.ci95.toFixed(4)}
                                </td>
                              </>
                            ))}
                            <td style={td}>
                              {delta != null && (
                                <span style={{ color: deltaGood ? "#22c55e" : "#f87171", fontWeight: 600, fontVariantNumeric: "tabular-nums" }}>
                                  {delta >= 0 ? "+" : ""}{delta.toFixed(4)}
                                  {deltaPct != null && (
                                    <span style={{ fontSize: 9, marginLeft: 4, opacity: 0.7 }}>
                                      ({deltaPct >= 0 ? "+" : ""}{deltaPct.toFixed(1)}%)
                                    </span>
                                  )}
                                </span>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>

              {/* Per-KPI boxplots */}
              <div>
                <div style={{ fontSize: 11, color: "#64748b", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 10 }}>
                  Distribution by Method — {scenario}
                </div>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
                  {Object.entries(KPI_META).map(([kpiKey, meta]) => (
                    <div key={kpiKey} style={{ background: "#0c1220", borderRadius: 8, padding: "10px 12px", border: `1px solid ${meta.color}18` }}>
                      <div style={{ fontSize: 10, color: meta.color, fontWeight: 600, marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.06em" }}>
                        {meta.label}
                      </div>
                      <BoxPlotChart
                        series={methodStats.map(ms => ({
                          label: ms.method,
                          color: METHOD_COLORS[ms.method] ?? "#94a3b8",
                          stats: ms.kpis[kpiKey] as BoxStats,
                        }))}
                      />
                    </div>
                  ))}
                </div>
              </div>

              {/* Seed outlier table */}
              <div>
                <div style={{ display: "flex", gap: 10, alignItems: "center", marginBottom: 10 }}>
                  <div style={{ fontSize: 11, color: "#64748b", fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.07em" }}>
                    Seed Outliers
                  </div>
                  <select
                    value={outlierKpi}
                    onChange={e => setOutlierKpi(e.target.value)}
                    style={{ background: "#1e293b", color: "#f1f5f9", border: "1px solid #334155", borderRadius: 5, padding: "3px 8px", fontSize: 10, cursor: "pointer", outline: "none" }}
                  >
                    {Object.entries(KPI_META).map(([k, m]) => <option key={k} value={k}>{m.label}</option>)}
                  </select>
                </div>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}>
                  {outlierData.map(({ method, top3, bottom3 }) => (
                    <div key={method} style={{ background: "#0c1220", borderRadius: 8, padding: "10px 12px", border: "1px solid #1e293b" }}>
                      <div style={{ fontSize: 10, color: METHOD_COLORS[method] ?? "#94a3b8", fontWeight: 600, marginBottom: 8 }}>
                        {method === "ProposedSystem" ? "Proposed" : method}
                      </div>
                      <div style={{ fontSize: 9, color: "#22c55e", marginBottom: 3, textTransform: "uppercase" }}>Best seeds</div>
                      {top3.map(({ seed, value }) => (
                        <div key={seed} style={{ display: "flex", justifyContent: "space-between", fontSize: 10, color: "#94a3b8", padding: "1px 0" }}>
                          <span>seed {seed}</span>
                          <span style={{ fontVariantNumeric: "tabular-nums" }}>{value.toFixed(4)}</span>
                        </div>
                      ))}
                      <div style={{ fontSize: 9, color: "#f87171", marginTop: 6, marginBottom: 3, textTransform: "uppercase" }}>Worst seeds</div>
                      {bottom3.map(({ seed, value }) => (
                        <div key={seed} style={{ display: "flex", justifyContent: "space-between", fontSize: 10, color: "#94a3b8", padding: "1px 0" }}>
                          <span>seed {seed}</span>
                          <span style={{ fontVariantNumeric: "tabular-nums" }}>{value.toFixed(4)}</span>
                        </div>
                      ))}
                    </div>
                  ))}
                </div>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}

function SummaryCard({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div style={{
      background: "#0c1220", borderRadius: 10, padding: "10px 16px",
      border: "1px solid #1e293b", display: "flex", flexDirection: "column", gap: 3, minWidth: 110,
    }}>
      <div style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em" }}>{label}</div>
      <div style={{ fontSize: 26, fontWeight: 700, color, lineHeight: 1 }}>{value}</div>
    </div>
  );
}

const td: React.CSSProperties = { padding: "9px 10px", verticalAlign: "middle" };
const th: React.CSSProperties = {
  padding: "6px 10px", textAlign: "left", color: "#475569",
  fontSize: 10, textTransform: "uppercase", letterSpacing: "0.07em",
  fontWeight: 600, borderBottom: "1px solid #1e293b", whiteSpace: "nowrap",
};
