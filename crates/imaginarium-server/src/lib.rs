//! LAN HTTP API for Imaginarium-RS (Phase 3).

mod auth;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::Router;
use imaginarium_core::client::ImagineClient;
use imaginarium_core::config::Config;
use imaginarium_core::library::Library;
use imaginarium_core::tokens::{is_loopback_bind, TokenStore};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

pub use routes::api_router;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub client: Arc<ImagineClient>,
    pub library: Arc<Library>,
    pub tokens: Arc<Mutex<TokenStore>>,
}

pub struct ServeOptions {
    pub bind: String,
    pub allow_localhost_no_auth: bool,
}

/// Build router + bind checks + listen.
pub async fn serve(cfg: Config, opts: ServeOptions) -> Result<()> {
    let loopback = is_loopback_bind(&opts.bind);
    let node_token = cfg.resolve_node_token();
    let tokens = TokenStore::open(&cfg.tokens_db_path(), node_token)?;
    if !loopback && !tokens.has_any_auth()? {
        bail!(
            "refusing non-loopback bind {} without auth — set IMAGINARIUM_TOKEN \
             or run `imaginarium token create` first (ApexOS-compatible gate)",
            opts.bind
        );
    }
    if loopback && !tokens.has_any_auth()? && !opts.allow_localhost_no_auth {
        tracing::warn!(
            "no tokens configured on loopback bind; requests need a token unless \
             --allow-localhost-no-auth is set"
        );
    }

    let mut cfg = cfg;
    cfg.server.bind = opts.bind.clone();
    cfg.server.allow_localhost_no_auth = opts.allow_localhost_no_auth;

    let client =
        ImagineClient::from_config(&cfg).context("upstream xAI credentials required for serve")?;
    let library = Library::new(cfg.library_dir());

    let state = AppState {
        cfg: Arc::new(cfg),
        client: Arc::new(client),
        library: Arc::new(library),
        tokens: Arc::new(Mutex::new(tokens)),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::public_router())
        .merge(api_router(state.clone(), opts.allow_localhost_no_auth))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let addr: SocketAddr = opts
        .bind
        .parse()
        .with_context(|| format!("invalid bind address: {}", opts.bind))?;
    info!(%addr, "imaginarium listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn serve_from_config(
    cfg: Config,
    bind: Option<String>,
    allow_localhost_no_auth: bool,
) -> Result<()> {
    let bind = bind.unwrap_or_else(|| cfg.server.bind.clone());
    serve(
        cfg,
        ServeOptions {
            bind,
            allow_localhost_no_auth,
        },
    )
    .await
}

/// Resolve content file path for a job asset (best-effort first media file).
pub fn job_content_path(library_root: &PathBuf, job_id: &str) -> Option<PathBuf> {
    let root = library_root;
    if !root.is_dir() {
        return None;
    }
    for year in std::fs::read_dir(root).ok()? {
        let year = year.ok()?.path();
        for month in std::fs::read_dir(&year).ok()? {
            let month = month.ok()?.path();
            for day in std::fs::read_dir(&month).ok()? {
                let day = day.ok()?.path();
                let job_dir = day.join(job_id);
                if job_dir.is_dir() {
                    for name in ["00.mp4", "00.png", "0.mp4", "0.png"] {
                        let p = job_dir.join(name);
                        if p.is_file() {
                            return Some(p);
                        }
                    }
                    if let Ok(rd) = std::fs::read_dir(&job_dir) {
                        for e in rd.flatten() {
                            let p = e.path();
                            if p.is_file() {
                                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                                if matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "mp4" | "webm") {
                                    return Some(p);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
