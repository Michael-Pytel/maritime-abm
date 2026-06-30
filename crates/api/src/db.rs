//! SQLite-backed store for batch-run KPI results, plus a Parquet exporter.
//!
//! The operational store is a single `results.db` file holding one row per run
//! (scenario × method × seed) keyed by `batch_id`. Tick-by-tick playback logs
//! stay as `.jsonl` files on disk — this table only records a pointer to them.
//! Parquet is offered purely as an analysis export, generated on demand from
//! the table for downstream tooling (pandas / polars / R).

use anyhow::Result;
use rusqlite::{params, Connection};
use simulation::kpi::KpiSnapshot;
use std::io::Write;

use crate::stats::BatchResult;

/// Default location of the operational results database.
pub const DB_PATH: &str = "outputs/results.db";

/// A fully-described completed run, as stored in the `runs` table.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub batch_id: String,
    /// Human-readable sweep-point label, auto-derived from the param override
    /// (e.g. `collision_warn_cpa_nm=0.5`), or `default` when unmodified.
    pub label: String,
    /// Compact JSON of the parameter override applied to this batch (`{}` when
    /// none). Lets a hyperparameter sweep be reconstructed and grouped.
    pub params: String,
    pub scenario: String,
    pub method: String,
    pub seed: u64,
    pub n_ticks: u32,
    pub n_vessels: u32,
    pub completed_at: u64,
    pub kpi: KpiSnapshot,
    pub log_path: String,
}

impl RunRecord {
    fn into_batch_result(self) -> BatchResult {
        BatchResult {
            scenario: self.scenario,
            method: self.method,
            seed: self.seed,
            kpi: self.kpi,
        }
    }
}

/// Opens (creating if needed) the results database and ensures the schema.
///
/// WAL mode lets readers (status/results polling) proceed while the batch
/// workers write, and serialises concurrent writers cleanly.
pub fn open(path: &str) -> Result<Connection> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id             TEXT    NOT NULL,
            label                TEXT    NOT NULL DEFAULT '',
            params               TEXT    NOT NULL DEFAULT '{}',
            scenario             TEXT    NOT NULL,
            method               TEXT    NOT NULL,
            seed                 INTEGER NOT NULL,
            n_ticks              INTEGER NOT NULL,
            n_vessels            INTEGER NOT NULL,
            completed_at         INTEGER NOT NULL,
            fatal_per_1k_hrs     REAL    NOT NULL,
            collision_per_1k_hrs REAL    NOT NULL,
            survival_ratio       REAL    NOT NULL,
            avg_tta_hours        REAL    NOT NULL,
            evac_activation_rate REAL    NOT NULL,
            mean_p_prep          REAL    NOT NULL,
            log_path             TEXT,
            UNIQUE(batch_id, scenario, method, seed)
        );
        CREATE INDEX IF NOT EXISTS idx_runs_batch ON runs(batch_id);",
    )?;
    // Idempotent migration for databases created before sweep columns existed.
    // SQLite has no `ADD COLUMN IF NOT EXISTS`; a duplicate-column error is the
    // expected no-op signal, so it is intentionally ignored.
    let _ = conn.execute(
        "ALTER TABLE runs ADD COLUMN label TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE runs ADD COLUMN params TEXT NOT NULL DEFAULT '{}'",
        [],
    );
    Ok(conn)
}

