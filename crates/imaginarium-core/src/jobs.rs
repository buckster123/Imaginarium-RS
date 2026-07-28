//! SQLite job store (Phase 1 skeleton).

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tracing::debug;

use crate::error::Result;
use crate::types::{JobId, JobMode, JobResult, JobStatus};

pub struct JobStore {
    conn: Connection,
}

impl JobStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
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
        self.conn.execute(
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
        let mut stmt = self
            .conn
            .prepare("SELECT result_json FROM jobs WHERE job_id = ?1")?;
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
        let mut stmt = self.conn.prepare(
            r#"
            SELECT job_id, status, mode, model, created_at, error
            FROM jobs
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(JobListItem {
                job_id: JobId(row.get(0)?),
                status: row.get(1)?,
                mode: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                error: row.get(5)?,
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
