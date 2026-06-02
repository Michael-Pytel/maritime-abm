import { useEffect, useRef } from "react";
import type { VesselMsg } from "../types";

// ── Colour / label helpers ────────────────────────────────────────────────────

const KIND_META: Record<string, { label: string; color: string; icon: string }> = {
  collision_warning:  { label: "COLLISION WARNING", color: "#f87171", icon: "⚠" },
  giving_way:         { label: "GIVING WAY",        color: "#f59e0b", icon: "↩" },
  maintaining_course: { label: "MAINTAINING COURSE",color: "#60a5fa", icon: "→" },
  resume_route:       { label: "ROUTE RESUMED",     color: "#4ade80", icon: "✓" },
};

function kindMeta(type: string) {
  return KIND_META[type] ?? { label: type.toUpperCase(), color: "#94a3b8", icon: "·" };
}

// ── Sub-components ────────────────────────────────────────────────────────────

function MsgRow({ msg }: { msg: VesselMsg }) {
  const { type } = msg.kind;
  const { label, color, icon } = kindMeta(type);

  const detail =
    type === "collision_warning"
      ? ` — CPA ${(msg.kind as { cpa_nm: number; tta_ticks: number }).cpa_nm.toFixed(2)} nm in ${
          (msg.kind as { cpa_nm: number; tta_ticks: number }).tta_ticks
        } ticks`
      : "";

  return (
    <div style={{
      display: "flex", gap: 8, alignItems: "flex-start",
      padding: "5px 0",
      borderBottom: "1px solid #1e293b",
    }}>
      {/* tick badge */}
      <span style={{
        flexShrink: 0, width: 44, textAlign: "right",
        fontSize: 9, color: "#475569", fontVariantNumeric: "tabular-nums",
        paddingTop: 1,
      }}>
        t{msg.tick}
      </span>

      {/* icon */}
      <span style={{ flexShrink: 0, color, fontSize: 11, width: 14, textAlign: "center" }}>
        {icon}
      </span>

      {/* body */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 10, fontWeight: 700, color, letterSpacing: "0.05em" }}>
          {label}{detail}
        </div>
        <div style={{ fontSize: 10, color: "#94a3b8", marginTop: 1 }}>
          <span style={{ color: "#cbd5e1" }}>{msg.from_name}</span>
          <span style={{ color: "#334155" }}> → </span>
          <span style={{ color: "#cbd5e1" }}>{msg.to_name}</span>
        </div>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface Props {
  messages: VesselMsg[];
}

export default function CommsLog({ messages }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to newest message.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>

      {/* Header */}
      <div style={{
        padding: "8px 16px",
        background: "#0d1526",
        borderBottom: "1px solid #1e293b",
        flexShrink: 0,
        display: "flex", alignItems: "center", gap: 10,
      }}>
        <span style={{ fontSize: 10, fontWeight: 700, color: "#475569", textTransform: "uppercase", letterSpacing: "0.08em" }}>
          Vessel Communications
        </span>
        <span style={{
          marginLeft: "auto",
          padding: "2px 8px", borderRadius: 10,
          background: messages.length > 0 ? "#f59e0b22" : "#1e293b",
          border: `1px solid ${messages.length > 0 ? "#f59e0b44" : "#1e3a5f"}`,
          fontSize: 10, color: messages.length > 0 ? "#f59e0b" : "#475569",
        }}>
          {messages.length} messages
        </span>
      </div>

      {/* Legend */}
      <div style={{
        display: "flex", gap: 12, flexWrap: "wrap",
        padding: "6px 16px",
        background: "#060c18",
        borderBottom: "1px solid #1e293b",
        flexShrink: 0,
      }}>
        {Object.entries(KIND_META).map(([, { label, color, icon }]) => (
          <span key={label} style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 9, color: "#475569" }}>
            <span style={{ color, fontSize: 10 }}>{icon}</span>
            <span style={{ color }}>{label}</span>
          </span>
        ))}
      </div>

      {/* Message feed */}
      <div style={{
        flex: 1, overflowY: "auto",
        padding: "0 16px",
        background: "#0a0f1e",
      }}>
        {messages.length === 0 ? (
          <div style={{
            height: "100%", display: "flex", flexDirection: "column",
            alignItems: "center", justifyContent: "center",
            color: "#334155", gap: 8,
          }}>
            <span style={{ fontSize: 28 }}>📡</span>
            <span style={{ fontSize: 12 }}>No messages yet</span>
            <span style={{ fontSize: 10, color: "#1e3a5f" }}>
              Vessel conversations appear here when collision risk is detected
            </span>
          </div>
        ) : (
          <>
            {messages.map((msg, i) => (
              <MsgRow key={`${msg.tick}-${msg.from_id}-${msg.to_id}-${i}`} msg={msg} />
            ))}
            <div ref={bottomRef} />
          </>
        )}
      </div>
    </div>
  );
}
