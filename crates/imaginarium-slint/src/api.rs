//! Thin HTTP client for imaginarium serve (edge UI).

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub struct NodeClient {
    http: reqwest::Client,
    pub base: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    #[allow(dead_code)]
    pub ok: Option<bool>,
    pub product: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub job_id: String,
    pub mode: String,
    pub status: String,
    pub summary: String,
}

impl NodeClient {
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self {
            http,
            base: base.into().trim_end_matches('/').to_string(),
            token: token.into(),
        })
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.token.is_empty() {
            req
        } else {
            req.header("Authorization", format!("Bearer {}", self.token))
        }
    }

    pub async fn health(&self) -> Result<Health> {
        let url = format!("{}/health", self.base);
        let res = self.http.get(&url).send().await.context("health request")?;
        if !res.status().is_success() {
            return Err(anyhow!("health HTTP {}", res.status()));
        }
        Ok(res.json().await?)
    }

    pub async fn image_generate(
        &self,
        prompt: &str,
        model: Option<&str>,
        n: u32,
        aspect: Option<&str>,
        resolution: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({
            "prompt": prompt,
            "n": n,
        });
        if let Some(m) = model {
            if m != "auto" && !m.is_empty() {
                body["model"] = json!(m);
            }
        }
        if let Some(a) = aspect {
            if a != "auto" && !a.is_empty() {
                body["aspect_ratio"] = json!(a);
            }
        }
        if let Some(r) = resolution {
            if !r.is_empty() {
                body["resolution"] = json!(r);
            }
        }
        let url = format!("{}/v1/images/generations", self.base);
        let res = self
            .auth(self.http.post(&url).json(&body))
            .send()
            .await
            .context("image generate")?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("image gen HTTP {status}: {text}"));
        }
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn jobs_list(&self, limit: u32) -> Result<Vec<JobInfo>> {
        let url = format!("{}/v1/jobs?limit={limit}", self.base);
        let res = self
            .auth(self.http.get(&url))
            .send()
            .await
            .context("jobs list")?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("jobs HTTP {status}: {text}"));
        }
        let v: Value = serde_json::from_str(&text)?;
        let arr = v
            .as_array()
            .cloned()
            .or_else(|| v.get("jobs").and_then(|j| j.as_array().cloned()))
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .filter_map(|j| {
                let job_id = j.get("job_id")?.as_str()?.to_string();
                let mode = j
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .unwrap_or("?")
                    .to_string();
                let status = j
                    .get("status")
                    .and_then(|m| m.as_str())
                    .unwrap_or("?")
                    .to_string();
                let summary = j
                    .get("prompt")
                    .and_then(|m| m.as_str())
                    .or_else(|| j.get("error").and_then(|m| m.as_str()))
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect();
                Some(JobInfo {
                    job_id,
                    mode,
                    status,
                    summary,
                })
            })
            .collect())
    }

    /// Download library content to a cache path; returns path.
    pub async fn download_job_content(&self, job_id: &str, dest_dir: &std::path::Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dest_dir)?;
        let mut url = format!("{}/v1/library/{}/content", self.base, job_id);
        // also support query token for dumb openers
        if !self.token.is_empty() {
            url.push_str(&format!("?token={}", urlencoding_minimal(&self.token)));
        }
        let res = self
            .auth(self.http.get(&url))
            .send()
            .await
            .context("library content")?;
        let status = res.status();
        if !status.is_success() {
            let t = res.text().await.unwrap_or_default();
            return Err(anyhow!("library content HTTP {status}: {t}"));
        }
        let bytes = res.bytes().await?;
        // sniff ext
        let ext = if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
            "png"
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            "jpg"
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" {
            "webp"
        } else if bytes.len() > 8 && &bytes[4..8] == b"ftyp" {
            "mp4"
        } else {
            "bin"
        };
        let path = dest_dir.join(format!("{job_id}.{ext}"));
        std::fs::write(&path, &bytes)?;
        Ok(path)
    }
}

fn urlencoding_minimal(s: &str) -> String {
    // enough for bearer tokens (hex / img_ ulid)
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}
