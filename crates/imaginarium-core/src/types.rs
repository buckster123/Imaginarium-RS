//! Shared domain types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::error::{Error, Result};

/// Local primary job identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub String);

impl JobId {
    pub fn new() -> Self {
        Self(Ulid::new().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(pub String);

impl AssetId {
    pub fn new() -> Self {
        Self(Ulid::new().to_string())
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    Expired,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobMode {
    ImageGenerate,
    ImageEdit,
    VideoGenerate,
    VideoEdit,
    VideoExtend,
    /// Local craft export / library import (no upstream).
    CraftExport,
}

impl JobMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImageGenerate => "image_generate",
            Self::ImageEdit => "image_edit",
            Self::VideoGenerate => "video_generate",
            Self::VideoEdit => "video_edit",
            Self::VideoExtend => "video_extend",
            Self::CraftExport => "craft_export",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    Video,
    /// Imported audio (music beds, Sonus tracks) — never produced upstream.
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub kind: AssetKind,
    /// Absolute path on the node filesystem (if downloaded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// Node-relative HTTP path when served (`/v1/library/{id}/content`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    /// Ephemeral upstream URL (may expire).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_ticks: Option<i64>,
}

/// Stable agent-facing response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub ok: bool,
    pub job_id: JobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_request_id: Option<String>,
    pub status: JobStatus,
    pub mode: JobMode,
    pub model: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

impl JobResult {
    /// A freshly-accepted job that hasn't produced anything yet (the async
    /// craft flow returns this row immediately; the render task finalizes it).
    pub fn pending(
        job_id: JobId,
        mode: JobMode,
        model: impl Into<String>,
        prompt: Option<String>,
    ) -> Self {
        Self {
            ok: true,
            job_id,
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

    pub fn failure(
        job_id: JobId,
        mode: JobMode,
        model: impl Into<String>,
        error_type: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            job_id,
            upstream_request_id: None,
            status: JobStatus::Failed,
            mode,
            model: model.into(),
            assets: vec![],
            usage: None,
            error: Some(message.into()),
            error_type: Some(error_type.into()),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            prompt: None,
        }
    }
}

/// Media input reference accepted by the core.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MediaRef {
    Url {
        url: String,
    },
    FileId {
        file_id: String,
    },
    /// Local filesystem path (read + encode by the node).
    Path {
        path: String,
    },
}

impl MediaRef {
    pub fn from_user_input(s: &str) -> Self {
        let t = s.trim();
        if t.starts_with("file_") {
            Self::FileId {
                file_id: t.to_string(),
            }
        } else if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("data:") {
            Self::Url { url: t.to_string() }
        } else {
            Self::Path {
                path: t.to_string(),
            }
        }
    }

    /// Parse a media ref from a NETWORK or agent caller.
    ///
    /// Unlike `from_user_input`, this never yields a local-filesystem `Path`.
    /// Accepting a bare path from a remote / token-authenticated caller would let it
    /// read arbitrary node-local files (which the client then base64-encodes and
    /// ships upstream) — the highest-severity finding of the 2026-07-28 audit. Bare
    /// paths are rejected; callers must pass a `data:` URL, an `http(s):` URL, or an
    /// xAI `file_…` id. Keep `from_user_input` (which accepts local paths) for the
    /// local CLI only.
    pub fn from_remote_input(s: &str) -> Result<Self> {
        let t = s.trim();
        if t.is_empty() {
            return Err(Error::forbidden("empty media reference"));
        }
        if t.starts_with("file_") {
            Ok(Self::FileId {
                file_id: t.to_string(),
            })
        } else if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("data:") {
            Ok(Self::Url { url: t.to_string() })
        } else {
            Err(Error::forbidden(format!(
                "local filesystem paths are not accepted from remote callers; \
                 use library:{{job_id}} for a node-library asset, a data: URL, \
                 an http(s) URL, or an xAI file_… id (got: {})",
                t.chars().take(40).collect::<String>()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_input_rejects_local_paths() {
        assert!(MediaRef::from_remote_input("/etc/shadow").is_err());
        assert!(MediaRef::from_remote_input("relative/thing.png").is_err());
        assert!(MediaRef::from_remote_input("thing.png").is_err());
        assert!(MediaRef::from_remote_input("   ").is_err());
    }

    #[test]
    fn remote_input_allows_url_data_fileid() {
        assert!(matches!(
            MediaRef::from_remote_input("https://x/y.png"),
            Ok(MediaRef::Url { .. })
        ));
        assert!(matches!(
            MediaRef::from_remote_input("data:image/png;base64,AAAA"),
            Ok(MediaRef::Url { .. })
        ));
        assert!(matches!(
            MediaRef::from_remote_input("file_abc123"),
            Ok(MediaRef::FileId { .. })
        ));
    }

    #[test]
    fn user_input_still_accepts_local_paths() {
        assert!(matches!(
            MediaRef::from_user_input("/tmp/a.png"),
            MediaRef::Path { .. }
        ));
    }
}
