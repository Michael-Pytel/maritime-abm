import type { RunManifest, TickMessage, WeatherChannel } from "../types";
import { SPEED_PRESETS, STRIDE_OPTIONS } from "../hooks/usePlayback";

interface Props {
  runs: RunManifest[];
  onRefreshRuns: () => void;
  selectedRunId: string | null;
  onSelectRun: (id: string) => void;
  tickCount: number;
  currentIdx: number;
  onSeek: (idx: number) => void;
  playing: boolean;
  onTogglePlay: () => void;
  speedMs: number;
  onSetSpeed: (ms: number) => void;
  stride: number;
  onSetStride: (s: number) => void;
  loading: boolean;
  currentTick: TickMessage | null;
  weatherChannel: WeatherChannel;
  onSetWeatherChannel: (ch: WeatherChannel) => void;
}

const METHOD_COLOR: Record<string, string> = {
  BaselineA: "#6b7280",
  BaselineB: "#f59e0b",
  ProposedSystem: "#3b82f6",
};

export default function PlaybackControls({
  runs, onRefreshRuns, selectedRunId, onSelectRun,
  tickCount, currentIdx, onSeek,
  playing, onTogglePlay,
  speedMs, onSetSpeed,
  stride, onSetStride,
  loading, currentTick,
  weatherChannel, onSetWeatherChannel,
}: Props) {
  const pct = tickCount > 1 ? (currentIdx / (tickCount - 1)) * 100 : 0;

  return (
    <div style={{
      background: "#060c18",
      borderTop: "1px solid #1e293b",
      padding: "10px 16px",
      display: "flex",
      flexDirection: "column",
      gap: 8,
      flexShrink: 0,
    }}>
      {/* Row 1: run selector + weather channel */}
      <div style={{ display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 3, flex: "1 1 300px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ fontSize: 10, color: "#475569", textTransform: "uppercase", letterSpacing: "0.06em" }}>
              Run
            </span>
            <button
              onClick={onRefreshRuns}
              title="Refresh run list"
              style={{
                background: "none", border: "none", cursor: "pointer",
                color: "#475569", fontSize: 12, padding: 0, lineHeight: 1,
              }}
            >↻</button>
            <span style={{ fontSize: 10, color: "#334155" }}>({runs.length})</span>
          </div>
          <select
            value={selectedRunId ?? ""}
            onChange={e => onSelectRun(e.target.value)}
            style={{
              background: "#1e293b", color: "#f1f5f9",
              border: "1px solid #334155", borderRadius: 6,
              padding: "5px 8px", fontSize: 11, cursor: "pointer", outline: "none",
            }}
          >
            <option value="">— select a completed run —</option>
            {runs.map(r => (
              <option key={r.run_id} value={r.run_id} disabled={r.has_log === false}>
                {r.has_log === false ? "⚠ " : "▶ "}
                {r.scenario} / {r.method} / seed {r.seed}
                {r.has_log === false ? " (no log)" : ` (${r.n_ticks} ticks)`}
              </option>
            ))}
          </select>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          <span style={{ fontSize: 10, color: "#475569", textTransform: "uppercase", letterSpacing: "0.06em" }}>
            Weather channel
          </span>
          <select
            value={weatherChannel}
            onChange={e => onSetWeatherChannel(e.target.value as WeatherChannel)}
            style={{
              background: "#1e293b", color: "#f1f5f9",
              border: "1px solid #334155", borderRadius: 6,
              padding: "5px 8px", fontSize: 11, cursor: "pointer", outline: "none",
            }}
          >
            <option value="hazard">Hazard</option>
            <option value="sea_state">Sea State</option>
            <option value="wind">Wind</option>
            <option value="visibility">Visibility</option>
          </select>
        </div>

        {/* Run metadata pill */}
        {selectedRunId && runs.find(r => r.run_id === selectedRunId) && (() => {
          const m = runs.find(r => r.run_id === selectedRunId)!;
          return (
            <div style={{
              padding: "4px 10px", borderRadius: 20, fontSize: 11,
              background: "#1e293b", border: "1px solid #334155", color: "#94a3b8",
              display: "flex", alignItems: "center", gap: 8,
            }}>
              <span style={{ color: METHOD_COLOR[m.method] ?? "#94a3b8", fontWeight: 700 }}>{m.method}</span>
              <span>{m.scenario}</span>
              <span style={{ color: "#475569" }}>seed {m.seed}</span>
            </div>
          );
        })()}
      </div>

      {/* Row 2: scrubber */}
      {tickCount > 0 && (
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontSize: 11, color: "#475569", fontVariantNumeric: "tabular-nums", minWidth: 52 }}>
            {currentTick?.step ?? 0}
          </span>
          <input
            type="range"
            min={0}
            max={tickCount - 1}
            value={currentIdx}
            onChange={e => onSeek(Number(e.target.value))}
            style={{ flex: 1, accentColor: "#3b82f6", cursor: "pointer" }}
          />
          <span style={{ fontSize: 11, color: "#475569", fontVariantNumeric: "tabular-nums", minWidth: 52, textAlign: "right" }}>
            {tickCount - 1}
          </span>
          <span style={{ fontSize: 10, color: "#475569" }}>
            {pct.toFixed(0)}%
          </span>
        </div>
      )}

      {/* No-log notice — only for runs that explicitly lack a log (has_log===false from API) */}
      {!loading && tickCount === 0 && selectedRunId && (() => {
        const run = runs.find(r => r.run_id === selectedRunId);
        if (!run || run.has_log === false) {
          return (
            <div style={{
              fontSize: 11, color: "#f59e0b", background: "#451a0340",
              border: "1px solid #78350f80", borderRadius: 6, padding: "4px 10px",
            }}>
              ⚠ Run <strong>{selectedRunId}</strong> has no tick log — it was recorded before playback logging was enabled.
              Select any run marked <strong>▶</strong> to play it back.
            </div>
          );
        }
        return null;
      })()}

      {/* Row 3: controls */}
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        {/* Play / Pause */}
        <button
          onClick={onTogglePlay}
          disabled={tickCount === 0 || loading}
          style={{
            padding: "6px 14px",
            background: playing ? "#450a0a" : "#052e16",
            color: playing ? "#f87171" : "#22c55e",
            border: `1px solid ${playing ? "#f8717140" : "#22c55e40"}`,
            borderRadius: 7, cursor: tickCount === 0 ? "default" : "pointer",
            fontSize: 13, fontWeight: 700,
          }}
        >
          {loading ? "Loading…" : playing ? "■ Pause" : "▶ Play"}
        </button>

        {/* Speed */}
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <span style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.06em" }}>Speed</span>
          <div style={{ display: "flex", gap: 4 }}>
            {SPEED_PRESETS.map(p => (
              <button key={p.ms} onClick={() => onSetSpeed(p.ms)} style={{
                padding: "3px 8px", fontSize: 10,
                background: speedMs === p.ms ? "#1d4ed8" : "#1e293b",
                color: speedMs === p.ms ? "#fff" : "#64748b",
                border: `1px solid ${speedMs === p.ms ? "#3b82f6" : "#334155"}`,
                borderRadius: 4, cursor: "pointer",
              }}>{p.label}</button>
            ))}
          </div>
        </div>

        {/* Stride */}
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <span style={{ fontSize: 9, color: "#475569", textTransform: "uppercase", letterSpacing: "0.06em" }}>Stride</span>
          <div style={{ display: "flex", gap: 4 }}>
            {STRIDE_OPTIONS.map(s => (
              <button key={s} onClick={() => onSetStride(s)} style={{
                padding: "3px 8px", fontSize: 10,
                background: stride === s ? "#1d4ed8" : "#1e293b",
                color: stride === s ? "#fff" : "#64748b",
                border: `1px solid ${stride === s ? "#3b82f6" : "#334155"}`,
                borderRadius: 4, cursor: "pointer",
              }}>×{s}</button>
            ))}
          </div>
        </div>

        {/* Tick counter */}
        {tickCount > 0 && (
          <span style={{ marginLeft: "auto", fontSize: 11, color: "#475569", fontVariantNumeric: "tabular-nums" }}>
            tick {currentTick?.step ?? 0} / {(ticks => ticks > 0 ? ticks - 1 : 0)(tickCount)}
          </span>
        )}
      </div>
    </div>
  );
}
