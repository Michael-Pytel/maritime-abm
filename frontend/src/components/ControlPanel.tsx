import { useState } from "react";
import type { BatchResult, KpiSnapshot } from "../types";

const API = "http://localhost:3000";

type SeedKpiKey = keyof KpiSnapshot;

interface Props {
  connected: boolean;
  step: number;
}

const KPI_OPTS: { key: SeedKpiKey; label: string; higherIsBetter: boolean }[] = [
  { key: "survival_ratio",   label: "Survival ratio", higherIsBetter: true  },
  { key: "fatal_per_1k_hrs", label: "Fatal/1k hrs",   higherIsBetter: false },
  { key: "avg_tta_hours",    label: "Avg TTA hours",  higherIsBetter: false },
];

export default function ControlPanel({ connected: _connected, step: _step }: Props) {
  const [scenario, setScenario] = useState("CalmPassage");
  const [method, setMethod] = useState("ProposedSystem");
  const [seed, setSeed] = useState(42);
  const [loopMode, setLoopMode] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);
  const [seedKpi, setSeedKpi] = useState<SeedKpiKey>("survival_ratio");
  const [seedLoading, setSeedLoading] = useState(false);

  // Advanced hyperparams
  const [nTicks, setNTicks] = useState(2880);
  const [nVessels, setNVessels] = useState(20);
  const [radioRange, setRadioRange] = useState(15);
  const [shoreRadius, setShoreRadius] = useState(50);
  const [noiseStd, setNoiseStd] = useState(0.18);
  const [decayK, setDecayK] = useState(5.0);
  const [maxHops, setMaxHops] = useState(2);
  const [greenFrac, setGreenFrac] = useState(0.3);

  async function start() {
    setBusy(true);
    await fetch(`${API}/sim/start`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        scenario,
        method,
        seed,
        loop_mode: loopMode,
        n_ticks: nTicks,
        n_vessels: nVessels,
        vessel_radio_range_nm: radioRange,
        shore_broadcast_radius_nm: shoreRadius,
        shore_noise_std: noiseStd,
        shore_trust_decay_k: decayK,
        max_hop_count: maxHops,
        green_crew_fraction: greenFrac,
      }),
    });
    setBusy(false);
  }

  async function stop() {
    await fetch(`${API}/sim/stop`, { method: "POST" });
  }

  async function pickSeed(best: boolean) {
    setSeedLoading(true);
    try {
      const results: BatchResult[] = await fetch(`${API}/sim/batch/results`).then(r => r.json());
      const filtered = results.filter(r => r.scenario === scenario && r.method === method);
      if (filtered.length === 0) return;
      const opt = KPI_OPTS.find(k => k.key === seedKpi)!;
      const sorted = [...filtered].sort((a, b) => {
        const av = a.kpi[seedKpi] as number;
        const bv = b.kpi[seedKpi] as number;
        return opt.higherIsBetter
          ? (best ? bv - av : av - bv)
          : (best ? av - bv : bv - av);
      });
      setSeed(sorted[0].seed);
    } finally {
      setSeedLoading(false);
    }
  }

  return (
    <div style={{ padding: "10px 16px", color: "#f1f5f9" }}>
      {/* Core params row */}
      <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "flex-end" }}>
        <Sel
          label="Scenario"
          value={scenario}
          onChange={setScenario}
          options={["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"]}
        />
        <Sel
          label="Method"
          value={method}
          onChange={setMethod}
          options={["ProposedSystem", "BaselineA", "BaselineB"]}
          optionColors={{ ProposedSystem: "#3b82f6", BaselineA: "#94a3b8", BaselineB: "#f59e0b" }}
        />
        <Num label="Seed" value={seed} onChange={setSeed} width={70} />

        {/* Loop toggle */}
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span style={{ fontSize: 10, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>
            Loop
          </span>
          <button
            onClick={() => setLoopMode(v => !v)}
            style={{
              padding: "6px 12px",
              background: loopMode ? "#052e16" : "#1e293b",
              color: loopMode ? "#22c55e" : "#64748b",
              border: `1px solid ${loopMode ? "#22c55e" : "#334155"}`,
              borderRadius: 6,
              cursor: "pointer",
              fontSize: 12,
              fontWeight: loopMode ? 600 : 400,
            }}
          >
            {loopMode ? "ON" : "OFF"}
          </button>
        </div>

        {/* Action buttons */}
        <div style={{ display: "flex", gap: 6, alignItems: "flex-end", marginLeft: 4 }}>
          <button
            onClick={start}
            disabled={busy}
            style={{
              padding: "7px 18px",
              background: busy ? "#1e293b" : "linear-gradient(135deg, #1d4ed8, #2563eb)",
              color: busy ? "#475569" : "#fff",
              border: `1px solid ${busy ? "#334155" : "#3b82f6"}`,
              borderRadius: 7,
              cursor: busy ? "default" : "pointer",
              fontSize: 13,
              fontWeight: 600,
              letterSpacing: "0.01em",
            }}
          >
            {busy ? "Starting…" : "▶ Start"}
          </button>
          <button
            onClick={stop}
            style={{
              padding: "7px 16px",
              background: "#450a0a",
              color: "#f87171",
              border: "1px solid #f8717140",
              borderRadius: 7,
              cursor: "pointer",
              fontSize: 13,
              fontWeight: 600,
            }}
          >
            ■ Stop
          </button>
        </div>

        {/* Advanced toggle */}
        <button
          onClick={() => setShowAdvanced(v => !v)}
          style={{
            marginLeft: "auto",
            padding: "6px 10px",
            background: "transparent",
            border: "none",
            color: "#60a5fa",
            cursor: "pointer",
            fontSize: 11,
            display: "flex",
            alignItems: "center",
            gap: 4,
            alignSelf: "flex-end",
          }}
        >
          {showAdvanced ? "▲" : "▼"} Advanced
        </button>
      </div>

      {/* Advanced params */}
      {showAdvanced && (
        <>
          <div style={{
            marginTop: 10,
            padding: "12px 14px",
            background: "#060c18",
            borderRadius: 8,
            border: "1px solid #1e293b",
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))",
            gap: 10,
          }}>
            <Num label="Ticks" value={nTicks} onChange={setNTicks} />
            <Num label="Vessels" value={nVessels} onChange={setNVessels} />
            <Num label="Radio range (nm)" value={radioRange} onChange={setRadioRange} />
            <Num label="Shore radius (nm)" value={shoreRadius} onChange={setShoreRadius} />
            <Num label="Shore noise σ" value={noiseStd} onChange={setNoiseStd} step={0.01} />
            <Num label="Trust decay k" value={decayK} onChange={setDecayK} step={0.5} />
            <Num label="Max hops" value={maxHops} onChange={setMaxHops} />
            <Num label="Green crew %" value={greenFrac} onChange={setGreenFrac} step={0.05} />
          </div>

          {/* Seed shortcuts from batch results */}
          <div style={{
            marginTop: 10,
            display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap",
          }}>
            <span style={{ fontSize: 10, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Seed from batch
            </span>
            <select
              value={seedKpi}
              onChange={e => setSeedKpi(e.target.value as SeedKpiKey)}
              style={{
                background: "#1e293b", color: "#f1f5f9",
                border: "1px solid #334155", borderRadius: 5,
                padding: "3px 6px", fontSize: 10, cursor: "pointer", outline: "none",
              }}
            >
              {KPI_OPTS.map(k => <option key={k.key} value={k.key}>{k.label}</option>)}
            </select>
            <button
              onClick={() => pickSeed(true)}
              disabled={seedLoading}
              style={{
                padding: "3px 10px", fontSize: 10,
                background: "#052e16", color: "#22c55e",
                border: "1px solid #22c55e40", borderRadius: 5, cursor: "pointer",
              }}
            >
              {seedLoading ? "…" : "Best"}
            </button>
            <button
              onClick={() => pickSeed(false)}
              disabled={seedLoading}
              style={{
                padding: "3px 10px", fontSize: 10,
                background: "#450a0a", color: "#f87171",
                border: "1px solid #f8717140", borderRadius: 5, cursor: "pointer",
              }}
            >
              {seedLoading ? "…" : "Worst"}
            </button>
            <span style={{ fontSize: 9, color: "#334155" }}>
              (filters by current scenario + method)
            </span>
          </div>
        </>
      )}
    </div>
  );
}

