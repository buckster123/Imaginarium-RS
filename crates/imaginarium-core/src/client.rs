//! xAI Imagine HTTP client (Phase 1: images; Phase 2: video).

use std::path::Path;

use base64::Engine as _;
use chrono::Utc;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::estimate;
use crate::jobs::{self, JobStore};
use crate::library::{self, Library};
use crate::models::{self, ModelId};
use crate::types::*;

const USER_AGENT_VALUE: &str = concat!("Imaginarium-RS/", env!("CARGO_PKG_VERSION"));

#[derive(Clone)]
pub struct ImagineClient {
    http: Client,
    base_url: String,
    api_key: String,
    auto_download: bool,
    storage_profile: String,
    storage_public_url: bool,
}

#[derive(Debug, Clone)]
pub struct ImageGenerateRequest {
    pub prompt: String,
    pub model: ModelId,
    pub n: u32,
    pub aspect_ratio: Option<String>,
    pub resolution: Option<String>,
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone)]
pub struct ImageEditRequest {
    pub prompt: String,
    pub model: ModelId,
    pub images: Vec<MediaRef>,
    pub n: u32,
    pub aspect_ratio: Option<String>,
    pub resolution: Option<String>,
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    Url,
    B64Json,
}

impl ResponseFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::B64Json => "b64_json",
        }
    }
}

