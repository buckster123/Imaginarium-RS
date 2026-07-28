//! Local vs remote (HTTP proxy) backends for MCP tools.

use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use imaginarium_core::client::{
    models_table_json, ImageEditRequest, ImageGenerateRequest, ImagineClient, ResponseFormat,
    VideoEditRequest, VideoExtendRequest, VideoGenerateRequest,
};
use imaginarium_core::config::Config;
use imaginarium_core::estimate;
use imaginarium_core::jobs::JobStore;
use imaginarium_core::library::Library;
use imaginarium_core::models::ModelId;
use imaginarium_core::types::{JobId, JobResult, MediaRef};
use reqwest::Client;
use serde_json::{json, Value};

#[async_trait]
pub trait Backend: Send + Sync {
    async fn models(&self) -> Result<Value>;
    async fn estimate(
        &self,
        kind: &str,
        model: Option<&str>,
        n: u32,
        duration: u32,
    ) -> Result<Value>;
    async fn image_generate(&self, args: &Value) -> Result<Value>;
    async fn image_edit(&self, args: &Value) -> Result<Value>;
    async fn video_generate(&self, args: &Value) -> Result<Value>;
    async fn video_edit(&self, args: &Value) -> Result<Value>;
    async fn video_extend(&self, args: &Value) -> Result<Value>;
    async fn job_status(&self, job_id: &str) -> Result<Value>;
    async fn job_wait(&self, job_id: &str) -> Result<Value>;
    async fn jobs_list(&self, limit: usize) -> Result<Value>;
}

pub struct LocalBackend {
    client: OnceLock<ImagineClient>,
    library: Library,
    cfg: Config,
}

impl LocalBackend {
    pub fn new(cfg: Config) -> Result<Self> {
        let library = Library::new(cfg.library_dir());
        Ok(Self {
            client: OnceLock::new(),
            library,
            cfg,
        })
    }

    fn jobs(&self) -> Result<JobStore> {
        JobStore::open(&self.cfg.db_path()).map_err(|e| anyhow!(e))
    }

    fn client(&self) -> Result<&ImagineClient> {
        if self.client.get().is_none() {
            let c = ImagineClient::from_config(&self.cfg).map_err(|e| anyhow!(e))?;
            // Another task may have won the race — ignore AlreadyInit.
            let _ = self.client.set(c);
        }
        self.client
            .get()
            .ok_or_else(|| anyhow!("ImagineClient failed to initialize"))
    }
}

fn parse_model(s: Option<&str>, default: &str) -> Result<ModelId> {
    ModelId::parse(s.unwrap_or(default)).map_err(|e| anyhow!(e))
}

fn job_json(r: JobResult) -> Value {
    serde_json::to_value(r).unwrap_or(json!({ "ok": false, "error": "serialize" }))
}

#[async_trait]
impl Backend for LocalBackend {
    async fn models(&self) -> Result<Value> {
        Ok(models_table_json())
    }

    async fn estimate(
        &self,
        kind: &str,
        model: Option<&str>,
        n: u32,
        duration: u32,
    ) -> Result<Value> {
        let def = if kind == "video" { "video" } else { "image" };
        let m = parse_model(model, def)?;
        let e = if kind == "video" {
            estimate::estimate_video(m, duration)
        } else {
            estimate::estimate_image(m, n)
        };
        Ok(serde_json::to_value(e)?)
    }

