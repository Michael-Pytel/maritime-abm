/**
 * Central design tokens for the map-first UI. Graphite surfaces + a cyan accent
 * (the "modern marine dashboard" look). Consolidates the palette that was
 * previously scattered as inline hex across components, plus the domain colour
 * maps (methods, scenarios, KPIs). New components import from here; keep values
 * in sync with MapGL's layer colours (which stay numeric RGB and live there).
 */

export const color = {
  // Surfaces — neutral graphite, faint cool tint
  bg: "#0a0e14",
  panel: "#10151e",
  panel2: "#0c1017",
  panelSel: "#132433",
  // Borders
  border: "#1f2733",
  borderBlue: "#1b3a45",   // cyan-tinted border for accented panels
  hair: "#2a3542",
  // Accent — cyan / teal
  accent: "#22d3ee",
  accentDeep: "#0e7490",
  accentText: "#67e8f9",
  // Text ramp (bright → faint)
  text: "#eef2f6",
  textDim: "#c7d0db",
  sub: "#93a1b1",
  muted: "#647184",
  faint: "#465264",
  faintest: "#2a3542",
  // Semantic
  good: "#34d399",
  bad: "#f87171",
  warn: "#fbbf24",
  amber: "#f59e0b",
  cyan: "#22d3ee",
  violet: "#c084fc",
  rose: "#fb7185",
  blue: "#60a5fa",
} as const;

export const radius = { sm: 6, md: 8, lg: 11, pill: 20 } as const;

/** Floating-panel / dock shadow. */
export const shadow = "0 8px 30px rgba(0,0,0,.5)";

/** The brand mark gradient (replaces the old anchor tile). */
export const brandGradient = "linear-gradient(135deg,#22d3ee,#0e7490)";

/** AIS route / vessel-type filter selector value. */
export type RouteFilter = "all" | "cargo" | "passenger" | "tanker";

/** Canonical method colours (unifies the two divergent maps that existed). */
export const METHOD_COLORS: Record<string, string> = {
  ProposedSystem: "#38bdf8",
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
  cargo: "#38bdf8",
  passenger: "#34d399",
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
