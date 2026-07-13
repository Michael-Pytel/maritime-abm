import { useRef, useCallback } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import { SPEED_PRESETS } from "../hooks/usePlayback";
import { color, radius, METHOD_COLORS, SCEN_SHORT, methodShort } from "../theme/tokens";

type PB = ReturnType<typeof usePlayback>;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Pinned bottom dock of the left panel: now-playing card + live KPI tiles +
 *  a modern scrubber. Always visible regardless of the active tab. */
export default function PlaybackDock({ pb }: { pb: PB }) {
  const m = pb.manifest;
  const k = pb.currentTick?.kpis;
  const n = pb.ticks.length;
  const hasRun = n > 0;
  const step = pb.currentTick?.step ?? 0;

  const tiles = k ? [
    { label: "Coll /1k", value: k.collision_per_1k_hrs.toFixed(2), c: color.bad },
    { label: "Fatal /1k", value: k.fatal_per_1k_hrs.toFixed(2), c: color.rose },
    { label: "Survival", value: k.survival_ratio.toFixed(3), c: color.good },
    { label: "TTA h", value: k.avg_tta_hours.toFixed(2), c: color.blue },
    { label: "Evac", value: k.evac_activation_rate.toFixed(3), c: color.warn },
  ] : [];

  const methodCol = m ? (METHOD_COLORS[m.method] ?? color.accent) : color.accent;

  return (
    <div style={{
      flexShrink: 0, borderTop: `1px solid ${color.border}`,
      background: color.panel2, padding: "12px 14px 13px",
      display: "flex", flexDirection: "column", gap: 11,
    }}>
      {/* Now-playing card */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, minHeight: 18 }}>
        {m ? (
          <>
            <span style={{ width: 8, height: 8, borderRadius: "50%", background: methodCol, flexShrink: 0, boxShadow: `0 0 8px ${methodCol}88` }} />
            <span style={{ fontSize: 13, fontWeight: 700, color: color.text }}>{SCEN_SHORT[m.scenario] ?? m.scenario}</span>
            <span style={{ fontSize: 11, color: methodCol, fontWeight: 600 }}>{methodShort(m.method)}</span>
            <span style={{ fontSize: 11, color: color.faint }}>#{m.seed}</span>
            <span style={{ flex: 1 }} />
            <span style={{ fontSize: 10, color: color.muted }}>{m.n_vessels} vessels</span>
          </>
        ) : (
          <span style={{ fontSize: 11.5, color: color.muted }}>No run selected — pick one in the Runs tab</span>
        )}
      </div>

      {/* Live KPI tiles */}
      {hasRun && k && (
        <div style={{ display: "flex", gap: 6 }}>
          {tiles.map(t => (
            <div key={t.label} style={{
              flex: 1, background: color.bg, border: `1px solid ${color.border}`,
              borderRadius: radius.sm, padding: "6px 7px", minWidth: 0,
            }}>
              <div style={{ fontSize: 8, color: color.faint, textTransform: "uppercase", letterSpacing: "0.04em", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{t.label}</div>
              <div style={{ fontSize: 15, fontWeight: 700, color: t.c, fontVariantNumeric: "tabular-nums", lineHeight: 1.2 }}>{t.value}</div>
            </div>
          ))}
        </div>
      )}

      {/* Scrubber */}
      <Track idx={pb.currentIdx} max={Math.max(0, n - 1)} disabled={n === 0} onSeek={pb.setCurrentIdx} />

      {/* Transport controls */}
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <StepBtn title="Previous frame" disabled={n === 0} onClick={() => pb.setCurrentIdx(i => clamp(i - 1, 0, n - 1))}>◀</StepBtn>
        <button
          onClick={() => pb.setPlaying(p => !p)}
          disabled={n === 0}
          title={pb.playing ? "Pause" : "Play"}
          style={{
            width: 40, height: 40, borderRadius: "50%", flexShrink: 0,
            background: n === 0 ? color.border : color.accent,
            color: n === 0 ? color.faint : "#04252c",
            border: "none", cursor: n === 0 ? "default" : "pointer",
            fontSize: 14, display: "flex", alignItems: "center", justifyContent: "center",
            boxShadow: n === 0 ? "none" : `0 3px 14px ${color.accent}55`,
          }}
        >{pb.playing ? "❚❚" : "▶"}</button>
        <StepBtn title="Next frame" disabled={n === 0} onClick={() => pb.setCurrentIdx(i => clamp(i + 1, 0, n - 1))}>▶</StepBtn>

        <span style={{ flex: 1 }} />

        <span style={{ fontSize: 11, color: color.sub, fontVariantNumeric: "tabular-nums", textAlign: "right" }}>
          {pb.loading ? "loading…" : n === 0 ? "—" : (
            <>
              <span style={{ color: color.text, fontWeight: 600 }}>tick {step}</span>
              <span style={{ color: color.faint }}> · {pb.currentIdx + 1}/{n}</span>
            </>
          )}
        </span>

        <select
          value={pb.speedMs}
          onChange={e => pb.setSpeedMs(Number(e.target.value))}
          title="Playback speed"
          style={{
            background: color.bg, color: color.textDim, border: `1px solid ${color.hair}`,
            borderRadius: radius.sm, fontSize: 10.5, padding: "5px 6px", cursor: "pointer", outline: "none",
          }}
        >
          {SPEED_PRESETS.map(s => <option key={s.label} value={s.ms}>{s.label}</option>)}
        </select>
      </div>
    </div>
  );
}

function StepBtn({ children, onClick, title, disabled }: {
  children: React.ReactNode; onClick: () => void; title: string; disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      style={{
        width: 30, height: 30, borderRadius: radius.sm, flexShrink: 0,
        background: "transparent", border: `1px solid ${color.border}`,
        color: disabled ? color.faintest : color.textDim,
        cursor: disabled ? "default" : "pointer", fontSize: 9,
        display: "flex", alignItems: "center", justifyContent: "center",
      }}
    >{children}</button>
  );
}

/** A custom draggable progress track — click or drag anywhere to seek. */
function Track({ idx, max, disabled, onSeek }: {
  idx: number; max: number; disabled: boolean; onSeek: (i: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const pct = max > 0 ? (idx / max) * 100 : 0;

  const seekTo = useCallback((clientX: number) => {
    const el = ref.current;
    if (!el || max <= 0) return;
    const r = el.getBoundingClientRect();
    const p = clamp((clientX - r.left) / r.width, 0, 1);
    onSeek(Math.round(p * max));
  }, [max, onSeek]);

  const onPointerDown = (e: React.PointerEvent) => {
    if (disabled) return;
    seekTo(e.clientX);
    const move = (ev: PointerEvent) => seekTo(ev.clientX);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div
      ref={ref}
      onPointerDown={onPointerDown}
      style={{
        position: "relative", height: 18, display: "flex", alignItems: "center",
        cursor: disabled ? "default" : "pointer", touchAction: "none",
      }}
    >
      {/* track */}
      <div style={{ position: "absolute", left: 0, right: 0, height: 5, borderRadius: 3, background: color.hair }} />
      {/* fill */}
      <div style={{
        position: "absolute", left: 0, width: `${pct}%`, height: 5, borderRadius: 3,
        background: disabled ? color.faint : `linear-gradient(90deg,${color.accentDeep},${color.accent})`,
      }} />
      {/* thumb */}
      {!disabled && (
        <div style={{
          position: "absolute", left: `${pct}%`, width: 13, height: 13, borderRadius: "50%",
          transform: "translateX(-50%)", background: "#fff",
          border: `2px solid ${color.accent}`, boxShadow: "0 1px 6px rgba(0,0,0,.6)",
        }} />
      )}
    </div>
  );
}
