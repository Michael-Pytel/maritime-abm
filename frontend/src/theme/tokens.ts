/**
 * Central design tokens for the map-first UI. Consolidates the palette that was
 * previously scattered as inline hex across components, plus the domain colour
 * maps (methods, scenarios, KPIs) that were duplicated in StatsPanel / BatchPanel /
 * RunsWorkspace. New components import from here; keep values in sync with MapGL's
 * layer colours (which must stay numeric RGB and live there).
 */

export const color = {
  // Surfaces
  bg: "#0a0f1e",
  panel: "#0d1526",
  panel2: "#0c1220",
  panelSel: "#12233f",
  // Borders
  border: "#1e293b",
  borderBlue: "#1e3a5f",
  hair: "#334155",
  // Accent
  accent: "#3b82f6",
  accentDeep: "#1d4ed8",
  accentText: "#93c5fd",
  // Text ramp (bright → faint)
  text: "#f1f5f9",
  textDim: "#cbd5e1",
  sub: "#94a3b8",
  muted: "#64748b",
  faint: "#475569",
  faintest: "#334155",
  // Semantic
  good: "#34d399",
  bad: "#f87171",
  warn: "#fbbf24",
  amber: "#f59e0b",
  cyan: "#22d3ee",
  violet: "#c084fc",
} as const;

export const radius = { sm: 6, md: 8, lg: 10, pill: 20 } as const;

/** Floating-island / drawer shadow — the only shadow we use. */
export const shadow = "0 6px 24px rgba(0,0,0,.45)";

/** Canonical method colours (unifies the two divergent maps that existed). */
export const METHOD_COLORS: Record<string, string> = {
  ProposedSystem: "#3b82f6",
  BaselineA: "#94a3b8",
  BaselineB: "#fbbf24",
};

/** Short display method label. */
export function methodShort(m: string): string {
  return m === "ProposedSystem" ? "Proposed" : m.replace("Baseline", "B-");
}

/** Scenario short names for compact chips/cards. */
export const SCEN_SHORT: Record<string, string> = {
  CalmPassage: "Calm",
  StormCorridor: "Storm",
  BlindShore: "Blind",
  DeepWaterRescue: "Deep",
};

export const ALL_SCENARIOS = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"];
export const ALL_METHODS = ["BaselineA", "BaselineB", "ProposedSystem"];

/** The 6 experiment KPIs with presentation metadata (label / unit / polarity / colour). */
export const KPI_META: Record<string, { label: string; unit: string; higherBetter: boolean; color: string }> = {
  survival_ratio:       { label: "Survival Ratio",       unit: "",        higherBetter: true,  color: "#34d399" },
  fatal_per_1k_hrs:     { label: "Fatalities /1k hrs",   unit: "/1k hrs", higherBetter: false, color: "#f87171" },
  mean_p_prep:          { label: "Mean Preparedness",    unit: "",        higherBetter: true,  color: "#60a5fa" },
  collision_per_1k_hrs: { label: "Collisions /1k hrs",   unit: "/1k hrs", higherBetter: false, color: "#fb923c" },
  avg_tta_hours:        { label: "Avg Time-to-Rescue",   unit: "hrs",     higherBetter: false, color: "#fbbf24" },
  evac_activation_rate: { label: "Evac Activation Rate", unit: "",        higherBetter: false, color: "#c084fc" },
};

/** Vessel-type colours for the AIS route layer + filter (hex form; MapGL keeps RGB). */
export const TYPE_HEX: Record<string, string> = {
  cargo: "#3b82f6",
  passenger: "#22c55e",
  tanker: "#f97316",
};
export const TYPE_LABELS: Record<string, string> = {
  cargo: "Cargo",
  passenger: "Passenger",
  tanker: "Tanker",
};

/** Shared recharts styling so every chart in the app reads as one system. */
export const CHART = {
  card: { background: color.panel, border: `1px solid ${color.border}`, borderRadius: radius.md, padding: "14px 16px" },
  axis: { fontSize: 9, fill: color.faint },
  tooltip: {
    contentStyle: { background: color.panel, border: `1px solid ${color.border}`, fontSize: 11, color: color.text },
    labelStyle: { color: color.sub },
  },
} as const;
