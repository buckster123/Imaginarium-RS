//! HTTP route handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use imaginarium_core::client::{
    ImageEditRequest, ImageGenerateRequest, ResponseFormat, VideoEditRequest, VideoExtendRequest,
    VideoGenerateRequest,
};
use imaginarium_core::estimate;
use imaginarium_core::jobs::JobStore;
use imaginarium_core::library::media_from_node_input;
use imaginarium_core::models::{
    parse_model_selector, parse_optional_image_quality, parse_reference_audios,
    validate_image_quality, ModelId,
};
use imaginarium_core::tokens::TokenScope;
use imaginarium_core::types::{JobId, JobMode, JobResult, JobStatus, MediaRef};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{health, require_auth};
use crate::{job_content_path, AppState};

pub fn public_router() -> Router {
    Router::new().route("/health", get(health))
}

pub fn api_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/estimate", post(estimate_handler))
        .route("/v1/images/generations", post(image_gen))
        .route("/v1/images/edits", post(image_edit))
        .route("/v1/videos/generations", post(video_gen))
        .route("/v1/videos/edits", post(video_edit))
        .route("/v1/videos/extensions", post(video_extend))
        .route("/v1/jobs", get(jobs_list))
        .route("/v1/jobs/{id}", get(jobs_get))
        .route("/v1/jobs/{id}/wait", post(jobs_wait))
        .route("/v1/library/{id}/content", get(library_content))
        .route("/v1/library/{id}/thumb", get(library_thumb))
        .route("/v1/library/import", post(library_import))
        .route("/v1/craft/video/render", post(craft_video_render))
        .route("/v1/tokens", get(tokens_list).post(tokens_create))
        .route("/v1/tokens/{id}", delete(tokens_revoke))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ))
        .with_state(state)
}

fn err_response(status: StatusCode, msg: impl ToString) -> Response {
    (
        status,
        Json(json!({ "ok": false, "error": msg.to_string() })),
    )
        .into_response()
}

fn client_err(e: imaginarium_core::Error) -> Response {
    use imaginarium_core::Error as E;
    let status = match e {
        E::RateLimit { retry_after_s } => {
            return crate::auth::rate_limit_response(retry_after_s);
        }
        E::InvalidMode(_) | E::SpendLimit(_) => StatusCode::BAD_REQUEST,
        E::Forbidden(_) => StatusCode::FORBIDDEN,
        E::JobNotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_GATEWAY,
    };
    err_response(status, e)
}

async fn models() -> impl IntoResponse {
    Json(imaginarium_core::client::models_table_json())
}

#[derive(Debug, Deserialize)]
struct EstimateBody {
    kind: String,
    model: Option<String>,
    n: Option<u32>,
    duration: Option<u32>,
    /// Video only: 480p | 720p | 1080p. Omitted → 720p (studio/config default).
    resolution: Option<String>,
}

