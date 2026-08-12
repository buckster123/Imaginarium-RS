//! xAI Imagine HTTP client (images + video).

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use chrono::Utc;
use reqwest::header::{HeaderMap, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, info};
use ulid::Ulid;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::estimate;
use crate::jobs::{self, JobStore};
use crate::library::{self, Library};
use crate::models::{
    self, validate_image_quality, validate_video_generate, ImageQuality, ModelId, VideoMode,
};
use crate::rate_limit::RateLimiter;
use crate::types::*;

const USER_AGENT_VALUE: &str = concat!("Imaginarium-RS/", env!("CARGO_PKG_VERSION"));

/// Persist Imagine `b64_json` bytes. Always done when present — auto_download
/// only gates fetching ephemeral URLs, not bytes we already hold.
fn write_b64_image(dest: &Path, b64: &str) -> Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| Error::other(format!("b64_json decode: {e}")))?;
    if bytes.is_empty() {
        return Err(Error::other("b64_json decoded to empty"));
    }
    std::fs::write(dest, bytes)?;
    Ok(())
}

#[derive(Clone)]
pub struct ImagineClient {
    pub(crate) http: Client,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) auto_download: bool,
    pub(crate) storage_profile: String,
    pub(crate) storage_public_url: bool,
    pub(crate) poll_interval: Duration,
    pub(crate) poll_timeout: Duration,
    pub(crate) limits: crate::config::LimitsConfig,
    /// In-process CLI / MCP bucket (`local`). `None` on the HTTP server
    /// (per-token limiter lives in auth middleware).
    pub(crate) local_rate: Option<RateLimiter>,
}

#[derive(Debug, Clone)]
pub struct ImageGenerateRequest {
    pub prompt: String,
    pub model: ModelId,
    pub n: u32,
    pub aspect_ratio: Option<String>,
    pub resolution: Option<String>,
    /// Image 2.0 only (`low` | `medium`). Omitted → upstream default `medium`.
    pub quality: Option<ImageQuality>,
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
    /// Image 2.0 only (`low` | `medium`). Omitted → upstream default `medium`.
    pub quality: Option<ImageQuality>,
    pub response_format: ResponseFormat,
}

