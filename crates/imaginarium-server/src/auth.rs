//! Auth middleware — ApexOS agentd-compatible token gate.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::RETRY_AFTER;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use imaginarium_core::rate_limit::is_paid_upstream;
use imaginarium_core::tokens::{extract_presented_token, AuthIdentity, TokenScope};
use imaginarium_core::{PRODUCT, VERSION};
use serde_json::json;

use crate::AppState;

pub(crate) fn rate_limit_response(retry_after_s: u64) -> Response {
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({
            "ok": false,
            "error_type": "rate_limit",
            "error": format!("paid-request rate limit — retry in {retry_after_s}s"),
            "retry_after_s": retry_after_s,
        })),
    )
        .into_response();
    if let Ok(val) = retry_after_s.to_string().parse() {
        res.headers_mut().insert(RETRY_AFTER, val);
    }
    res
}

#[derive(Clone)]
#[allow(dead_code)] // reserved for handlers that need identity
pub struct RequestAuth(pub AuthIdentity);

pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let required = required_scope(req.uri().path(), req.method().as_str());

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let x_token = req
        .headers()
        .get("x-imaginarium-token")
        .and_then(|v| v.to_str().ok());
    let query = req.uri().query();

    let presented = extract_presented_token(auth_header, x_token, query);

    if let Some(token) = presented {
        let identity = {
            let store = state.tokens.lock().await;
            match store.verify(&token) {
                Ok(id) => id,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            }
        };
        if let Some(id) = identity {
            if !id.scope.allows(required) {
                return (StatusCode::FORBIDDEN, "insufficient token scope").into_response();
            }
            if is_paid_upstream(req.method().as_str(), req.uri().path()) {
                if let (Some(lim), Some(key)) = (state.rate_limiter.as_ref(), id.rate_key()) {
                    if let Err(imaginarium_core::Error::RateLimit { retry_after_s }) =
                        lim.check(&key)
                    {
                        return rate_limit_response(retry_after_s);
                    }
                }
            }
            req.extensions_mut().insert(RequestAuth(id));
            return next.run(req).await;
        }
        return (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    }

    // Optional localhost bypass — explicit opt-in flag AND a genuinely-loopback peer.
    // The decision is gated on the REAL peer address (via `ConnectInfo`), not the
    // configured bind string, so it cannot be tricked by config and cannot fire for a
    // remote client. If `ConnectInfo` is absent (e.g. an embedder mounted `api_router`
    // without `into_make_service_with_connect_info`), the bypass fails closed.
    if state.allow_localhost_no_auth {
        let peer_is_loopback = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().is_loopback())
            .unwrap_or(false);
        if peer_is_loopback {
            req.extensions_mut().insert(RequestAuth(AuthIdentity {
                source: imaginarium_core::AuthSource::LocalhostBypass,
                label: "localhost".into(),
                scope: TokenScope::Admin,
                token_id: None,
            }));
            return next.run(req).await;
        }
    }

    (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response()
}

fn required_scope(path: &str, method: &str) -> TokenScope {
    if path.starts_with("/v1/tokens") {
        return TokenScope::Admin;
    }
    if method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD") {
        return TokenScope::Read;
    }
    // POST wait is read-ish but still a job action — allow write
    if path.contains("/wait") {
        return TokenScope::Read;
    }
    TokenScope::Write
}

pub async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "ok": true,
        "product": PRODUCT,
        "version": VERSION,
    }))
}
