//! Auth middleware — ApexOS agentd-compatible token gate.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use imaginarium_core::tokens::{extract_presented_token, AuthIdentity, TokenScope};
use imaginarium_core::{is_loopback_bind, PRODUCT, VERSION};

use crate::AppState;

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
            if id.scope.allows(required) {
                req.extensions_mut().insert(RequestAuth(id));
                return next.run(req).await;
            }
            return (StatusCode::FORBIDDEN, "insufficient token scope").into_response();
        }
        return (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    }

    // Optional localhost bypass (explicit flag only).
    if state.cfg.server.allow_localhost_no_auth {
        // Cannot easily know peer IP in middleware without ConnectInfo; bypass is
        // only safe when bind is loopback — enforce that here.
        if is_loopback_bind(&state.cfg.server.bind) {
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
