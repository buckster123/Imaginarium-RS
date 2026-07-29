//! SQLite job store.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tracing::debug;

use crate::error::{Error, Result};
use crate::types::{JobId, JobMode, JobResult, JobStatus};

/// Sync job store — Connection is !Sync, so we mutex it for axum Send futures.
pub struct JobStore {
    conn: Mutex<Connection>,
}

impl JobStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Db(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                job_id TEXT PRIMARY KEY,
                upstream_request_id TEXT,
                status TEXT NOT NULL,
                mode TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt TEXT,
                result_json TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_created ON jobs(created_at DESC);
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_result(&self, result: &JobResult) -> Result<()> {
        let result_json = serde_json::to_string(result)?;
        let conn = self.conn.lock().map_err(|e| Error::Db(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO jobs (job_id, upstream_request_id, status, mode, model, prompt, result_json, created_at, completed_at, error)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(job_id) DO UPDATE SET
                upstream_request_id = excluded.upstream_request_id,
                status = excluded.status,
                mode = excluded.mode,
                model = excluded.model,
                prompt = excluded.prompt,
                result_json = excluded.result_json,
                completed_at = excluded.completed_at,
                error = excluded.error
            WHERE NOT (
                jobs.status IN ('done', 'failed', 'expired', 'cancelled')
                AND excluded.status NOT IN ('done', 'failed', 'expired', 'cancelled')
            )
            "#,
            params![
                result.job_id.as_str(),
                result.upstream_request_id,
                result.status.as_str(),
                result.mode.as_str(),
                result.model,
                result.prompt,
                result_json,
                result.created_at.to_rfc3339(),
                result.completed_at.map(|t| t.to_rfc3339()),
                result.error,
            ],
        )?;
        debug!(job_id = %result.job_id, status = result.status.as_str(), "job upserted");
        Ok(())
    }

    pub fn get(&self, job_id: &JobId) -> Result<Option<JobResult>> {
        let conn = self.conn.lock().map_err(|e| Error::Db(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT result_json FROM jobs WHERE job_id = ?1")?;
        let mut rows = stmt.query(params![job_id.as_str()])?;
        if let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            let result: JobResult = serde_json::from_str(&json)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<JobListItem>> {
        let conn = self.conn.lock().map_err(|e| Error::Db(e.to_string()))?;
        let mut stmt = conn.prepare(
            r#"
            SELECT job_id, status, mode, model, created_at, error, prompt, result_json
            FROM jobs
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            // Gallery projection: prompt + asset count ride along so list
            // consumers (jobs rails, galleries) stop needing N+1 detail GETs.
            let result_json: Option<String> = row.get(7)?;
            let assets = result_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| v.get("assets").and_then(|a| a.as_array().map(|a| a.len())))
                .unwrap_or(0);
            Ok(JobListItem {
                job_id: JobId(row.get(0)?),
                status: row.get(1)?,
                mode: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                error: row.get(5)?,
                prompt: row.get(6)?,
                assets,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobListItem {
    pub job_id: JobId,
    pub status: String,
    pub mode: String,
    pub model: String,
    pub created_at: String,
    pub error: Option<String>,
    /// The job's prompt (None for imports) — additive; old clients ignore it.
    pub prompt: Option<String>,
    /// Media assets on the job (0 when unknown) — `?i=` addresses each.
    pub assets: usize,
}

/// Helper to stamp a running job skeleton.
pub fn pending_job(mode: JobMode, model: impl Into<String>, prompt: Option<String>) -> JobResult {
    JobResult {
        ok: true,
        job_id: JobId::new(),
        upstream_request_id: None,
        status: JobStatus::Pending,
        mode,
        model: model.into(),
        assets: vec![],
        usage: None,
        error: None,
        error_type: None,
        created_at: Utc::now(),
        completed_at: None,
        prompt,
    }
}

pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn job(id: &JobId, status: JobStatus) -> JobResult {
        let mut j = pending_job(JobMode::VideoGenerate, "grok-imagine-video", None);
        j.job_id = id.clone();
        j.status = status;
        j
    }

    #[test]
    fn terminal_job_not_clobbered_by_stale_running() {
        let dir = tempdir().unwrap();
        let store = JobStore::open(&dir.path().join("j.db")).unwrap();
        let id = JobId::new();
        store.upsert_result(&job(&id, JobStatus::Done)).unwrap();
        // a stale in-flight write for the same job lands late
        store.upsert_result(&job(&id, JobStatus::Running)).unwrap();
        assert_eq!(
            store.get(&id).unwrap().unwrap().status,
            JobStatus::Done,
            "a completed job must not be reset to running"
        );
    }

    #[test]
    fn running_still_advances_to_done() {
        let dir = tempdir().unwrap();
        let store = JobStore::open(&dir.path().join("j.db")).unwrap();
        let id = JobId::new();
        store.upsert_result(&job(&id, JobStatus::Running)).unwrap();
        store.upsert_result(&job(&id, JobStatus::Done)).unwrap();
        assert_eq!(store.get(&id).unwrap().unwrap().status, JobStatus::Done);
    }
}
