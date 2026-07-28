//! Local asset library helpers (download + layout).

use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::info;

use crate::error::Result;
use crate::types::JobId;

#[derive(Debug, Clone)]
pub struct Library {
    /// Root of the library tree (…/library).
    pub root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `library/YYYY/MM/DD/<job_id>/`
    pub fn ensure_job_dir(&self, job_id: &JobId) -> Result<PathBuf> {
        let when = Utc::now();
        let dir = self
            .root
            .join(when.format("%Y").to_string())
            .join(when.format("%m").to_string())
            .join(when.format("%d").to_string())
            .join(job_id.as_str());
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn write_meta(&self, job_dir: &Path, meta: &impl serde::Serialize) -> Result<PathBuf> {
        let path = job_dir.join("meta.json");
        let body = serde_json::to_vec_pretty(meta)?;
        std::fs::write(&path, body)?;
        Ok(path)
    }

    pub fn write_prompt(&self, job_dir: &Path, prompt: &str) -> Result<PathBuf> {
        let path = job_dir.join("prompt.txt");
        std::fs::write(&path, prompt)?;
        Ok(path)
    }
}

/// Download a URL to `dest`.
pub async fn download_url(client: &reqwest::Client, url: &str, dest: &Path) -> Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    let len = bytes.len() as u64;
    std::fs::write(dest, &bytes)?;
    info!(path = %dest.display(), bytes = len, "downloaded asset");
    Ok(len)
}