#[derive(Debug, Clone)]
pub struct VideoGenerateRequest {
    pub prompt: Option<String>,
    /// If None, auto-selected (generate modes → video-1.5).
    pub model: Option<ModelId>,
    /// When true, `model` was user-forced (no auto-swap).
    pub explicit_model: bool,
    pub image: Option<MediaRef>,
    pub reference_images: Vec<MediaRef>,
    /// Preset `voice_id`s for Video 1.5 R2V (`reference_audios`). Max 3.
    pub reference_audios: Vec<String>,
    pub duration: Option<u32>,
    pub aspect_ratio: Option<String>,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VideoEditRequest {
    pub prompt: String,
    pub video: MediaRef,
    pub model: Option<ModelId>,
}

#[derive(Debug, Clone)]
pub struct VideoExtendRequest {
    pub prompt: String,
    pub video: MediaRef,
    /// Extension segment length only (2–10, default 6).
    pub duration: Option<u32>,
    pub model: Option<ModelId>,
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
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self {
            http,
            base_url: cfg.base_url().to_string(),
            api_key,
            auto_download: cfg.storage.auto_download,
            storage_profile: cfg.storage.profile.clone(),
            storage_public_url: cfg.storage.public_url,
            poll_interval: Duration::from_millis(cfg.poll.interval_ms.max(500)),
            poll_timeout: Duration::from_secs(cfg.poll.timeout_s.max(30)),
            limits: cfg.limits.clone(),
            local_rate: RateLimiter::from_limits(&cfg.limits),
        })
    }

    /// Server process: LAN tokens are throttled in auth, not here.
    pub fn without_local_rate(mut self) -> Self {
        self.local_rate = None;
        self
    }

    fn check_rate(&self) -> Result<()> {
        if let Some(lim) = &self.local_rate {
            lim.check("local")?;
        }
        Ok(())
    }

    fn check_spend(&self, store: Option<&JobStore>, this_job_usd: f64) -> Result<()> {
        let spent_today = if self.limits.max_usd_per_day.is_some() {
            if let Some(s) = store {
                let start = Utc::now()
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .map(|t| t.and_utc())
                    .unwrap_or_else(Utc::now);
                s.estimated_spend_since(start)?
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.limits.check(this_job_usd, spent_today)
    }

    pub(crate) fn auth_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
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

    pub(crate) fn storage_options_json(&self, filename: &str) -> Option<Value> {
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

    pub(crate) async fn media_ref_to_image_field(&self, media: &MediaRef) -> Result<Value> {
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

    pub(crate) async fn media_ref_to_video_field(&self, media: &MediaRef) -> Result<Value> {
        match media {
            MediaRef::Url { url } => Ok(json!({ "url": url })),
            MediaRef::FileId { file_id } => Ok(json!({ "file_id": file_id })),
            MediaRef::Path { path } => {
                let p = Path::new(path);
                let raw = std::fs::read(p).map_err(|e| {
                    Error::other(format!("failed to read video {}: {e}", p.display()))
                })?;
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("mp4")
                    .to_ascii_lowercase();
                let mime = match ext.as_str() {
                    "webm" => "video/webm",
                    "mov" => "video/quicktime",
                    _ => "video/mp4",
                };
                let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
                Ok(json!({ "url": format!("data:{mime};base64,{b64}") }))
            }
        }
    }

    pub async fn image_generate(
        &self,
        req: ImageGenerateRequest,
        library: &Library,
        store: Option<&JobStore>,
    ) -> Result<JobResult> {
        validate_image_quality(req.model, req.quality)?;
        let cost = estimate::estimate_image(req.model, req.n);
        self.check_rate()?;
        self.check_spend(store, cost.estimated_usd)?;
        let mut job = jobs::pending_job(
            JobMode::ImageGenerate,
            req.model.as_str(),
            Some(req.prompt.clone()),
        );
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

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
        if let Some(q) = req.quality {
            payload
                .as_object_mut()
                .unwrap()
                .insert("quality".into(), json!(q.as_str()));
        }
        if let Some(storage) = self.storage_options_json("imaginarium-image.png") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let url = format!("{}/images/generations", self.base_url);
        debug!(%url, "POST image generate");
        let body_text = self
            .post_image_json(&url, &payload, job.clone(), store)
            .await?;

        let job = self
            .finalize_image_job(
                job,
                &body_text,
                library,
                store,
                &req.prompt,
                cost.estimated_usd,
            )
            .await?;
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
        let max_src = models::get(req.model)
            .capabilities
            .max_source_images
            .unwrap_or(3);
        if req.images.len() as u32 > max_src {
            return Err(Error::invalid_mode(format!(
                "image edit supports at most {max_src} source images"
            )));
        }
        validate_image_quality(req.model, req.quality)?;
        let cost = estimate::estimate_image(req.model, req.n);
        self.check_rate()?;
        self.check_spend(store, cost.estimated_usd)?;

        let mut job = jobs::pending_job(
            JobMode::ImageEdit,
            req.model.as_str(),
            Some(req.prompt.clone()),
        );
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let mut payload = json!({
            "model": req.model.as_str(),
            "prompt": req.prompt,
            "n": req.n.max(1),
            "response_format": req.response_format.as_str(),
        });

        if req.images.len() == 1 {
            let field = match self.media_ref_to_image_field(&req.images[0]).await {
                Ok(f) => f,
                Err(e) => return self.fail_job_err(job, store, e),
            };
            payload
                .as_object_mut()
                .unwrap()
                .insert("image".into(), field);
        } else {
            let mut arr = Vec::new();
            for img in &req.images {
                match self.media_ref_to_image_field(img).await {
                    Ok(f) => arr.push(f),
                    Err(e) => return self.fail_job_err(job, store, e),
                }
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
        if let Some(q) = req.quality {
            payload
                .as_object_mut()
                .unwrap()
                .insert("quality".into(), json!(q.as_str()));
        }
        if let Some(storage) = self.storage_options_json("imaginarium-edit.png") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let url = format!("{}/images/edits", self.base_url);
        let body_text = self
            .post_image_json(&url, &payload, job.clone(), store)
            .await?;

        self.finalize_image_job(
            job,
            &body_text,
            library,
            store,
            &req.prompt,
            cost.estimated_usd,
        )
        .await
    }

    // ── Video ──────────────────────────────────────────────────────────

    pub async fn video_generate(
        &self,
        req: VideoGenerateRequest,
        library: &Library,
        store: Option<&JobStore>,
        wait: bool,
    ) -> Result<JobResult> {
        if req.image.is_some()
            && (!req.reference_images.is_empty() || !req.reference_audios.is_empty())
        {
            return Err(Error::invalid_mode(
                "cannot combine image (I2V) with reference_images / reference_audios (R2V)",
            ));
        }

        let mode = if req.image.is_some() {
            VideoMode::ImageToVideo
        } else if !req.reference_images.is_empty() || !req.reference_audios.is_empty() {
            VideoMode::ReferenceToVideo
        } else {
            VideoMode::TextToVideo
        };

        let model = if req.explicit_model {
            req.model
                .unwrap_or_else(|| models::default_video_model_for(mode))
        } else {
            req.model
                .filter(|m| {
                    // Allow explicit-looking catalog pick only when valid; else auto.
                    validate_video_generate(
                        *m,
                        mode,
                        req.resolution.as_deref(),
                        req.duration,
                        req.reference_images.len(),
                        req.reference_audios.len(),
                    )
                    .is_ok()
                })
                .unwrap_or_else(|| models::default_video_model_for(mode))
        };

        if mode == VideoMode::TextToVideo
            && req
                .prompt
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(Error::invalid_mode(
                "text-to-video requires a non-empty prompt",
            ));
        }
        if mode == VideoMode::ReferenceToVideo
            && req
                .prompt
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(Error::invalid_mode(
                "reference-to-video requires a non-empty prompt",
            ));
        }

        validate_video_generate(
            model,
            mode,
            req.resolution.as_deref(),
            req.duration,
            req.reference_images.len(),
            req.reference_audios.len(),
        )?;

        let duration = req.duration.unwrap_or(8).clamp(1, 15);
        let cost = estimate::estimate_video(model, duration, req.resolution.as_deref());
        self.check_rate()?;
        self.check_spend(store, cost.estimated_usd)?;
        let prompt = req.prompt.clone().unwrap_or_default();
        let mut job =
            jobs::pending_job(JobMode::VideoGenerate, model.as_str(), Some(prompt.clone()));
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let mut payload = json!({
            "model": model.as_str(),
            "duration": duration,
        });
        if !prompt.trim().is_empty() {
            payload
                .as_object_mut()
                .unwrap()
                .insert("prompt".into(), json!(prompt));
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
        if let Some(img) = &req.image {
            let field = self.media_ref_to_image_field(img).await?;
            // generations accepts image.url / image.file_id (type optional)
            payload
                .as_object_mut()
                .unwrap()
                .insert("image".into(), field);
        }
        if !req.reference_images.is_empty() {
            let mut refs = Vec::new();
            for r in &req.reference_images {
                refs.push(self.media_ref_to_image_field(r).await?);
            }
            payload
                .as_object_mut()
                .unwrap()
                .insert("reference_images".into(), Value::Array(refs));
        }
        if !req.reference_audios.is_empty() {
            let audios: Vec<Value> = req
                .reference_audios
                .iter()
                .map(|id| json!({ "voice_id": id }))
                .collect();
            payload
                .as_object_mut()
                .unwrap()
                .insert("reference_audios".into(), Value::Array(audios));
        }
        if let Some(storage) = self.storage_options_json("imaginarium-video.mp4") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let request_id = self
            .submit_video("generations", payload)
            .await
            .map_err(|e| {
                let _ = self.mark_failed(&job, store, &e);
                e
            })?;
        job.upstream_request_id = Some(request_id.clone());
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        if wait {
            self.finish_video_job(job, &request_id, library, store, Some(cost.estimated_usd))
                .await
        } else {
            job.status = JobStatus::Pending;
            job.usage = Some(UsageInfo {
                estimated_usd: Some(cost.estimated_usd),
                upstream_ticks: None,
            });
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            Ok(job)
        }
    }

    pub async fn video_edit(
        &self,
        req: VideoEditRequest,
        library: &Library,
        store: Option<&JobStore>,
        wait: bool,
    ) -> Result<JobResult> {
        let model = req.model.unwrap_or(ModelId::Video);
        if !models::get(model).capabilities.video_edit {
            return Err(Error::invalid_mode(format!(
                "{} does not support video edit",
                model.as_str()
            )));
        }
        // Edit duration unknown; rough mid estimate 6s. No res on the request — assume 720p.
        let cost = estimate::estimate_video(model, 6, Some("720p"));
        self.check_rate()?;
        self.check_spend(store, cost.estimated_usd)?;

        let mut job =
            jobs::pending_job(JobMode::VideoEdit, model.as_str(), Some(req.prompt.clone()));
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let video_field = self.media_ref_to_video_field(&req.video).await?;
        let mut payload = json!({
            "model": model.as_str(),
            "prompt": req.prompt,
            "video": video_field,
        });
        if let Some(storage) = self.storage_options_json("imaginarium-video-edit.mp4") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let request_id = self.submit_video("edits", payload).await.map_err(|e| {
            let _ = self.mark_failed(&job, store, &e);
            e
        })?;
        job.upstream_request_id = Some(request_id.clone());
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        if wait {
            self.finish_video_job(job, &request_id, library, store, Some(cost.estimated_usd))
                .await
        } else {
            job.status = JobStatus::Pending;
            job.usage = Some(UsageInfo {
                estimated_usd: Some(cost.estimated_usd),
                upstream_ticks: None,
            });
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            Ok(job)
        }
    }

    pub async fn video_extend(
        &self,
        req: VideoExtendRequest,
        library: &Library,
        store: Option<&JobStore>,
        wait: bool,
    ) -> Result<JobResult> {
        let model = req.model.unwrap_or(ModelId::Video);
        if !models::get(model).capabilities.video_extend {
            return Err(Error::invalid_mode(format!(
                "{} does not support video extend",
                model.as_str()
            )));
        }
        let duration = models::parse_video_extend_duration(req.duration)?;
        let cost = estimate::estimate_video(model, duration, Some("720p"));
        self.check_rate()?;
        self.check_spend(store, cost.estimated_usd)?;

        let mut job = jobs::pending_job(
            JobMode::VideoExtend,
            model.as_str(),
            Some(req.prompt.clone()),
        );
        job.status = JobStatus::Running;
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        let video_field = self.media_ref_to_video_field(&req.video).await?;
        let mut payload = json!({
            "model": model.as_str(),
            "prompt": req.prompt,
            "video": video_field,
            "duration": duration,
        });
        if let Some(storage) = self.storage_options_json("imaginarium-video-extend.mp4") {
            payload
                .as_object_mut()
                .unwrap()
                .insert("storage_options".into(), storage);
        }

        let request_id = self
            .submit_video("extensions", payload)
            .await
            .map_err(|e| {
                let _ = self.mark_failed(&job, store, &e);
                e
            })?;
        job.upstream_request_id = Some(request_id.clone());
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }

        if wait {
            self.finish_video_job(job, &request_id, library, store, Some(cost.estimated_usd))
                .await
        } else {
            job.status = JobStatus::Pending;
            job.usage = Some(UsageInfo {
                estimated_usd: Some(cost.estimated_usd),
                upstream_ticks: None,
            });
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            Ok(job)
        }
    }

    /// Poll upstream by request_id and update the local job (by job_id lookup).
    pub async fn video_wait(
        &self,
        job_id: &JobId,
        library: &Library,
        store: &JobStore,
    ) -> Result<JobResult> {
        let mut job = store
            .get(job_id)?
            .ok_or_else(|| Error::JobNotFound(job_id.to_string()))?;
        let request_id = job
            .upstream_request_id
            .clone()
            .ok_or_else(|| Error::other("job has no upstream_request_id to poll"))?;
        if matches!(
            job.status,
            JobStatus::Done | JobStatus::Failed | JobStatus::Expired
        ) {
            return Ok(job);
        }
        job.status = JobStatus::Running;
        store.upsert_result(&job)?;
        let est = job.usage.as_ref().and_then(|u| u.estimated_usd);
        self.finish_video_job(job, &request_id, library, Some(store), est)
            .await
    }

    /// Refresh status once without blocking full timeout.
    pub async fn video_status_once(
        &self,
        job_id: &JobId,
        library: &Library,
        store: &JobStore,
    ) -> Result<JobResult> {
        let mut job = store
            .get(job_id)?
            .ok_or_else(|| Error::JobNotFound(job_id.to_string()))?;
        let Some(request_id) = job.upstream_request_id.clone() else {
            return Ok(job);
        };
        if matches!(
            job.status,
            JobStatus::Done | JobStatus::Failed | JobStatus::Expired
        ) {
            return Ok(job);
        }

        let body = self.poll_video_once(&request_id).await?;
        let status = body
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending")
            .to_ascii_lowercase();

        match status.as_str() {
            "done" => {
                let est = job.usage.as_ref().and_then(|u| u.estimated_usd);
                self.complete_video_from_body(job, &body, library, Some(store), est)
                    .await
            }
            "failed" | "error" => {
                job.ok = false;
                job.status = JobStatus::Failed;
                job.error = Some(body.to_string().chars().take(2000).collect());
                job.error_type = Some("upstream".into());
                job.completed_at = Some(Utc::now());
                store.upsert_result(&job)?;
                Ok(job)
            }
            "expired" | "cancelled" => {
                job.ok = false;
                job.status = if status == "expired" {
                    JobStatus::Expired
                } else {
                    JobStatus::Cancelled
                };
                job.error = Some(format!("upstream status: {status}"));
                job.completed_at = Some(Utc::now());
                store.upsert_result(&job)?;
                Ok(job)
            }
            _ => {
                job.status = JobStatus::Pending;
                store.upsert_result(&job)?;
                Ok(job)
            }
        }
    }

    // ── internals ──────────────────────────────────────────────────────

    async fn submit_video(&self, endpoint: &str, payload: Value) -> Result<String> {
        let url = format!("{}/videos/{endpoint}", self.base_url);
        debug!(%url, "POST video {endpoint}");
        let mut headers = self.auth_headers();
        headers.insert(
            "x-idempotency-key",
            Ulid::new().to_string().parse().unwrap(),
        );
        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Upstream {
                status: status.as_u16(),
                body: body_text.chars().take(2000).collect(),
            });
        }
        let body: Value = serde_json::from_str(&body_text)?;
        body.get("request_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| Error::other("video response missing request_id"))
    }

    async fn poll_video_once(&self, request_id: &str) -> Result<Value> {
        let url = format!("{}/videos/{request_id}", self.base_url);
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Upstream {
                status: status.as_u16(),
                body: body_text.chars().take(2000).collect(),
            });
        }
        Ok(serde_json::from_str(&body_text)?)
    }

    async fn finish_video_job(
        &self,
        job: JobResult,
        request_id: &str,
        library: &Library,
        store: Option<&JobStore>,
        estimated_usd: Option<f64>,
    ) -> Result<JobResult> {
        let deadline = std::time::Instant::now() + self.poll_timeout;
        #[allow(unused_assignments)]
        let mut last_status = String::from("pending");
        loop {
            let body = self.poll_video_once(request_id).await?;
            last_status = body
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending")
                .to_ascii_lowercase();
            debug!(%request_id, status = %last_status, "video poll");

            match last_status.as_str() {
                "done" => {
                    return self
                        .complete_video_from_body(job, &body, library, store, estimated_usd)
                        .await;
                }
                "failed" | "error" => {
                    let err = Error::Upstream {
                        status: 500,
                        body: body.to_string().chars().take(2000).collect(),
                    };
                    return self.fail_job_err(job, store, err);
                }
                "expired" | "cancelled" => {
                    let mut job = job;
                    job.ok = false;
                    job.status = if last_status == "expired" {
                        JobStatus::Expired
                    } else {
                        JobStatus::Cancelled
                    };
                    job.error = Some(format!("upstream status: {last_status}"));
                    job.completed_at = Some(Utc::now());
                    if let Some(s) = store {
                        s.upsert_result(&job)?;
                    }
                    return Ok(job);
                }
                _ => {
                    // still pending — fall through to timeout check / sleep
                }
            }

            if std::time::Instant::now() >= deadline {
                // The wait timed out, but the upstream job may still be running — do
                // NOT mark it terminally Failed. That would seal off recovery for a
                // job we already paid for (video_wait/video_status_once early-return
                // on terminal states). Leave it non-terminal (Running) and report the
                // timeout on the envelope so the caller re-polls.
                let mut job = job;
                job.ok = false;
                job.status = JobStatus::Running;
                job.error = Some(format!(
                    "poll timeout after {}s (last upstream status: {last_status}); \
                     job may still be running — re-poll with job_wait / job_status",
                    self.poll_timeout.as_secs()
                ));
                job.error_type = Some("timeout".into());
                job.completed_at = None;
                if let Some(s) = store {
                    s.upsert_result(&job)?;
                }
                return Ok(job);
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn complete_video_from_body(
        &self,
        mut job: JobResult,
        body: &Value,
        library: &Library,
        store: Option<&JobStore>,
        estimated_usd: Option<f64>,
    ) -> Result<JobResult> {
        let video = body.get("video").cloned().unwrap_or(Value::Null);
        let temporary_url = video
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let file_output = video.get("file_output");
        let public_url = file_output
            .and_then(|f| f.get("public_url"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let file_id = file_output
            .and_then(|f| f.get("file_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let duration = video
            .get("duration")
            .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)));

        // A 2xx "done" carrying no url / file_output is not a usable result — don't
        // record an empty ok=true job. Surface it as a parse failure with a snippet.
        if temporary_url.is_none() && public_url.is_none() && file_id.is_none() {
            let snippet = body.to_string().chars().take(400).collect::<String>();
            job.ok = false;
            job.status = JobStatus::Failed;
            job.error = Some(format!(
                "upstream reported done but returned no video asset: {snippet}"
            ));
            job.error_type = Some("parse".into());
            job.completed_at = Some(Utc::now());
            if let Some(s) = store {
                s.upsert_result(&job)?;
            }
            return Err(Error::other(
                "upstream reported done but returned no video asset",
            ));
        }

        let job_dir = match library.ensure_job_dir(&job.job_id) {
            Ok(d) => d,
            Err(e) => return self.fail_job_err(job, store, e),
        };
        if let Some(p) = &job.prompt {
            if let Err(e) = library.write_prompt(&job_dir, p) {
                return self.fail_job_err(job, store, e);
            }
        }

        let download_url = public_url.clone().or_else(|| temporary_url.clone());
        let mut local_path = None;
        if self.auto_download {
            if let Some(url) = &download_url {
                let dest = job_dir.join("00.mp4");
                match library::download_url(&self.http, url, &dest).await {
                    Ok(_) => local_path = Some(dest.display().to_string()),
                    Err(e) => tracing::warn!(
                        job_id = %job.job_id,
                        "video download failed; asset kept as upstream url only: {e}"
                    ),
                }
            }
        }

        let content_url = local_path
            .as_ref()
            .map(|_| library::Library::content_url(&job.job_id, 0));
        let asset = Asset {
            id: AssetId::new(),
            kind: AssetKind::Video,
            local_path,
            content_url,
            upstream_url: temporary_url,
            file_id,
            public_url,
            mime_type: Some("video/mp4".into()),
        };

        if self.auto_download && asset.local_path.is_none() {
            job.ok = false;
            job.error = Some("auto_download failed; asset kept as upstream url only".into());
            job.error_type = Some("download".into());
        } else {
            job.ok = true;
        }
        job.status = JobStatus::Done;
        job.assets = vec![asset];
        job.usage = Some(UsageInfo {
            estimated_usd: estimated_usd.or_else(|| {
                duration.map(|d| {
                    estimate::estimate_video(
                        ModelId::parse(&job.model).unwrap_or(ModelId::Video),
                        d.ceil() as u32,
                        None,
                    )
                    .estimated_usd
                })
            }),
            upstream_ticks: None,
        });
        job.completed_at = Some(Utc::now());
        if let Err(e) = library.write_meta(&job_dir, &job) {
            tracing::warn!(job_id = %job.job_id, "write_meta failed (non-fatal): {e}");
        }
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }
        info!(job_id = %job.job_id, "video job done");
        Ok(job)
    }

    /// Finalize an image job after a 2xx upstream response.
    ///
    /// Runs the fallible post-response steps (parse, dir, prompt, per-asset
    /// download) in a guarded block: on ANY error the job is marked Failed rather
    /// than left stranded in `running` (the pre-response state). A `done` response
    /// carrying no image data is treated as a parse failure, not a silent empty
    /// success. `write_meta` is best-effort (auxiliary metadata never fails a job
    /// whose assets already materialized).
    async fn finalize_image_job(
        &self,
        mut job: JobResult,
        body_text: &str,
        library: &Library,
        store: Option<&JobStore>,
        prompt: &str,
        estimated_usd: f64,
    ) -> Result<JobResult> {
        let jid = job.job_id.clone();
        let built = async {
            let parsed: ImageApiResponse = serde_json::from_str(body_text)?;
            if parsed.data.is_empty() {
                return Err(Error::other(
                    "upstream reported success but returned no image data",
                ));
            }
            let job_dir = library.ensure_job_dir(&jid)?;
            library.write_prompt(&job_dir, prompt)?;
            let mut assets = Vec::new();
            for (i, item) in parsed.data.iter().enumerate() {
                assets.push(
                    self.materialize_image_asset(&jid, &job_dir, i, item)
                        .await?,
                );
            }
            let ticks = parsed.usage.as_ref().and_then(|u| u.cost_in_usd_ticks);
            Ok::<_, Error>((job_dir, assets, ticks))
        }
        .await;

        let (job_dir, assets, ticks) = match built {
            Ok(v) => v,
            Err(e) => return self.fail_job_err(job, store, e),
        };

        let downloaded = assets.iter().any(|a| a.local_path.is_some());
        if self.auto_download && !downloaded {
            job.ok = false;
            job.error = Some("auto_download failed; assets kept as upstream urls only".into());
            job.error_type = Some("download".into());
        } else {
            job.ok = true;
        }
        job.status = JobStatus::Done;
        job.assets = assets;
        job.usage = Some(UsageInfo {
            estimated_usd: Some(estimated_usd),
            upstream_ticks: ticks,
        });
        job.completed_at = Some(Utc::now());
        if let Err(e) = library.write_meta(&job_dir, &job) {
            tracing::warn!(job_id = %job.job_id, "write_meta failed (non-fatal): {e}");
        }
        if let Some(s) = store {
            s.upsert_result(&job)?;
        }
        Ok(job)
    }

    async fn materialize_image_asset(
        &self,
        job_id: &JobId,
        job_dir: &Path,
        i: usize,
        item: &ImageApiItem,
    ) -> Result<Asset> {
        let asset_id = AssetId::new();
        let dest = job_dir.join(format!("{:02}.png", i));
        let mut local_path = None;

        // Inline bytes are already paid-for and in RAM. Persist them even when
        // auto_download is off — that flag only skips fetching ephemeral URLs.
        if let Some(b64) = &item.b64_json {
            match write_b64_image(&dest, b64) {
                Ok(()) => local_path = Some(dest.display().to_string()),
                Err(e) if item.url.is_none() => return Err(e),
                Err(e) => tracing::warn!(
                    job_id = %job_id,
                    "image b64 decode failed for asset {i} (will try url): {e}"
                ),
            }
        }

        if local_path.is_none() && self.auto_download {
            if let Some(url) = &item.url {
                match library::download_url(&self.http, url, &dest).await {
                    Ok(_) => local_path = Some(dest.display().to_string()),
                    Err(e) => tracing::warn!(
                        job_id = %job_id,
                        "image download failed for asset {i}: {e}"
                    ),
                }
            }
        }
        let file_output = item.file_output.as_ref();
        Ok(Asset {
            id: asset_id,
            kind: AssetKind::Image,
            content_url: local_path
                .as_ref()
                .map(|_| library::Library::content_url(job_id, i)),
            local_path,
            upstream_url: item.url.clone(),
            file_id: file_output.and_then(|f| f.file_id.clone()),
            public_url: file_output.and_then(|f| f.public_url.clone()),
            mime_type: item.mime_type.clone(),
        })
    }

    /// POST an image payload. Transport / body-read failures mark the job Failed
    /// so it is never left stranded in `running` with no upstream id.
    async fn post_image_json(
        &self,
        url: &str,
        payload: &Value,
        job: JobResult,
        store: Option<&JobStore>,
    ) -> Result<String> {
        let resp = match self
            .http
            .post(url)
            .headers(self.auth_headers())
            .json(payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return self
                    .fail_job_err(job, store, e.into())
                    .map(|_| String::new());
            }
        };
        let status = resp.status();
        let body_text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                return self
                    .fail_job_err(job, store, e.into())
                    .map(|_| String::new());
            }
        };
        if !status.is_success() {
            return self
                .fail_job(job, store, status.as_u16(), body_text)
                .map(|_| String::new());
        }
        Ok(body_text)
    }

    fn fail_job(
        &self,
        job: JobResult,
        store: Option<&JobStore>,
        status: u16,
        body_text: String,
    ) -> Result<JobResult> {
        let err = Error::Upstream {
            status,
            body: body_text.chars().take(2000).collect(),
        };
        self.fail_job_err(job, store, err)
    }

    fn fail_job_err(
        &self,
        mut job: JobResult,
        store: Option<&JobStore>,
        err: Error,
    ) -> Result<JobResult> {
        job.ok = false;
        job.status = JobStatus::Failed;
        job.error = Some(err.to_string());
        job.error_type = Some(match &err {
            Error::UpstreamTransport(_) => "transport".into(),
            Error::SpendLimit(_) => "spend_limit".into(),
            Error::RateLimit { .. } => "rate_limit".into(),
            Error::InvalidMode(_) => "invalid_mode".into(),
            Error::Serde(_) | Error::Other(_) => "parse".into(),
            _ => "upstream".into(),
        });
        job.completed_at = Some(Utc::now());
        if let Some(s) = store {
            let _ = s.upsert_result(&job);
        }
        Err(err)
    }

    fn mark_failed(&self, job: &JobResult, store: Option<&JobStore>, err: &Error) -> Result<()> {
        let mut j = job.clone();
        j.ok = false;
        j.status = JobStatus::Failed;
        j.error = Some(err.to_string());
        j.error_type = Some("upstream".into());
        j.completed_at = Some(Utc::now());
        if let Some(s) = store {
            s.upsert_result(&j)?;
        }
        Ok(())
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

/// Print-friendly catalog for CLI.
pub fn models_table_json() -> Value {
    json!({
        "product": crate::PRODUCT,
        "version": crate::VERSION,
        "models": models::catalog(),
        "image_aspect_ratios": models::IMAGE_ASPECT_RATIOS,
        "video_aspect_ratios": models::VIDEO_ASPECT_RATIOS,
        "image_resolutions": models::IMAGE_RESOLUTIONS,
        "image_quality_levels": models::IMAGE_QUALITY_LEVELS,
        "video_resolutions": models::VIDEO_RESOLUTIONS,
        "preset_voice_ids": models::PRESET_VOICE_IDS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn dummy_client(auto_download: bool) -> ImagineClient {
        ImagineClient {
            http: Client::new(),
            base_url: "http://127.0.0.1".into(),
            api_key: "test".into(),
            auto_download,
            storage_profile: "local".into(),
            storage_public_url: false,
            poll_interval: Duration::from_secs(1),
            poll_timeout: Duration::from_secs(30),
            limits: crate::config::LimitsConfig::default(),
            local_rate: None,
        }
    }

    #[tokio::test]
    async fn b64_json_is_written_when_auto_download_is_off() {
        let dir = tempdir().unwrap();
        let client = dummy_client(false);
        let job_id = JobId::new();
        // 1×1 PNG
        let b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let item = ImageApiItem {
            url: None,
            b64_json: Some(b64.into()),
            mime_type: Some("image/png".into()),
            file_output: None,
        };
        let asset = client
            .materialize_image_asset(&job_id, dir.path(), 0, &item)
            .await
            .unwrap();
        let path = asset.local_path.expect("b64 must land on disk");
        assert!(std::path::Path::new(&path).is_file());
        assert!(asset.content_url.is_some());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }

    #[tokio::test]
    async fn url_only_is_not_fetched_when_auto_download_is_off() {
        let dir = tempdir().unwrap();
        let client = dummy_client(false);
        let job_id = JobId::new();
        let item = ImageApiItem {
            url: Some("https://imgen.example/ephemeral.png".into()),
            b64_json: None,
            mime_type: Some("image/png".into()),
            file_output: None,
        };
        let asset = client
            .materialize_image_asset(&job_id, dir.path(), 0, &item)
            .await
            .unwrap();
        assert!(asset.local_path.is_none());
        assert!(asset.content_url.is_none());
        assert_eq!(
            asset.upstream_url.as_deref(),
            Some("https://imgen.example/ephemeral.png")
        );
    }

    #[test]
    fn write_b64_rejects_garbage() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("00.png");
        assert!(write_b64_image(&dest, "%%%not-b64%%%").is_err());
        assert!(!dest.exists());
    }
}
