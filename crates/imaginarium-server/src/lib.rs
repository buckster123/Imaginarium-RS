//! LAN HTTP API for Imaginarium-RS (Phase 3 + Phase 5 UI).

mod auth;
mod routes;
mod static_files;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::extract::DefaultBodyLimit;
use axum::Router;
use imaginarium_core::client::ImagineClient;
use imaginarium_core::config::Config;
use imaginarium_core::library::Library;
use imaginarium_core::tokens::{is_loopback_bind, TokenStore};
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;
use tracing::info;

pub use routes::api_router;

/// JSON bodies carry data-URLs for craft import / image edit — default axum 2MB is too small.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// Shared server state.
#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub client: Arc<ImagineClient>,
    pub library: Arc<Library>,
    pub tokens: Arc<Mutex<TokenStore>>,
    /// Explicit opt-in to allow tokenless Admin access for genuinely-loopback peers.
    /// Set by the server constructor. The auth middleware additionally requires the
    /// real peer IP (via `ConnectInfo`) to be loopback — never just the bind string —
    /// so an embedder that mounts `api_router` without connect-info fails closed.
    pub allow_localhost_no_auth: bool,
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

    let allow_localhost_no_auth = opts.allow_localhost_no_auth;
    let state = AppState {
        cfg: Arc::new(cfg),
        client: Arc::new(client),
        library: Arc::new(library),
        tokens: Arc::new(Mutex::new(tokens)),
        allow_localhost_no_auth,
    };

    // No wildcard CORS on an authenticated API. The embedded SPA is served from the
    // node's own origin (same-origin → no CORS needed); non-browser clients (CLI,
    // Slint app, ApexOS) ignore CORS entirely. A cross-origin browser deployment
    // should add an explicit `CorsLayer` origin allowlist here, never `allow_origin(Any)`.

    // The request span records method + path only — never the query string, which
    // can carry a `?token=` LAN token.
    let trace = TraceLayer::new_for_http().make_span_with(
        |req: &axum::http::Request<axum::body::Body>| {
            tracing::info_span!("request", method = %req.method(), path = %req.uri().path())
        },
    );

    let app = Router::new()
        .merge(routes::public_router())
        .merge(api_router(state.clone()))
        .merge(static_files::static_router())
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(trace);

    let addr: SocketAddr = opts
        .bind
        .parse()
        .with_context(|| format!("invalid bind address: {}", opts.bind))?;
    info!(%addr, "imaginarium listening (API + UI)");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Connect-info exposes the real peer address to the auth middleware so the
    // localhost bypass can require a genuinely-loopback peer.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
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

/// Resolve the `index`-th content file for a job (index 0 = the historic
/// first-media-file behavior). Delegates to the library's traversal-guarded
/// resolver so the content route, craft renders, and `library:` MediaRefs all
/// share one walk.
pub fn job_content_path(library_root: &Path, job_id: &str, index: u32) -> Option<PathBuf> {
    imaginarium_core::library::resolve_job_asset(library_root, job_id, index)
}
