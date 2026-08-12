//! Imaginarium core — xAI Imagine client, capability matrix, config, jobs, local library.

pub mod client;
pub mod config;
pub mod craft_video;
pub mod error;
pub mod estimate;
pub mod jobs;
pub mod library;
pub mod models;
pub mod paths;
pub mod rate_limit;
pub mod tokens;
pub mod types;

pub use config::Config;
pub use error::{Error, Result};
pub use models::{
    catalog, default_video_model_for, parse_optional_image_quality, parse_reference_audios,
    validate_image_quality, validate_video_generate, ImageQuality, ModelId, ModelInfo, VideoMode,
};
pub use rate_limit::{is_paid_upstream, RateLimiter};
pub use tokens::{
    extract_presented_token, is_loopback_bind, AuthIdentity, AuthSource, TokenMintResult,
    TokenRecord, TokenScope, TokenStore,
};
pub use types::*;

/// Crate / product version (workspace package version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default HTTP listen address for `imaginarium serve` (loopback).
pub const DEFAULT_BIND: &str = "127.0.0.1:8791";

/// Product name.
pub const PRODUCT: &str = "Imaginarium-RS";
