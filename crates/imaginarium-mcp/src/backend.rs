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
use imaginarium_core::library::{media_from_node_input, Library};
use imaginarium_core::models::{
    parse_model_selector, parse_optional_image_quality, parse_reference_audios, ModelId,
};
use imaginarium_core::types::{JobId, JobResult};
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
        resolution: Option<&str>,
    ) -> Result<Value>;
    async fn image_generate(&self, args: &Value) -> Result<Value>;
    async fn image_edit(&self, args: &Value) -> Result<Value>;
    async fn video_generate(&self, args: &Value) -> Result<Value>;
    async fn video_edit(&self, args: &Value) -> Result<Value>;
    async fn video_extend(&self, args: &Value) -> Result<Value>;
    async fn job_status(&self, job_id: &str) -> Result<Value>;
    async fn job_wait(&self, job_id: &str) -> Result<Value>;
    async fn jobs_list(&self, limit: usize) -> Result<Value>;
    /// Render a craft timeline (contract v1) into a library job.
    async fn craft_video(&self, args: &Value) -> Result<Value>;
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
    match parse_model_selector(s).map_err(|e| anyhow!(e))? {
        Some(m) => Ok(m),
        None => ModelId::parse(default).map_err(|e| anyhow!(e)),
    }
}

fn optional_model(s: Option<&str>) -> Result<Option<ModelId>> {
    parse_model_selector(s).map_err(|e| anyhow!(e))
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
        resolution: Option<&str>,
    ) -> Result<Value> {
        let def = if kind == "video" { "1.5" } else { "image" };
        let m = parse_model(model, def)?;
        let e = if kind == "video" {
            estimate::estimate_video(m, duration, resolution)
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
        let quality = parse_optional_image_quality(args["quality"].as_str())?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .image_generate(
                ImageGenerateRequest {
                    prompt,
                    model,
                    n: crate::args::u32_or(args, "n", 1)?,
                    aspect_ratio: args["aspect_ratio"].as_str().map(str::to_string),
                    resolution: args["resolution"].as_str().map(str::to_string),
                    quality,
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
            .map(|s| media_from_node_input(s, &self.library.root))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| anyhow!(e))?;
        let model = parse_model(args["model"].as_str(), "image")?;
        let quality = parse_optional_image_quality(args["quality"].as_str())?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .image_edit(
                ImageEditRequest {
                    prompt,
                    model,
                    images,
                    n: crate::args::u32_or(args, "n", 1)?,
                    aspect_ratio: args["aspect_ratio"].as_str().map(str::to_string),
                    resolution: args["resolution"].as_str().map(str::to_string),
                    quality,
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
        let model = optional_model(args["model"].as_str())?;
        let explicit = model.is_some();
        let refs = match args["reference_images"].as_array() {
            Some(a) => a
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| media_from_node_input(s, &self.library.root))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| anyhow!(e))?,
            None => Vec::new(),
        };
        let image = args["image"]
            .as_str()
            .map(|s| media_from_node_input(s, &self.library.root))
            .transpose()
            .map_err(|e| anyhow!(e))?;
        let reference_audios = match args["reference_audios"].as_array() {
            Some(a) => parse_reference_audios(a.iter().filter_map(|v| v.as_str()))
                .map_err(|e| anyhow!(e))?,
            None => {
                // Also accept a single string or voice_id alias.
                if let Some(one) = args["voice_id"].as_str().or_else(|| args["voice"].as_str()) {
                    parse_reference_audios([one]).map_err(|e| anyhow!(e))?
                } else {
                    Vec::new()
                }
            }
        };
        let no_wait = crate::args::bool_or(args, "no_wait", false)?;
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
                    reference_audios,
                    duration: crate::args::optional_u32(args, "duration")?,
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
        let model = optional_model(args["model"].as_str())?;
        let no_wait = crate::args::bool_or(args, "no_wait", false)?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_edit(
                VideoEditRequest {
                    prompt,
                    video: media_from_node_input(video, &self.library.root)
                        .map_err(|e| anyhow!(e))?,
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
        let model = optional_model(args["model"].as_str())?;
        let no_wait = crate::args::bool_or(args, "no_wait", false)?;
        let jobs = self.jobs()?;
        let client = self.client()?;
        let r = client
            .video_extend(
                VideoExtendRequest {
                    prompt,
                    video: media_from_node_input(video, &self.library.root)
                        .map_err(|e| anyhow!(e))?,
                    duration: crate::args::optional_u32(args, "duration")?,
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

    async fn craft_video(&self, args: &Value) -> Result<Value> {
        let timeline: imaginarium_core::craft_video::VideoTimeline = serde_json::from_value(
            args.get("timeline")
                .cloned()
                .ok_or_else(|| anyhow!("timeline required (VideoTimeline v1 object)"))?,
        )
        .map_err(|e| anyhow!("timeline parse: {e}"))?;
        let wait = crate::args::bool_or(args, "wait", false)?;
        let jobs = self.jobs()?;
        let library = self.library.clone();
        let root = self.cfg.library_dir();
        if wait {
            // Blocking render on this tool call (the MCP loop is serial —
            // callers wanting concurrency use wait=false + job_status).
            let r = tokio::task::spawn_blocking(move || {
                imaginarium_core::craft_video::render_timeline(
                    &library, &jobs, &root, &timeline, None,
                )
            })
            .await
            .map_err(|e| anyhow!("render task: {e}"))?
            .map_err(|e| anyhow!(e))?;
            return Ok(job_json(r));
        }
        // Async: pending row now, render in the background, finalize under the
        // same id; every failure path flips the row to failed (never stuck).
        let job_id = JobId::new();
        let pending = JobResult::pending(
            job_id.clone(),
            imaginarium_core::types::JobMode::CraftExport,
            "local-craft",
            timeline.note.clone(),
        );
        jobs.upsert_result(&pending).map_err(|e| anyhow!(e))?;
        let db_path = self.cfg.db_path();
        let jid = job_id.clone();
        std::thread::spawn(move || {
            let rendered = imaginarium_core::craft_video::render_timeline(
                &library,
                &jobs,
                &root,
                &timeline,
                Some(jid.clone()),
            );
            if let Err(e) = rendered {
                if let Ok(store) = JobStore::open(&db_path) {
                    let failed = JobResult::failure(
                        jid,
                        imaginarium_core::types::JobMode::CraftExport,
                        "local-craft",
                        "craft",
                        e.to_string(),
                    );
                    let _ = store.upsert_result(&failed);
                }
            }
        });
        Ok(job_json(pending))
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
        resolution: Option<&str>,
    ) -> Result<Value> {
        self.post(
            "/v1/estimate",
            json!({
                "kind": kind,
                "model": model,
                "n": n,
                "duration": duration,
                "resolution": resolution,
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

    async fn craft_video(&self, args: &Value) -> Result<Value> {
        let timeline = args
            .get("timeline")
            .cloned()
            .ok_or_else(|| anyhow!("timeline required (VideoTimeline v1 object)"))?;
        let wait = crate::args::bool_or(args, "wait", false)?;
        let path = if wait {
            "/v1/craft/video/render".to_string()
        } else {
            "/v1/craft/video/render?no_wait=true".to_string()
        };
        self.post(&path, timeline).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_model_is_not_a_parse_error() {
        assert!(optional_model(Some("auto")).unwrap().is_none());
        assert!(optional_model(Some("")).unwrap().is_none());
        assert!(optional_model(None).unwrap().is_none());
        assert_eq!(parse_model(Some("auto"), "video").unwrap(), ModelId::Video);
        assert_eq!(optional_model(Some("1.5")).unwrap(), Some(ModelId::Video15));
        assert!(optional_model(Some("nope")).is_err());
    }
}
