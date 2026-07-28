//! Local asset library helpers (download + layout + craft import).

use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::info;

use crate::error::{Error, Result};
use crate::jobs::JobStore;
use crate::types::{Asset, AssetId, AssetKind, JobId, JobMode, JobResult, JobStatus};

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

    /// Import raw bytes as a new completed library job (Studio+ craft / upload).
    pub fn import_bytes(
        &self,
        jobs: &JobStore,
        bytes: &[u8],
        filename_hint: &str,
        note: Option<&str>,
        source_job_id: Option<&str>,
    ) -> Result<JobResult> {
        let ext = extension_from_name(filename_hint)
            .unwrap_or_else(|| sniff_ext(bytes).unwrap_or("bin").to_string());
        let kind = if matches!(ext.as_str(), "mp4" | "webm" | "mov") {
            AssetKind::Video
        } else {
            AssetKind::Image
        };
        let job_id = JobId::new();
        let dir = self.ensure_job_dir(&job_id)?;
        let file_name = format!("00.{ext}");
        let path = dir.join(&file_name);
        std::fs::write(&path, bytes)?;

        let note = note.unwrap_or("craft import");
        let _ = self.write_prompt(&dir, note);
        let meta = serde_json::json!({
            "source": "library_import",
            "filename_hint": filename_hint,
            "source_job_id": source_job_id,
            "bytes": bytes.len(),
        });
        let _ = self.write_meta(&dir, &meta);

        let content_url = format!("/v1/library/{}/content", job_id.as_str());
        let result = JobResult {
            ok: true,
            job_id: job_id.clone(),
            upstream_request_id: None,
            status: JobStatus::Done,
            mode: JobMode::CraftExport,
            model: "local-craft".into(),
            assets: vec![Asset {
                id: AssetId::new(),
                kind,
                local_path: Some(path.display().to_string()),
                content_url: Some(content_url),
                upstream_url: None,
                file_id: None,
                public_url: None,
                mime_type: Some(mime_for_ext(&ext).into()),
            }],
            usage: None,
            error: None,
            error_type: None,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            prompt: Some(note.into()),
        };
        jobs.upsert_result(&result)?;
        info!(job_id = %job_id, path = %path.display(), "library import");
        Ok(result)
    }
}

/// True if `id` is a safe library path segment (a job/asset id): non-empty, bounded,
/// and composed only of `[A-Za-z0-9_-]`. Used to guard every `…/{id}/…` filesystem
/// join against path traversal (`..`, `/`, `\`, percent-encoded separators). ULIDs
/// (Crockford base32) satisfy this.
pub fn is_safe_asset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn extension_from_name(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn sniff_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.windows(4).any(|w| w == b"ftyp") {
        return Some("mp4");
    }
    None
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
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

/// Decode `data:image/png;base64,...` or raw base64 into bytes + suggested ext.
pub fn decode_data_url_or_b64(input: &str) -> Result<(Vec<u8>, String)> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("data:") {
        let (meta, b64) = rest
            .split_once(',')
            .ok_or_else(|| Error::other("malformed data URL"))?;
        let mime = meta.split(';').next().unwrap_or("application/octet-stream");
        let ext = match mime {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            _ => "bin",
        };
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| Error::other(format!("base64: {e}")))?;
        return Ok((bytes, ext.into()));
    }
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| Error::other(format!("base64: {e}")))?;
    let ext = sniff_ext(&bytes).unwrap_or("bin").to_string();
    Ok((bytes, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_asset_id_accepts_ids_rejects_traversal() {
        assert!(is_safe_asset_id("01KYMRHFX5GSSMTWF5W9RTCT9D"));
        assert!(is_safe_asset_id("craft-01_abc"));
        assert!(!is_safe_asset_id(""));
        assert!(!is_safe_asset_id("a/b"));
        assert!(!is_safe_asset_id("../../etc/passwd"));
        assert!(!is_safe_asset_id("..%2f..%2fetc"));
        assert!(!is_safe_asset_id("a\\b"));
        assert!(!is_safe_asset_id(&"x".repeat(65)));
    }
}