    async fn image_generate(&self, args: &Value) -> Result<Value> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("prompt required"))?
            .to_string();
        let model = parse_model(args["model"].as_str(), "image")?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .image_generate(
                ImageGenerateRequest {
                    prompt,
                    model,
                    n: args["n"].as_u64().unwrap_or(1) as u32,
                    aspect_ratio: args["aspect_ratio"].as_str().map(str::to_string),
                    resolution: args["resolution"].as_str().map(str::to_string),
                    response_format: ResponseFormat::Url,
                },
                &self.library,
                Some(&jobs),
            )
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn image_edit(&self, args: &Value) -> Result<Value> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("prompt required"))?
            .to_string();
        let images = args["images"]
            .as_array()
            .ok_or_else(|| anyhow!("images required"))?
            .iter()
            .filter_map(|v| v.as_str())
            .map(MediaRef::from_remote_input)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow!(e))?;
        let model = parse_model(args["model"].as_str(), "image")?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .image_edit(
                ImageEditRequest {
                    prompt,
                    model,
                    images,
                    n: args["n"].as_u64().unwrap_or(1) as u32,
                    aspect_ratio: args["aspect_ratio"].as_str().map(str::to_string),
                    resolution: args["resolution"].as_str().map(str::to_string),
                    response_format: ResponseFormat::Url,
                },
                &self.library,
                Some(&jobs),
            )
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn video_generate(&self, args: &Value) -> Result<Value> {
        let explicit = args.get("model").and_then(|v| v.as_str()).is_some();
        let model = args["model"]
            .as_str()
            .map(ModelId::parse)
            .transpose()
            .map_err(|e| anyhow!(e))?;
        let refs = match args["reference_images"].as_array() {
            Some(a) => a
                .iter()
                .filter_map(|v| v.as_str())
                .map(MediaRef::from_remote_input)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!(e))?,
            None => Vec::new(),
        };
        let image = args["image"]
            .as_str()
            .map(MediaRef::from_remote_input)
            .transpose()
            .map_err(|e| anyhow!(e))?;
        let no_wait = args["no_wait"].as_bool().unwrap_or(false);
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_generate(
                VideoGenerateRequest {
                    prompt: args["prompt"].as_str().map(str::to_string),
                    model,
                    explicit_model: explicit,
                    image,
                    reference_images: refs,
                    duration: args["duration"].as_u64().map(|d| d as u32),
                    aspect_ratio: args["aspect_ratio"].as_str().map(str::to_string),
                    resolution: args["resolution"].as_str().map(str::to_string),
                },
                &self.library,
                Some(&jobs),
                !no_wait,
            )
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn video_edit(&self, args: &Value) -> Result<Value> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("prompt required"))?
            .to_string();
        let video = args["video"]
            .as_str()
            .ok_or_else(|| anyhow!("video required"))?;
        let model = args["model"]
            .as_str()
            .map(ModelId::parse)
            .transpose()
            .map_err(|e| anyhow!(e))?;
        let no_wait = args["no_wait"].as_bool().unwrap_or(false);
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_edit(
                VideoEditRequest {
                    prompt,
                    video: MediaRef::from_remote_input(video).map_err(|e| anyhow!(e))?,
                    model,
                },
                &self.library,
                Some(&jobs),
                !no_wait,
            )
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn video_extend(&self, args: &Value) -> Result<Value> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow!("prompt required"))?
            .to_string();
        let video = args["video"]
            .as_str()
            .ok_or_else(|| anyhow!("video required"))?;
        let model = args["model"]
            .as_str()
            .map(ModelId::parse)
            .transpose()
            .map_err(|e| anyhow!(e))?;
        let no_wait = args["no_wait"].as_bool().unwrap_or(false);
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_extend(
                VideoExtendRequest {
                    prompt,
                    video: MediaRef::from_remote_input(video).map_err(|e| anyhow!(e))?,
                    duration: args["duration"].as_u64().map(|d| d as u32),
                    model,
                },
                &self.library,
                Some(&jobs),
                !no_wait,
            )
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn job_status(&self, job_id: &str) -> Result<Value> {
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_status_once(&JobId(job_id.to_string()), &self.library, &jobs)
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn job_wait(&self, job_id: &str) -> Result<Value> {
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_wait(&JobId(job_id.to_string()), &self.library, &jobs)
            .await
            .map_err(|e| anyhow!(e))?;
        Ok(job_json(r))
    }

    async fn jobs_list(&self, limit: usize) -> Result<Value> {
        let jobs = self.jobs()?;
        let list = jobs.list_recent(limit).map_err(|e| anyhow!(e))?;
        Ok(serde_json::to_value(list)?)
    }
}

/// Thin HTTP client to a fat Imaginarium node (Phase 3 API).
pub struct ProxyBackend {
    http: Client,
    base: String,
    token: String,
}

impl ProxyBackend {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            http: Client::new(),
            base: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .context("proxy GET")?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("proxy HTTP {status}: {body}"));
        }
        Ok(serde_json::from_str(&body)?)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
            .context("proxy POST")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("proxy HTTP {status}: {text}"));
        }
        Ok(serde_json::from_str(&text)?)
    }
}

#[async_trait]
impl Backend for ProxyBackend {
    async fn models(&self) -> Result<Value> {
        self.get("/v1/models").await
    }

    async fn estimate(
        &self,
        kind: &str,
        model: Option<&str>,
        n: u32,
        duration: u32,
    ) -> Result<Value> {
        self.post(
            "/v1/estimate",
            json!({
                "kind": kind,
                "model": model,
                "n": n,
                "duration": duration
            }),
        )
        .await
    }

    async fn image_generate(&self, args: &Value) -> Result<Value> {
        self.post("/v1/images/generations", args.clone()).await
    }

    async fn image_edit(&self, args: &Value) -> Result<Value> {
        self.post("/v1/images/edits", args.clone()).await
    }

    async fn video_generate(&self, args: &Value) -> Result<Value> {
        self.post("/v1/videos/generations", args.clone()).await
    }

    async fn video_edit(&self, args: &Value) -> Result<Value> {
        self.post("/v1/videos/edits", args.clone()).await
    }

    async fn video_extend(&self, args: &Value) -> Result<Value> {
        self.post("/v1/videos/extensions", args.clone()).await
    }

    async fn job_status(&self, job_id: &str) -> Result<Value> {
        self.get(&format!("/v1/jobs/{job_id}")).await
    }

    async fn job_wait(&self, job_id: &str) -> Result<Value> {
        self.post(&format!("/v1/jobs/{job_id}/wait"), json!({}))
            .await
    }

    async fn jobs_list(&self, limit: usize) -> Result<Value> {
        self.get(&format!("/v1/jobs?limit={limit}")).await
    }
}