/// Inserts (or replaces, on re-run of the same cell) a single completed run.
#[allow(clippy::cast_possible_wrap)]
pub fn insert_run(conn: &Connection, r: &RunRecord) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO runs
            (batch_id, label, params, scenario, method, seed, n_ticks, n_vessels,
             completed_at, fatal_per_1k_hrs, collision_per_1k_hrs, survival_ratio,
             avg_tta_hours, evac_activation_rate, mean_p_prep, log_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            r.batch_id,
            r.label,
            r.params,
            r.scenario,
            r.method,
            r.seed as i64,
            i64::from(r.n_ticks),
            i64::from(r.n_vessels),
            r.completed_at as i64,
            r.kpi.fatal_per_1k_hrs,
            r.kpi.collision_per_1k_hrs,
            r.kpi.survival_ratio,
            r.kpi.avg_tta_hours,
            r.kpi.evac_activation_rate,
            r.kpi.mean_p_prep,
            r.log_path,
        ],
    )?;
    Ok(())
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        batch_id: row.get("batch_id")?,
        label: row.get::<_, Option<String>>("label")?.unwrap_or_default(),
        params: row
            .get::<_, Option<String>>("params")?
            .unwrap_or_else(|| "{}".to_string()),
        scenario: row.get("scenario")?,
        method: row.get("method")?,
        seed: row.get::<_, i64>("seed")? as u64,
        n_ticks: row.get::<_, i64>("n_ticks")? as u32,
        n_vessels: row.get::<_, i64>("n_vessels")? as u32,
        completed_at: row.get::<_, i64>("completed_at")? as u64,
        kpi: KpiSnapshot {
            step: 0,
            fatal_per_1k_hrs: row.get("fatal_per_1k_hrs")?,
            collision_per_1k_hrs: row.get("collision_per_1k_hrs")?,
            survival_ratio: row.get("survival_ratio")?,
            avg_tta_hours: row.get("avg_tta_hours")?,
            evac_activation_rate: row.get("evac_activation_rate")?,
            mean_p_prep: row.get("mean_p_prep")?,
        },
        log_path: row
            .get::<_, Option<String>>("log_path")?
            .unwrap_or_default(),
    })
}

/// All runs for one batch, ordered for stable output.
pub fn records_for_batch(conn: &Connection, batch_id: &str) -> Result<Vec<RunRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM runs WHERE batch_id = ?1 ORDER BY scenario, method, seed")?;
    let rows = stmt
        .query_map([batch_id], row_to_record)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every run across all batches — the full hyperparameter-sweep table.
/// Ordered by completion so successive sweep points read in run order.
pub fn records_all(conn: &Connection) -> Result<Vec<RunRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM runs ORDER BY completed_at, batch_id, scenario, method, seed")?;
    let rows = stmt
        .query_map([], row_to_record)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The `batch_id` of the most recently completed batch, if any.
