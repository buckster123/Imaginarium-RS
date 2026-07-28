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
use imaginarium_core::models::ModelId;
use imaginarium_core::tokens::TokenScope;
use imaginarium_core::types::{JobId, MediaRef};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{health, require_auth};
use crate::{job_content_path, AppState};

pub fn public_router() -> Router {
    Router::new().route("/health", get(health))
}

pub fn api_router(state: AppState, _allow_localhost_no_auth: bool) -> Router {
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
        .route("/v1/library/import", post(library_import))
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

async fn models() -> impl IntoResponse {
    Json(imaginarium_core::client::models_table_json())
}

#[derive(Debug, Deserialize)]
struct EstimateBody {
    kind: String,
    model: Option<String>,
    n: Option<u32>,
    duration: Option<u32>,
}

async fn estimate_handler(Json(body): Json<EstimateBody>) -> Response {
    let model = body.model.as_deref().unwrap_or(match body.kind.as_str() {
        "video" => "video",
        _ => "image",
    });
    let mid = match ModelId::parse(model) {
        Ok(m) => m,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let est = match body.kind.as_str() {
        "video" => estimate::estimate_video(mid, body.duration.unwrap_or(8)),
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
}

async fn image_gen(State(state): State<AppState>, Json(body): Json<ImageGenBody>) -> Response {
    let model = match ModelId::parse(body.model.as_deref().unwrap_or("image")) {
        Ok(m) => m,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
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
                response_format: ResponseFormat::Url,
            },
            &state.library,
            Some(&jobs),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
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
}

async fn image_edit(State(state): State<AppState>, Json(body): Json<ImageEditBody>) -> Response {
    let model = match ModelId::parse(body.model.as_deref().unwrap_or("image")) {
        Ok(m) => m,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    let refs: Vec<MediaRef> = body
        .images
        .iter()
        .map(|s| MediaRef::from_user_input(s))
        .collect();
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
                response_format: ResponseFormat::Url,
            },
            &state.library,
            Some(&jobs),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
    }
}

#[derive(Debug, Deserialize)]
struct VideoGenBody {
    prompt: Option<String>,
    model: Option<String>,
    image: Option<String>,
    reference_images: Option<Vec<String>>,
    duration: Option<u32>,
    aspect_ratio: Option<String>,
    resolution: Option<String>,
    /// When true, return after submit without polling.
    no_wait: Option<bool>,
}

async fn video_gen(State(state): State<AppState>, Json(body): Json<VideoGenBody>) -> Response {
    let explicit = body.model.is_some();
    let model = match body.model.as_deref() {
        Some(m) => match ModelId::parse(m) {
            Ok(m) => Some(m),
            Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
        },
        None => None,
    };
    let refs = body
        .reference_images
        .unwrap_or_default()
        .iter()
        .map(|s| MediaRef::from_user_input(s))
        .collect();
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
                image: body.image.map(|s| MediaRef::from_user_input(&s)),
                reference_images: refs,
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
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
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
    match state
        .client
        .video_edit(
            VideoEditRequest {
                prompt: body.prompt,
                video: MediaRef::from_user_input(&body.video),
                model,
            },
            &state.library,
            Some(&jobs),
            !body.no_wait.unwrap_or(false),
        )
        .await
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
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
    match state
        .client
        .video_extend(
            VideoExtendRequest {
                prompt: body.prompt,
                video: MediaRef::from_user_input(&body.video),
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
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
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
    match jobs.list_recent(q.limit.unwrap_or(20)) {
        Ok(items) => Json(items).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn jobs_get(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match jobs.get(&JobId(id.clone())) {
        Ok(Some(j)) => Json(j).into_response(),
        Ok(None) => err_response(StatusCode::NOT_FOUND, format!("job not found: {id}")),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn jobs_wait(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let jobs = match JobStore::open(&state.cfg.db_path()) {
        Ok(j) => j,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    match state
        .client
        .video_wait(&JobId(id.clone()), &state.library, &jobs)
        .await
    {
        Ok(j) => Json(j).into_response(),
        Err(e) => err_response(StatusCode::BAD_GATEWAY, e),
    }
}

async fn library_content(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    // id may be job_id; stream first media file.
    let root = state.cfg.library_dir();
    match job_content_path(&root, &id) {
        Some(path) => match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let ct = match path.extension().and_then(|e| e.to_str()) {
                    Some("mp4") => "video/mp4",
                    Some("webm") => "video/webm",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("webp") => "image/webp",
                    _ => "application/octet-stream",
                };
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

async fn library_import(State(state): State<AppState>, Json(body): Json<LibraryImportBody>) -> Response {
    let (bytes, ext) = match imaginarium_core::library::decode_data_url_or_b64(&body.data) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };
    if bytes.len() > 40 * 1024 * 1024 {
        return err_response(StatusCode::PAYLOAD_TOO_LARGE, "max 40MB import");
    }
    let filename = body
        .filename
        .unwrap_or_else(|| format!("import.{ext}"));
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
    ) {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
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
