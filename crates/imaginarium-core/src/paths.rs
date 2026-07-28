//! Filesystem layout under IMAGINARIUM_HOME / XDG data dir.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Error, Result};

const QUALIFIER: &str = "";
const ORGANIZATION: &str = "Imaginarium";
const APPLICATION: &str = "imaginarium";

/// Resolve data home: `$IMAGINARIUM_HOME` or platform project data dir.
pub fn data_home() -> Result<PathBuf> {
    if let Ok(v) = std::env::var("IMAGINARIUM_HOME") {
        let p = PathBuf::from(v);
        if !p.as_os_str().is_empty() {
            return Ok(p);
        }
    }
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| Error::config("could not resolve data directory"))
}

/// Config path: `$IMAGINARIUM_CONFIG` or `$IMAGINARIUM_HOME/config.toml` or XDG config.
pub fn config_path() -> Result<PathBuf> {
    if let Ok(v) = std::env::var("IMAGINARIUM_CONFIG") {
        return Ok(PathBuf::from(v));
    }
    if let Ok(home) = std::env::var("IMAGINARIUM_HOME") {
        return Ok(PathBuf::from(home).join("config.toml"));
    }
    let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .ok_or_else(|| Error::config("could not resolve config directory"))?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn ensure_layout(home: &Path) -> Result<()> {
    for sub in ["library", "cache", "tokens"] {
        std::fs::create_dir_all(home.join(sub))?;
    }
    Ok(())
}

pub fn db_path(home: &Path) -> PathBuf {
    home.join("imaginarium.db")
}

pub fn tokens_db_path(home: &Path) -> PathBuf {
    // Co-locate with main DB for simplicity; separate file also fine.
    home.join("tokens.db")
}

pub fn library_root(home: &Path) -> PathBuf {
    home.join("library")
}

pub fn cache_root(home: &Path) -> PathBuf {
    home.join("cache")
}

/// Job asset directory: `library/YYYY/MM/DD/<job_id>/`
pub fn job_dir(home: &Path, job_id: &str, when: chrono::DateTime<chrono::Utc>) -> PathBuf {
    library_root(home)
        .join(when.format("%Y").to_string())
        .join(when.format("%m").to_string())
        .join(when.format("%d").to_string())
        .join(job_id)
}