#[allow(clippy::unnecessary_wraps)] // keep `Result` for `?` ergonomics at call sites
pub fn latest_batch_id(conn: &Connection) -> Result<Option<String>> {
    let id = conn
        .query_row(
            "SELECT batch_id FROM runs ORDER BY completed_at DESC, id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(id)
}

/// `BatchResult` records for a batch (current batch if `batch_id` is given,
/// otherwise the most recent one). Returns an empty vec on any failure so
/// callers can fall back gracefully.
pub fn results(batch_id: Option<&str>) -> Vec<BatchResult> {
    let Ok(conn) = open(DB_PATH) else {
        return Vec::new();
    };
    let id = match batch_id {
        Some(id) => id.to_string(),
        None => match latest_batch_id(&conn) {
            Ok(Some(id)) => id,
            _ => return Vec::new(),
        },
    };
    records_for_batch(&conn, &id)
        .map(|recs| recs.into_iter().map(RunRecord::into_batch_result).collect())
        .unwrap_or_default()
}

/// Serialises runs to an in-memory Parquet buffer for download.
///
/// `all = true` exports every batch (the whole sweep). Otherwise `batch_id`
/// selects one batch, or `None` falls back to the most recent.
/// Returns `(filename, bytes)`.
pub fn export_parquet(batch_id: Option<&str>, all: bool) -> Result<(String, Vec<u8>)> {
    let conn = open(DB_PATH)?;
    let (filename, recs) = if all {
        ("sweep_all.parquet".to_string(), records_all(&conn)?)
    } else {
        let id = match batch_id {
            Some(id) => id.to_string(),
            None => latest_batch_id(&conn)?
                .ok_or_else(|| anyhow::anyhow!("no batches available to export"))?,
        };
        (
            format!("batch_{id}.parquet"),
            records_for_batch(&conn, &id)?,
        )
    };
    if recs.is_empty() {
        anyhow::bail!("no runs found to export");
    }
    let bytes = records_to_parquet(&recs)?;
    Ok((filename, bytes))
}

/// Encodes run records as a Parquet byte buffer (one row per run).
fn records_to_parquet(recs: &[RunRecord]) -> Result<Vec<u8>> {
    use arrow::array::{Float64Array, StringArray, UInt32Array, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use std::sync::Arc;

    let schema = Arc::new(Schema::new(vec![
        Field::new("batch_id", DataType::Utf8, false),
        Field::new("label", DataType::Utf8, false),
        Field::new("params", DataType::Utf8, false),
        Field::new("scenario", DataType::Utf8, false),
        Field::new("method", DataType::Utf8, false),
        Field::new("seed", DataType::UInt64, false),
        Field::new("n_ticks", DataType::UInt32, false),
        Field::new("n_vessels", DataType::UInt32, false),
        Field::new("completed_at", DataType::UInt64, false),
        Field::new("fatal_per_1k_hrs", DataType::Float64, false),
        Field::new("collision_per_1k_hrs", DataType::Float64, false),
        Field::new("survival_ratio", DataType::Float64, false),
        Field::new("avg_tta_hours", DataType::Float64, false),
        Field::new("evac_activation_rate", DataType::Float64, false),
        Field::new("mean_p_prep", DataType::Float64, false),
    ]));

    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(StringArray::from_iter_values(
                recs.iter().map(|r| r.batch_id.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                recs.iter().map(|r| r.label.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                recs.iter().map(|r| r.params.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                recs.iter().map(|r| r.scenario.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                recs.iter().map(|r| r.method.as_str()),
            )),
            Arc::new(UInt64Array::from_iter_values(recs.iter().map(|r| r.seed))),
            Arc::new(UInt32Array::from_iter_values(
                recs.iter().map(|r| r.n_ticks),
            )),
            Arc::new(UInt32Array::from_iter_values(
                recs.iter().map(|r| r.n_vessels),
            )),
            Arc::new(UInt64Array::from_iter_values(
                recs.iter().map(|r| r.completed_at),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.fatal_per_1k_hrs),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.collision_per_1k_hrs),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.survival_ratio),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.avg_tta_hours),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.evac_activation_rate),
            )),
            Arc::new(Float64Array::from_iter_values(
                recs.iter().map(|r| r.kpi.mean_p_prep),
            )),
        ],
    )?;

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = ArrowWriter::try_new(&mut buf, schema, None)?;
        writer.write(&batch)?;
        writer.close()?;
    }
    buf.flush().ok();

    Ok(buf)
}