impl ImagineClient {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let api_key = cfg.resolve_api_key()?;
        let http = Client::builder()
            .user_agent(USER_AGENT_VALUE)
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        Ok(Self {
            http,
            base_url: cfg.base_url().to_string(),
            api_key,
            auto_download: cfg.storage.auto_download,
            storage_profile: cfg.storage.profile.clone(),
            storage_public_url: cfg.storage.public_url,
        })
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            format!("Bearer {}", self.api_key)
                .parse()
                .expect("api key header"),
        );
        h.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        h.insert(USER_AGENT, USER_AGENT_VALUE.parse().unwrap());
        h
    }

    fn storage_options_json(&self, filename: &str) -> Option<Value> {
        if self.storage_profile != "xai_files" {
            return None;
        }
        let mut obj = json!({ "filename": filename });
        if self.storage_public_url {
            obj.as_object_mut()
                .unwrap()
                .insert("public_url".into(), json!(true));
        }
        Some(obj)
    }

    async fn media_ref_to_image_field(&self, media: &MediaRef) -> Result<Value> {
        match media {
            MediaRef::Url { url } => Ok(json!({ "url": url, "type": "image_url" })),
            MediaRef::FileId { file_id } => Ok(json!({ "file_id": file_id })),
            MediaRef::Path { path } => {
                let p = Path::new(path);
                let raw = std::fs::read(p).map_err(|e| {
                    Error::other(format!("failed to read image {}: {e}", p.display()))
                })?;
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("png")
                    .to_ascii_lowercase();
                let mime = match ext.as_str() {
                    "jpg" | "jpeg" => "image/jpeg",
                    "webp" => "image/webp",
                    "gif" => "image/gif",
                    _ => "image/png",
                };
                let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
                Ok(json!({
                    "url": format!("data:{mime};base64,{b64}"),
                    "type": "image_url"
                }))
            }
        }
    }

    pub async fn image_generate(
        &self,
        req: ImageGenerateRequest,
        library: &Library,
        store: Option<&JobStore>,
    ) -> Result<JobResult> {
        let mut job = jobs::pending_job(
            JobMode::ImageGenerate,
            req.model.as_str(),
            Some(req.prompt.clone()),
        );
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let cost = estimate::estimate_image(req.model, req.n);
        let mut payload = json!({
            "model": req.model.as_str(),
            "prompt": req.prompt,
            "n": req.n.max(1),
            "response_format": req.response_format.as_str(),
        });
        if let Some(ar) = &req.aspect_ratio {
            payload
                .as_object_mut()
                .unwrap()
                .insert("aspect_ratio".into(), json!(ar));
        }
        if let Some(res) = &req.resolution {
            payload
                .as_object_mut()
                .unwrap()
                .insert("resolution".into(), json!(res));
        }
        if let Some(storage) = self.storage_options_json("imaginarium-image.png") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let url = format!("{}/images/generations", self.base_url);
        debug!(%url, "POST image generate");
        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers())
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        let body_text = resp.text().await?;
        if !status.is_success() {
            let err = Error::Upstream {
                status: status.as_u16(),
                body: body_text.chars().take(2000).collect(),
            };
            job.ok = false;
            job.status = JobStatus::Failed;
            job.error = Some(err.to_string());
            job.error_type = Some("upstream".into());
            job.completed_at = Some(Utc::now());
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            return Err(err);
        }

        let parsed: ImageApiResponse = serde_json::from_str(&body_text)?;
        let job_dir = library.ensure_job_dir(&job.job_id)?;
        library.write_prompt(&job_dir, &req.prompt)?;

        let mut assets = Vec::new();
        for (i, item) in parsed.data.iter().enumerate() {
            let asset_id = AssetId::new();
            let mut local_path = None;
            let mut upstream_url = item.url.clone();

            if self.auto_download {
                if let Some(url) = &item.url {
                    let dest = job_dir.join(format!("{:02}.png", i));
                    if let Err(e) = library::download_url(&self.http, url, &dest).await {
                        debug!("download failed: {e}");
                    } else {
                        local_path = Some(dest.display().to_string());
                    }
                } else if let Some(b64) = &item.b64_json {
                    let dest = job_dir.join(format!("{:02}.png", i));
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                        std::fs::write(&dest, bytes)?;
                        local_path = Some(dest.display().to_string());
                    }
                }
            }

            let file_output = item.file_output.as_ref();
            assets.push(Asset {
                id: asset_id,
                kind: AssetKind::Image,
                local_path,
                content_url: None,
                upstream_url: upstream_url.take(),
                file_id: file_output.and_then(|f| f.file_id.clone()),
                public_url: file_output.and_then(|f| f.public_url.clone()),
                mime_type: item.mime_type.clone(),
            });
        }

        job.ok = true;
        job.status = JobStatus::Done;
        job.assets = assets;
        job.usage = Some(UsageInfo {
            estimated_usd: Some(cost.estimated_usd),
            upstream_ticks: parsed.usage.as_ref().and_then(|u| u.cost_in_usd_ticks),
        });
        job.completed_at = Some(Utc::now());
        library.write_meta(&job_dir, &job)?;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }
        info!(job_id = %job.job_id, n = job.assets.len(), "image generate done");
        Ok(job)
    }

    pub async fn image_edit(
        &self,
        req: ImageEditRequest,
        library: &Library,
        store: Option<&JobStore>,
    ) -> Result<JobResult> {
        if req.images.is_empty() {
            return Err(Error::invalid_mode(
                "image edit requires at least one image",
            ));
        }
        if req.images.len() > 3 {
            return Err(Error::invalid_mode(
                "image edit supports at most 3 source images",
            ));
        }

        let mut job = jobs::pending_job(
            JobMode::ImageEdit,
            req.model.as_str(),
            Some(req.prompt.clone()),
        );
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let cost = estimate::estimate_image(req.model, req.n);
        let mut payload = json!({
            "model": req.model.as_str(),
            "prompt": req.prompt,
            "n": req.n.max(1),
            "response_format": req.response_format.as_str(),
        });

        if req.images.len() == 1 {
            let field = self.media_ref_to_image_field(&req.images[0]).await?;
            payload
                .as_object_mut()
                .unwrap()
                .insert("image".into(), field);
        } else {
            let mut arr = Vec::new();
            for img in &req.images {
                arr.push(self.media_ref_to_image_field(img).await?);
            }
            payload
                .as_object_mut()
                .unwrap()
                .insert("images".into(), Value::Array(arr));
        }

        if let Some(ar) = &req.aspect_ratio {
            payload
                .as_object_mut()
                .unwrap()
                .insert("aspect_ratio".into(), json!(ar));
        }
        if let Some(res) = &req.resolution {
            payload
                .as_object_mut()
                .unwrap()
                .insert("resolution".into(), json!(res));
        }
        if let Some(storage) = self.storage_options_json("imaginarium-edit.png") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let url = format!("{}/images/edits", self.base_url);
        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers())
            .json(&payload)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        if !status.is_success() {
            let err = Error::Upstream {
                status: status.as_u16(),
                body: body_text.chars().take(2000).collect(),
            };
            job.ok = false;
            job.status = JobStatus::Failed;
            job.error = Some(err.to_string());
            job.error_type = Some("upstream".into());
            job.completed_at = Some(Utc::now());
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            return Err(err);
        }

        let parsed: ImageApiResponse = serde_json::from_str(&body_text)?;
        let job_dir = library.ensure_job_dir(&job.job_id)?;
        library.write_prompt(&job_dir, &req.prompt)?;

        let mut assets = Vec::new();
        for (i, item) in parsed.data.iter().enumerate() {
            let asset_id = AssetId::new();
            let mut local_path = None;
            if self.auto_download {
                if let Some(u) = &item.url {
                    let dest = job_dir.join(format!("{:02}.png", i));
                    if library::download_url(&self.http, u, &dest).await.is_ok() {
                        local_path = Some(dest.display().to_string());
                    }
                } else if let Some(b64) = &item.b64_json {
                    let dest = job_dir.join(format!("{:02}.png", i));
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                        std::fs::write(&dest, bytes)?;
                        local_path = Some(dest.display().to_string());
                    }
                }
            }
            let file_output = item.file_output.as_ref();
            assets.push(Asset {
                id: asset_id,
                kind: AssetKind::Image,
                local_path,
                content_url: None,
                upstream_url: item.url.clone(),
                file_id: file_output.and_then(|f| f.file_id.clone()),
                public_url: file_output.and_then(|f| f.public_url.clone()),
                mime_type: item.mime_type.clone(),
            });
        }

        job.ok = true;
        job.status = JobStatus::Done;
        job.assets = assets;
        job.usage = Some(UsageInfo {
            estimated_usd: Some(cost.estimated_usd),
            upstream_ticks: parsed.usage.as_ref().and_then(|u| u.cost_in_usd_ticks),
        });
        job.completed_at = Some(Utc::now());
        library.write_meta(&job_dir, &job)?;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }
        Ok(job)
    }
}

#[derive(Debug, Deserialize)]
struct ImageApiResponse {
    #[serde(default)]
    data: Vec<ImageApiItem>,
    usage: Option<ImageUsage>,
}

#[derive(Debug, Deserialize)]
struct ImageApiItem {
    url: Option<String>,
    b64_json: Option<String>,
    mime_type: Option<String>,
    file_output: Option<FileOutput>,
}

#[derive(Debug, Deserialize)]
struct FileOutput {
    file_id: Option<String>,
    public_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImageUsage {
    cost_in_usd_ticks: Option<i64>,
}

/// Print-friendly catalog for CLI (re-export helper).
pub fn models_table_json() -> Value {
    json!({
        "product": crate::PRODUCT,
        "version": crate::VERSION,
        "models": models::catalog(),
        "image_aspect_ratios": models::IMAGE_ASPECT_RATIOS,
        "video_aspect_ratios": models::VIDEO_ASPECT_RATIOS,
        "image_resolutions": models::IMAGE_RESOLUTIONS,
        "video_resolutions": models::VIDEO_RESOLUTIONS,
    })
}
