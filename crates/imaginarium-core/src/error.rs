//! Error types for imaginarium-core.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("invalid mode: {0}")]
    InvalidMode(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("missing credential: {0}")]
    MissingCredential(String),

    #[error("upstream xAI HTTP {status}: {body}")]
    Upstream { status: u16, body: String },

    #[error("upstream request failed: {0}")]
    UpstreamTransport(String),

    #[error("job not found: {0}")]
    JobNotFound(String),

    #[error("asset not found: {0}")]
    AssetNotFound(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("serialization error: {0}")]
    Serde(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn invalid_mode(msg: impl Into<String>) -> Self {
        Self::InvalidMode(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Self::Config(value.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Self::Config(value.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Db(value.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::UpstreamTransport(value.to_string())
    }
}