/// Recursively merges a parameter `override` object onto a base JSON value.
/// Objects merge key-by-key; any other value (scalar, array) replaces wholesale.
pub fn deep_merge(base: &mut serde_json::Value, ov: &serde_json::Value) {
    use serde_json::Value;
    match (base, ov) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                deep_merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// Derives a compact sweep-point label from a param-override object, e.g.
/// `{"collision_warn_cpa_nm":0.5,"forcefield":{"enabled":true}}` →
/// `collision_warn_cpa_nm=0.5_forcefield.enabled=true`. Empty override → `default`.
#[must_use]
pub fn derive_label(ov: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    flatten_label("", ov, &mut parts);
    if parts.is_empty() {
        return "default".to_string();
    }
    let mut label = parts.join("_");
    label.truncate(120); // keep labels manageable for wide overrides
    label
}

fn flatten_label(prefix: &str, v: &serde_json::Value, parts: &mut Vec<String>) {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_label(&key, val, parts);
            }
        }
        Value::String(s) => parts.push(format!("{prefix}={s}")),
        other => parts.push(format!("{prefix}={other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(batch_id: &str, seed: u64, completed_at: u64) -> RunRecord {
        RunRecord {
            batch_id: batch_id.to_string(),
            label: "default".to_string(),
            params: "{}".to_string(),
            scenario: "CalmPassage".to_string(),
            method: "ProposedSystem".to_string(),
            seed,
            n_ticks: 100,
            n_vessels: 12,
            completed_at,
            kpi: KpiSnapshot {
                step: 0,
                fatal_per_1k_hrs: 0.1,
                collision_per_1k_hrs: 0.2,
                survival_ratio: 0.99,
                avg_tta_hours: 1.5,
                evac_activation_rate: 0.3,
                mean_p_prep: 0.7,
            },
            log_path: format!("outputs/{batch_id}_{seed}.jsonl"),
        }
    }

    fn temp_db() -> String {
        let p = std::env::temp_dir().join(format!("abm_test_{}.db", uuid::Uuid::new_v4()));
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn roundtrip_is_scoped_by_batch() {
        let path = temp_db();
        let conn = open(&path).unwrap();
        insert_run(&conn, &sample("b1", 1, 100)).unwrap();
        insert_run(&conn, &sample("b1", 2, 101)).unwrap();
        insert_run(&conn, &sample("b2", 1, 200)).unwrap();

        // Results never mix across batches.
        assert_eq!(records_for_batch(&conn, "b1").unwrap().len(), 2);
        assert_eq!(records_for_batch(&conn, "b2").unwrap().len(), 1);
        // Most recent batch wins (highest completed_at).
        assert_eq!(latest_batch_id(&conn).unwrap().as_deref(), Some("b2"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn insert_or_replace_dedupes_rerun_cell() {
        let path = temp_db();
        let conn = open(&path).unwrap();
        insert_run(&conn, &sample("b1", 1, 100)).unwrap();
        insert_run(&conn, &sample("b1", 1, 105)).unwrap(); // same cell re-run
        assert_eq!(records_for_batch(&conn, "b1").unwrap().len(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn deep_merge_is_recursive_and_scalars_replace() {
        let mut base = serde_json::json!({
            "collision_warn_cpa_nm": 5.0,
            "forcefield": { "enabled": false, "gain": 1.0 }
        });
        let ov = serde_json::json!({
            "collision_warn_cpa_nm": 0.5,
            "forcefield": { "enabled": true }   // gain must be preserved
        });
        deep_merge(&mut base, &ov);
        assert_eq!(base["collision_warn_cpa_nm"], serde_json::json!(0.5));
        assert_eq!(base["forcefield"]["enabled"], serde_json::json!(true));
        assert_eq!(base["forcefield"]["gain"], serde_json::json!(1.0));
    }

    #[test]
    fn derive_label_flattens_nested_keys() {
        let ov = serde_json::json!({
            "collision_warn_cpa_nm": 0.5,
            "forcefield": { "enabled": true }
        });
        let label = derive_label(&ov);
        assert!(label.contains("collision_warn_cpa_nm=0.5"));
        assert!(label.contains("forcefield.enabled=true"));
        assert_eq!(derive_label(&serde_json::json!({})), "default");
    }

    #[test]
    fn parquet_buffer_has_valid_framing() {
        let recs = vec![sample("b1", 1, 100), sample("b1", 2, 101)];
        let bytes = records_to_parquet(&recs).unwrap();
        // Parquet files are delimited by the "PAR1" magic at both ends.
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }

    #[test]
    fn sweep_export_spans_all_batches_with_params() {
        let path = temp_db();
        let conn = open(&path).unwrap();
        // Two sweep points (distinct param overrides), each with 2 seeds.
        for seed in 1..=2 {
            let mut r = sample("b1", seed, 100 + seed);
            r.label = "collision_warn_cpa_nm=0.5".to_string();
            r.params = r#"{"collision_warn_cpa_nm":0.5}"#.to_string();
            insert_run(&conn, &r).unwrap();

            let mut r2 = sample("b2", seed, 200 + seed);
            r2.label = "collision_warn_cpa_nm=2.0".to_string();
            r2.params = r#"{"collision_warn_cpa_nm":2.0}"#.to_string();
            insert_run(&conn, &r2).unwrap();
        }

        // The whole-sweep view retains every batch (append-only, not latest-only).
        let all = records_all(&conn).unwrap();
        assert_eq!(all.len(), 4);
        let labels: std::collections::HashSet<_> = all.iter().map(|r| r.label.clone()).collect();
        assert_eq!(labels.len(), 2);
        assert!(all.iter().any(|r| r.params.contains("0.5")));

        let bytes = records_to_parquet(&all).unwrap();
        assert_eq!(&bytes[..4], b"PAR1");
        std::fs::remove_file(&path).ok();
    }
}