function Sel({
  label, value, onChange, options, optionColors,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: string[];
  optionColors?: Record<string, string>;
}) {
  const color = optionColors?.[value];
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span style={{ fontSize: 10, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>
        {label}
      </span>
      <select
        value={value}
        onChange={e => onChange(e.target.value)}
        style={{
          padding: "5px 8px",
          background: "#1e293b",
          color: color ?? "#f1f5f9",
          border: "1px solid #334155",
          borderRadius: 6,
          fontSize: 12,
          cursor: "pointer",
          outline: "none",
        }}
      >
        {options.map(o => <option key={o} value={o}>{o}</option>)}
      </select>
    </div>
  );
}

function Num({
  label, value, onChange, width, step,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  width?: number;
  step?: number;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span style={{ fontSize: 10, color: "#64748b", textTransform: "uppercase", letterSpacing: "0.06em" }}>
        {label}
      </span>
      <input
        type="number"
        value={value}
        step={step ?? 1}
        onChange={e => onChange(Number(e.target.value))}
        style={{
          padding: "5px 8px",
          background: "#1e293b",
          color: "#f1f5f9",
          border: "1px solid #334155",
          borderRadius: 6,
          fontSize: 12,
          width: width ?? "100%",
          outline: "none",
        }}
      />
    </div>
  );
}
