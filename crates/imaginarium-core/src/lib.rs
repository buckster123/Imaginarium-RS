//! Imaginarium core — xAI Imagine client, capability matrix, config, jobs, local library.

pub mod client;
pub mod config;
pub mod error;
pub mod estimate;
pub mod jobs;
pub mod library;
pub mod models;
pub mod paths;
pub mod types;

pub use config::Config;
pub use error::{Error, Result};
pub use models::{
    catalog, default_video_model_for, validate_video_generate, ModelId, ModelInfo, VideoMode,
};
pub use types::*;

/// Crate / product version (workspace package version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default HTTP listen address for `imaginarium serve` (loopback).
pub const DEFAULT_BIND: &str = "127.0.0.1:8791";

/// Product name.
pub const PRODUCT: &str = "Imaginarium-RS";

/// MCP stub helper used by CLI until Phase 4.
pub fn mcp_stub_message(proxy: Option<&str>) -> String {
    match proxy {
        Some(url) => format!(
            "Imaginarium MCP stub — planned thin proxy to {url} (Phase 4). See docs/AGENTS.md."
        ),
        None => {
            "Imaginarium MCP stub — stdio server planned in Phase 4. Core image client is ready."
                .into()
        }
    }
}
