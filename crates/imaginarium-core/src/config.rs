//! Configuration loading and defaults.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::{self, ensure_layout};

pub const DEFAULT_BASE_URL: &str = "https://api.x.ai/v1";
pub const DEFAULT_BIND: &str = crate::DEFAULT_BIND;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub upstream: UpstreamConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub library: LibraryConfig,
    #[serde(default)]
    pub poll: PollConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    /// Resolved data home (not always serialized from file).
    #[serde(skip)]
    pub data_home: PathBuf,
    #[serde(skip)]
    pub config_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// Env var name to read for API key (default XAI_API_KEY).
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    /// Optional plaintext key in config (discouraged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// server | client | either — who may supply the xAI key.
    #[serde(default = "default_key_source")]
    pub key_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// local | xai_files
    #[serde(default = "default_storage_profile")]
    pub profile: String,
    #[serde(default)]
    pub public_url: bool,
    #[serde(default = "default_true")]
    pub auto_download: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryConfig {
    /// Override library root; default `$data_home/library`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollConfig {
    #[serde(default = "default_poll_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_poll_timeout_s")]
    pub timeout_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub allow_localhost_no_auth: bool,
    /// Env var for shared node/admin token (ApexOS AGENTD_TOKEN analogue).
    #[serde(default = "default_node_token_env")]
    pub node_token_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_image_model")]
    pub image_model: String,
    #[serde(default = "default_video_model")]
    pub video_model: String,
    #[serde(default = "default_i2v_model")]
    pub i2v_model: String,
    #[serde(default = "default_image_res")]
    pub image_resolution: String,
    #[serde(default = "default_video_res")]
    pub video_resolution: String,
    #[serde(default = "default_duration")]
    pub video_duration: u32,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            api_key_env: default_api_key_env(),
            api_key: None,
            base_url: default_base_url(),
            key_source: default_key_source(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            profile: default_storage_profile(),
            public_url: false,
            auto_download: true,
        }
    }
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_poll_interval_ms(),
            timeout_s: default_poll_timeout_s(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            allow_localhost_no_auth: false,
            node_token_env: default_node_token_env(),
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            image_model: default_image_model(),
            video_model: default_video_model(),
            i2v_model: default_i2v_model(),
            image_resolution: default_image_res(),
            video_resolution: default_video_res(),
            video_duration: default_duration(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            upstream: UpstreamConfig::default(),
            storage: StorageConfig::default(),
            library: LibraryConfig::default(),
            poll: PollConfig::default(),
            server: ServerConfig::default(),
            defaults: DefaultsConfig::default(),
            data_home: PathBuf::new(),
            config_path: PathBuf::new(),
        }
    }
}

fn default_api_key_env() -> String {
    "XAI_API_KEY".into()
}
fn default_base_url() -> String {
    DEFAULT_BASE_URL.into()
}
fn default_key_source() -> String {
    "server".into()
}
fn default_storage_profile() -> String {
    "local".into()
}
fn default_true() -> bool {
    true
}
fn default_poll_interval_ms() -> u64 {
    5000
}
fn default_poll_timeout_s() -> u64 {
    600
}
fn default_bind() -> String {
    DEFAULT_BIND.into()
}
fn default_node_token_env() -> String {
    "IMAGINARIUM_TOKEN".into()
}
fn default_image_model() -> String {
    "grok-imagine-image".into()
}
fn default_video_model() -> String {
    "grok-imagine-video-1.5".into()
}
fn default_i2v_model() -> String {
    "grok-imagine-video-1.5".into()
}
fn default_image_res() -> String {
    "1k".into()
}
fn default_video_res() -> String {
    "720p".into()
}
fn default_duration() -> u32 {
    8
}

impl Config {
    /// Load config from disk (or defaults), resolve paths, ensure layout.
    pub fn load() -> Result<Self> {
        let config_path = paths::config_path()?;
        let data_home = paths::data_home()?;
        Self::load_from(&config_path, data_home)
    }

    pub fn load_from(config_path: &Path, data_home: PathBuf) -> Result<Self> {
        let mut cfg = if config_path.is_file() {
            let text = std::fs::read_to_string(config_path)?;
            toml::from_str::<Config>(&text)?
        } else {
            Config::default()
        };
        cfg.config_path = config_path.to_path_buf();
        cfg.data_home = data_home;
        ensure_layout(&cfg.data_home)?;
        if let Some(parent) = cfg.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(cfg)
    }

    /// Write default config if missing; return path.
    pub fn init_file() -> Result<PathBuf> {
        let path = paths::config_path()?;
        let data_home = paths::data_home()?;
        ensure_layout(&data_home)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.is_file() {
            return Ok(path);
        }
        let cfg = Config {
            data_home,
            config_path: path.clone(),
            ..Config::default()
        };
        let body = toml::to_string_pretty(&cfg.serializable())?;
        std::fs::write(&path, body)?;
        Ok(path)
    }

    /// Strip non-serializable fields for writing.
    pub fn serializable(&self) -> ConfigFile {
        ConfigFile {
            upstream: self.upstream.clone(),
            storage: self.storage.clone(),
            library: self.library.clone(),
            poll: self.poll.clone(),
            server: self.server.clone(),
            defaults: self.defaults.clone(),
        }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(&self.serializable())?;
        std::fs::write(&self.config_path, body)?;
        Ok(())
    }

    /// Resolve upstream API key: config plaintext → env.
    pub fn resolve_api_key(&self) -> Result<String> {
        if let Some(k) = self
            .upstream
            .api_key
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return Ok(k);
        }
        let env_name = &self.upstream.api_key_env;
        match std::env::var(env_name) {
            Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
            _ => Err(Error::MissingCredential(format!(
                "set {env_name} or upstream.api_key in config"
            ))),
        }
    }

    pub fn base_url(&self) -> &str {
        self.upstream.base_url.trim_end_matches('/')
    }

    pub fn library_dir(&self) -> PathBuf {
        self.library
            .dir
            .clone()
            .unwrap_or_else(|| paths::library_root(&self.data_home))
    }

    pub fn db_path(&self) -> PathBuf {
        paths::db_path(&self.data_home)
    }

    pub fn tokens_db_path(&self) -> PathBuf {
        paths::tokens_db_path(&self.data_home)
    }

    /// Shared node/admin token (env), ApexOS AGENTD_TOKEN style.
    pub fn resolve_node_token(&self) -> Option<String> {
        let env_name = &self.server.node_token_env;
        std::env::var(env_name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

/// On-disk TOML shape (no skip fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub upstream: UpstreamConfig,
    pub storage: StorageConfig,
    pub library: LibraryConfig,
    pub poll: PollConfig,
    pub server: ServerConfig,
    pub defaults: DefaultsConfig,
}
