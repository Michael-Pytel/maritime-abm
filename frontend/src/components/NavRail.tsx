import { color } from "../theme/tokens";

export type Drawer = "runs" | "stats";

interface Props {
  active: Drawer | null;
  onSelect: (d: Drawer) => void;
  onComms: () => void;
}

const ITEMS: { id: Drawer; icon: string; label: string }[] = [
  { id: "runs", icon: "◎", label: "Runs" },
  { id: "stats", icon: "▦", label: "Statistics" },
];

/** Slim left icon rail — brand mark + drawer togglers + a Comms shortcut. */
export default function NavRail({ active, onSelect, onComms }: Props) {
  return (
    <div style={{
      width: 60, flexShrink: 0, background: "#060c18",
      borderRight: `1px solid ${color.border}`,
      display: "flex", flexDirection: "column", alignItems: "center",
      padding: "10px 0", gap: 4, zIndex: 5,
    }}>
      {/* Brand */}
      <div style={{
        width: 34, height: 34, borderRadius: 8, marginBottom: 8,
        background: "linear-gradient(135deg, #1d4ed8, #0ea5e9)",
        display: "flex", alignItems: "center", justifyContent: "center", fontSize: 17,
      }}>⚓</div>

      {ITEMS.map(it => (
        <RailButton
          key={it.id}
          icon={it.icon}
          label={it.label}
          active={active === it.id}
          onClick={() => onSelect(it.id)}
        />
      ))}

      <div style={{ flex: 1 }} />

      <RailButton icon="📡" label="Comms" onClick={onComms} />
    </div>
  );
}

function RailButton({
  icon, label, active, onClick,
}: {
  icon: string; label: string; active?: boolean; onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      style={{
        width: 46, height: 46, borderRadius: 10,
        display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 2,
        background: active ? color.panelSel : "transparent",
        border: `1px solid ${active ? color.borderBlue : "transparent"}`,
        color: active ? color.accentText : color.muted,
        cursor: "pointer",
      }}
    >
      <span style={{ fontSize: 16, lineHeight: 1 }}>{icon}</span>
      <span style={{ fontSize: 7.5, letterSpacing: "0.03em", textTransform: "uppercase" }}>{label}</span>
    </button>
  );
}