async fn estimate_handler(Json(body): Json<EstimateBody>) -> Response {
    let mid = match parse_model_selector(body.model.as_deref()) {
        Ok(Some(m)) => m,
        Ok(None) if body.kind == "video" => ModelId::Video15,
        Ok(None) => ModelId::Image,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let est = match body.kind.as_str() {
        "video" => {
            estimate::estimate_video(mid, body.duration.unwrap_or(8), body.resolution.as_deref())
        }
        _ => estimate::estimate_image(mid, body.n.unwrap_or(1)),
    };
    Json(est).into_response()
}

#[derive(Debug, Deserialize)]
struct ImageGenBody {
    prompt: String,
    model: Option<String>,
    n: Option<u32>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    /// Image 2.0 only: `low` | `medium`.
    quality: Option<String>,
}

async fn image_gen(State(state): State<AppState>, Json(body): Json<ImageGenBody>) -> Response {
    let model = match parse_model_selector(body.model.as_deref()) {
        Ok(m) => m.unwrap_or(ModelId::Image),
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let quality = match parse_optional_image_quality(body.quality.as_deref()) {
        Ok(q) => q,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    if let Err(e) = validate_image_quality(model, quality) {
        return err_response(StatusCode::BAD_REQUEST, e);
    }
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match state
        .client
        .image_generate(
            ImageGenerateRequest {
                prompt: body.prompt,
                model,
                n: body.n.unwrap_or(1),
                aspect_ratio: body.aspect_ratio,
                resolution: body.resolution,
                quality,
                response_format: ResponseFormat::Url,
            },
            &state.library,
            Some(&jobs),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => client_err(e),
    }
}

#[derive(Debug, Deserialize)]
struct ImageEditBody {
    prompt: String,
    images: Vec<String>,
    model: Option<String>,
    n: Option<u32>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    /// Image 2.0 only: `low` | `medium`.
    quality: Option<String>,
}

async fn image_edit(State(state): State<AppState>, Json(body): Json<ImageEditBody>) -> Response {
    let model = match parse_model_selector(body.model.as_deref()) {
        Ok(m) => m.unwrap_or(ModelId::Image),
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let quality = match parse_optional_image_quality(body.quality.as_deref()) {
        Ok(q) => q,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    if let Err(e) = validate_image_quality(model, quality) {
        return err_response(StatusCode::BAD_REQUEST, e);
    }
    let lib_root = state.cfg.library_dir();
    let refs: Vec<MediaRef> = match body
        .images
        .iter()
        .map(|s| media_from_node_input(s, &lib_root))
        .collect::<std::result::Result<Vec<_>, _>>()
    {
        Ok(r) => r,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match state
        .client
        .image_edit(
            ImageEditRequest {
                prompt: body.prompt,
                model,
                images: refs,
                n: body.n.unwrap_or(1),
                aspect_ratio: body.aspect_ratio,
                resolution: body.resolution,
                quality,
                response_format: ResponseFormat::Url,
            },
            &state.library,
            Some(&jobs),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => client_err(e),
    }
}

#[derive(Debug, Deserialize)]
struct VideoGenBody {
    prompt: Option<String>,
    model: Option<String>,
    image: Option<String>,
    reference_images: Option<Vec<String>>,
    /// Video 1.5 preset voice_ids (max 3). Audio-only R2V is valid.
    reference_audios: Option<Vec<String>>,
    duration: Option<u32>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    /// When true, return after submit without polling.
    no_wait: Option<bool>,
}

async fn video_gen(State(state): State<AppState>, Json(body): Json<VideoGenBody>) -> Response {
    let model = match parse_model_selector(body.model.as_deref()) {
        Ok(m) => m,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    // "auto" / absent -> None -> auto-select by modality (not user-forced).
    let explicit = model.is_some();
    let lib_root = state.cfg.library_dir();
    let refs = match body
        .reference_images
        .unwrap_or_default()
        .iter()
        .map(|s| media_from_node_input(s, &lib_root))
        .collect::<std::result::Result<Vec<_>, _>>()
    {
        Ok(r) => r,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let image = match body
        .image
        .as_deref()
        .map(|s| media_from_node_input(s, &lib_root))
        .transpose()
    {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let reference_audios = match parse_reference_audios(body.reference_audios.unwrap_or_default()) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match state
        .client
        .video_generate(
            VideoGenerateRequest {
                prompt: body.prompt,
                model,
                explicit_model: explicit,
                image,
                reference_images: refs,
                reference_audios,
                duration: body.duration,
                aspect_ratio: body.aspect_ratio,
                resolution: body.resolution,
            },
            &state.library,
            Some(&jobs),
            !body.no_wait.unwrap_or(false),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => client_err(e),
    }
}

#[derive(Debug, Deserialize)]
struct VideoEditBody {
    prompt: String,
    video: String,
    model: Option<String>,
    no_wait: Option<bool>,
}

async fn video_edit(State(state): State<AppState>, Json(body): Json<VideoEditBody>) -> Response {
    let model = match body.model.as_deref() {
        Some(m) => match ModelId::parse(m) {
            Ok(m) => Some(m),
            Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
        },
        None => None,
    };
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let video = match media_from_node_input(&body.video, &state.cfg.library_dir()) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    match state
        .client
        .video_edit(
            VideoEditRequest {
                prompt: body.prompt,
                video,
                model,
            },
            &state.library,
            Some(&jobs),
            !body.no_wait.unwrap_or(false),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => client_err(e),
    }
}

#[derive(Debug, Deserialize)]
struct VideoExtendBody {
    prompt: String,
    video: String,
    duration: Option<u32>,
    model: Option<String>,
    no_wait: Option<bool>,
}

async fn video_extend(
    State(state): State<AppState>,
    Json(body): Json<VideoExtendBody>,
) -> Response {
    let model = match body.model.as_deref() {
        Some(m) => match ModelId::parse(m) {
            Ok(m) => Some(m),
            Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
        },
        None => None,
    };
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let video = match media_from_node_input(&body.video, &state.cfg.library_dir()) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    match state
        .client
        .video_extend(
            VideoExtendRequest {
                prompt: body.prompt,
                video,
                duration: body.duration,
                model,
            },
            &state.library,
            Some(&jobs),
            !body.no_wait.unwrap_or(false),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => client_err(e),
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<usize>,
}

async fn jobs_list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match jobs.list_recent(q.limit.unwrap_or(imaginarium_core::jobs::JOB_LIST_DEFAULT)) {
        Ok(items) => Json(items).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn jobs_get(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let job = match jobs.get(&JobId(id.clone())) {
        Ok(Some(j)) => j,
        Ok(None) => return err_response(StatusCode::NOT_FOUND, format!("job not found: {id}")),
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    // Local MCP already polls via video_status_once. HTTP GET (and the MCP
    // proxy that just GETs this route) used to return the stale DB row, so
    // no_wait video never completed over the fat-node path.
    let should_poll = job.upstream_request_id.is_some()
        && matches!(
            job.mode,
            JobMode::VideoGenerate | JobMode::VideoEdit | JobMode::VideoExtend
        )
        && !matches!(
            job.status,
            JobStatus::Done | JobStatus::Failed | JobStatus::Expired | JobStatus::Cancelled
        );
    if !should_poll {
        return Json(job).into_response();
    }
    match state
        .client
        .video_status_once(&job.job_id, &state.library, &jobs)
        .await
    {
        Ok(j) => Json(j).into_response(),
        Err(e) => {
            // Keep the last known row — a transient poll error must not hide
            // a job the caller already paid for.
            tracing::warn!(job_id = %id, "jobs_get poll failed, returning last row: {e}");
            Json(job).into_response()
        }
    }
}

async fn jobs_wait(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let job_id = JobId(id.clone());
    // Unknown id -> 404; an already-terminal job (e.g. a synchronous image job with
    // no upstream video to poll) -> return it as-is rather than a 502.
    match jobs.get(&job_id) {
        Ok(None) => return err_response(StatusCode::NOT_FOUND, format!("job not found: {id}")),
        Ok(Some(j))
            if matches!(
                j.status,
                JobStatus::Done | JobStatus::Failed | JobStatus::Expired | JobStatus::Cancelled
            ) =>
        {
            return Json(j).into_response();
        }
        // A craft job is advanced by the local render task, not xAI — its DB
        // row IS the truth; return it and let the caller poll.
        Ok(Some(j)) if matches!(j.mode, JobMode::CraftExport) => {
            return Json(j).into_response();
        }
        Ok(Some(_)) => {}
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
    match state
        .client
        .video_wait(&job_id, &state.library, &jobs)
        .await
    {
        Ok(j) => Json(j).into_response(),
        Err(e) => {
            let status = if matches!(e, imaginarium_core::error::Error::JobNotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_GATEWAY
            };
            err_response(status, e)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ContentQuery {
    /// Asset index within the job (n>1 image batches) — default 0, the
    /// historic first-file behavior.
    i: Option<u32>,
}

async fn library_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ContentQuery>,
) -> Response {
    // id is a job/asset id; reject anything that could traverse the library root.
    if !imaginarium_core::library::is_safe_asset_id(&id) {
        return err_response(StatusCode::BAD_REQUEST, "invalid asset id");
    }
    let root = state.cfg.library_dir();
    match job_content_path(&root, &id, q.i.unwrap_or(0)) {
        Some(path) => match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let ct = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| imaginarium_core::library::mime_for_ext(&e.to_ascii_lowercase()))
                    .unwrap_or("application/octet-stream");
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, ct)],
                    bytes,
                )
                    .into_response()
            }
            Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
        },
        None => err_response(StatusCode::NOT_FOUND, "asset not found"),
    }
}

/// Serve (or lazily build) the 480px poster for a job's first asset.
async fn library_thumb(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !imaginarium_core::library::is_safe_asset_id(&id) {
        return err_response(StatusCode::BAD_REQUEST, "invalid asset id");
    }
    let root = state.cfg.library_dir();
    let Some(media) = job_content_path(&root, &id, 0) else {
        return err_response(StatusCode::NOT_FOUND, "asset not found");
    };
    let Some(dir) = media.parent().map(|p| p.to_path_buf()) else {
        return err_response(StatusCode::NOT_FOUND, "asset not found");
    };
    let thumb = dir.join("thumb.jpg");
    let missing = std::fs::metadata(&thumb)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    if missing {
        // Lazy backfill — covers assets that predate eager thumbs and any
        // import where the eager pass missed.
        let (m, t) = (media.clone(), thumb.clone());
        let built = tokio::task::spawn_blocking(move || {
            imaginarium_core::craft_video::generate_thumb(&m, &t)
        })
        .await;
        match built {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return err_response(StatusCode::NOT_FOUND, format!("no thumbnail: {e}")),
            Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }
    match tokio::fs::read(&thumb).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
            bytes,
        )
            .into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Debug, Deserialize)]
struct LibraryImportBody {
    /// data URL or raw base64
    data: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    source_job_id: Option<String>,
}

async fn library_import(
    State(state): State<AppState>,
    Json(body): Json<LibraryImportBody>,
) -> Response {
    if imaginarium_core::library::estimate_b64_decoded_len(&body.data)
        > imaginarium_core::library::IMPORT_MAX_BYTES
    {
        return err_response(StatusCode::PAYLOAD_TOO_LARGE, "max 40MB import");
    }
    let (bytes, ext) = match imaginarium_core::library::decode_data_url_or_b64(&body.data) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    if bytes.len() > imaginarium_core::library::IMPORT_MAX_BYTES {
        return err_response(StatusCode::PAYLOAD_TOO_LARGE, "max 40MB import");
    }
    let filename = body.filename.unwrap_or_else(|| format!("import.{ext}"));
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match state.library.import_bytes(
        &jobs,
        &bytes,
        &filename,
        body.note.as_deref(),
        body.source_job_id.as_deref(),
        None,
    ) {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Debug, Deserialize)]
struct CraftQuery {
    /// `?no_wait=true` returns a pending craft job immediately; poll it with
    /// `GET /v1/jobs/{id}` (or `/wait`) like any other job. Default = block
    /// until the render lands (the historic behavior).
    no_wait: Option<bool>,
}

async fn craft_video_render(
    State(state): State<AppState>,
    Query(q): Query<CraftQuery>,
    Json(body): Json<imaginarium_core::craft_video::VideoTimeline>,
) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let lib_root = state.cfg.library_dir();
    let library = state.library.clone();

    if q.no_wait.unwrap_or(false) {
        let job_id = JobId::new();
        let pending = JobResult::pending(
            job_id.clone(),
            JobMode::CraftExport,
            "local-craft",
            body.note.clone(),
        );
        if let Err(e) = jobs.upsert_result(&pending) {
            return err_response(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
        let db_path = state.cfg.db_path();
        tokio::spawn(async move {
            let jid = job_id.clone();
            let rendered = tokio::task::spawn_blocking(move || {
                imaginarium_core::craft_video::render_timeline(
                    &library,
                    &jobs,
                    &lib_root,
                    &body,
                    Some(jid),
                )
            })
            .await;
            // Success finalizes inside render_timeline (import under job_id);
            // every failure path must flip the row to failed — a craft job
            // must never sit pending forever.
            let err_msg = match rendered {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(e.to_string()),
                Err(e) => Some(format!("render task panicked: {e}")),
            };
            if let Some(msg) = err_msg {
                tracing::warn!(job = %job_id, err = %msg, "async craft render failed");
                if let Ok(store) = JobStore::open(&db_path) {
                    let failed = JobResult::failure(
                        job_id,
                        JobMode::CraftExport,
                        "local-craft",
                        "craft",
                        msg,
                    );
                    let _ = store.upsert_result(&failed);
                }
            }
        });
        return Json(pending).into_response();
    }

    // Synchronous path — ffmpeg is blocking, run off the async worker.
    let result = tokio::task::spawn_blocking(move || {
        imaginarium_core::craft_video::render_timeline(&library, &jobs, &lib_root, &body, None)
    })
    .await;
    match result {
        Ok(Ok(job)) => Json(job).into_response(),
        Ok(Err(e)) => err_response(StatusCode::BAD_REQUEST, e),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct TokenCreateBody {
    label: String,
    scope: Option<String>,
}

async fn tokens_list(State(state): State<AppState>) -> Response {
    let store = state.tokens.lock().await;
    match store.list() {
        Ok(list) => {
            // Redact hashes — serialize public fields only.
            let pub_list: Vec<Value> = list
                .into_iter()
                .map(|t| {
                    json!({
                        "id": t.id,
                        "label": t.label,
                        "scope": t.scope,
                        "created_at": t.created_at,
                    })
                })
                .collect();
            Json(json!({ "tokens": pub_list })).into_response()
        }
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn tokens_create(
    State(state): State<AppState>,
    Json(body): Json<TokenCreateBody>,
) -> Response {
    let scope = match TokenScope::parse(body.scope.as_deref().unwrap_or("write")) {
        Ok(s) => s,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let store = state.tokens.lock().await;
    match store.mint(&body.label, scope) {
        Ok(m) => Json(m).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn tokens_revoke(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let store = state.tokens.lock().await;
    match store.revoke(&id) {
        Ok(true) => Json(json!({ "ok": true, "revoked": id })).into_response(),
        Ok(false) => err_response(StatusCode::NOT_FOUND, "token not found"),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

// end routes
