export interface VesselSnapshot {
  id: number;
  name: string;
  lat: number;
  lon: number;
  heading_deg: number;
  state: "Active" | "Docked";
  n_crew: number;
  dock_until_tick?: number | null;
  avoiding?: boolean;
}

// ── Collision-avoidance comms ─────────────────────────────────────────────────

export type MsgKind =
  | { type: "collision_warning"; cpa_nm: number; tta_ticks: number }
  | { type: "giving_way" }
  | { type: "maintaining_course" }
  | { type: "resume_route" };

export interface VesselMsg {
  tick: number;
  from_id: number;
  from_name: string;
  to_id: number;
  to_name: string;
  kind: MsgKind;
}

export interface KpiSnapshot {
  step: number;
  fatal_per_1k_hrs: number;
  collision_per_1k_hrs: number;
  survival_ratio: number;
  avg_tta_hours: number;
  evac_activation_rate: number;
  mean_p_prep: number;
}

export interface Port {
  id: number;
  name: string;
  lat: number;
  lon: number;
}

export interface SimBBox {
  lat_min: number;
  lat_max: number;
  lon_min: number;
  lon_max: number;
}

export interface TelemetrySnapshot {
  active_count: number;
  docked_count?: number;
  avoiding_count?: number;
  evac_count: number;
  fatal_cumulative: number;
  rescued_cumulative: number;
  rescue_agent_count: number;
  wreck_count?: number;
}

export interface CollisionEvent {
  tick: number;
  lat: number;
  lon: number;
}

export interface Storm {
  lat: number;
  lon: number;
  radius_nm: number;
}

export interface WeatherChannels {
  hazard?: number[];
  sea_state?: number[];
  wind?: number[];
  wind_direction?: number[];
  visibility?: number[];
  precipitation?: number[];
  pressure?: number[];
  tide?: number[];
  surge?: number[];
  total_water_level?: number[];
  wave_height?: number[];
  wave_period?: number[];
}

export interface TickMessage {
  step: number;
  vessels: VesselSnapshot[];
  kpis: KpiSnapshot;
  bbox?: SimBBox;
  ports?: Port[];
  telemetry?: TelemetrySnapshot;
  comms_log?: VesselMsg[];
  collision_events?: CollisionEvent[];
  storm?: Storm | null;
  /** Flat row-major array of W ∈ [0,1] values, length = weather_grid_size² */
  weather_grid?: number[];
  weather_grid_size?: number;
  weather_channels?: WeatherChannels;
}

export interface BatchStatus {
  batch_id?: string;
  total: number;
  completed: number;
  failed?: number;
  running: boolean;
  progress_pct: number;
}

export interface BatchResult {
  scenario: string;
  method: string;
  seed: number;
  kpi: KpiSnapshot;
}

export interface HypothesisResult {
  kpi: string;
  scenario: string;
  method_a: string;
  method_b: string;
  mean_a: number;
  mean_b: number;
  u_stat: number;
  z_score: number;
  p_value: number;
  p_corrected: number;
  cliffs_delta: number;
  effect_size: number;
  significant: boolean;
  confirmed: boolean;
}

export interface RunManifest {
  run_id: string;
  scenario: string;
  method: string;
  seed: number;
  n_ticks: number;
  n_vessels: number;
  completed_at: number;
  kpis: KpiSnapshot;
  /** True when a .jsonl tick-log exists alongside the manifest. */
  has_log?: boolean;
}
