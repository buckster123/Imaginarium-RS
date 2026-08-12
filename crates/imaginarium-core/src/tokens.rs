//! Downstream LAN token store — ApexOS-compatible auth surface.
//!
//! Clients may authenticate with:
//! - `Authorization: Bearer <token>`
//! - `X-Imaginarium-Token: <token>`
//! - `?token=<token>` query (browser / WS-friendly, same as agentd)
//!
//! Tokens are stored as SHA-256 hex digests (plaintext shown once at mint).
//! Env `IMAGINARIUM_TOKEN` acts as the node/admin secret (like `AGENTD_TOKEN`)
//! and is compared in constant time without hashing (shared mesh secret).

use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::error::{Error, Result};

/// Scope bits for a minted API token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// Full access including token admin.
    Admin,
    /// Create image/video jobs.
    Write,
    /// Models, jobs, library read.
    Read,
}

impl TokenScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Write => "write",
            Self::Read => "read",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "admin" => Ok(Self::Admin),
            "write" => Ok(Self::Write),
            "read" => Ok(Self::Read),
            other => Err(Error::config(format!("unknown token scope: {other}"))),
        }
    }

    pub fn allows(self, required: TokenScope) -> bool {
        matches!(
            (self, required),
            (Self::Admin, _) | (Self::Write, Self::Write | Self::Read) | (Self::Read, Self::Read)
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenRecord {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub created_at: String,
    /// Never expose hash in normal list JSON.
    #[serde(skip_serializing)]
    pub token_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenMintResult {
    pub id: String,
    pub label: String,
    pub scope: String,
    /// Plaintext — show once.
    pub token: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct AuthIdentity {
    pub source: AuthSource,
    pub label: String,
    pub scope: TokenScope,
    pub token_id: Option<String>,
}

impl AuthIdentity {
    /// Bucket key for the per-token rate limiter. `None` = do not throttle
    /// (localhost bypass — explicit dev opt-in).
    pub fn rate_key(&self) -> Option<String> {
        match self.source {
            AuthSource::Minted => self.token_id.as_deref().map(|id| format!("minted:{id}")),
            AuthSource::NodeEnv => Some("node".into()),
            AuthSource::LocalhostBypass => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSource {
    /// `IMAGINARIUM_TOKEN` / config node token (admin).
    NodeEnv,
    /// Hashed token from DB.
    Minted,
    /// Localhost bypass when explicitly allowed.
    LocalhostBypass,
}

pub struct TokenStore {
    conn: Connection,
    /// Optional shared node secret (plaintext, env).
    node_token: Option<String>,
}

impl TokenStore {
    pub fn open(path: &Path, node_token: Option<String>) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self {
            conn,
            node_token: node_token.filter(|s| !s.trim().is_empty()),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                scope TEXT NOT NULL,
                token_hash TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                revoked_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tokens_hash ON api_tokens(token_hash);
            "#,
        )?;
        Ok(())
    }

    pub fn has_any_auth(&self) -> Result<bool> {
        if self.node_token.is_some() {
            return Ok(true);
        }
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM api_tokens WHERE revoked_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn mint(&self, label: &str, scope: TokenScope) -> Result<TokenMintResult> {
        let id = Ulid::new().to_string();
        let token = format!("img_{}", Ulid::new());
        let hash = hash_token(&token);
        let created = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            INSERT INTO api_tokens (id, label, scope, token_hash, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![id, label, scope.as_str(), hash, created],
        )?;
        Ok(TokenMintResult {
            id,
            label: label.to_string(),
            scope: scope.as_str().into(),
            token,
            created_at: created,
        })
    }

    pub fn list(&self) -> Result<Vec<TokenRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, label, scope, token_hash, created_at
            FROM api_tokens
            WHERE revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TokenRecord {
                id: row.get(0)?,
                label: row.get(1)?,
                scope: row.get(2)?,
                token_hash: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn revoke(&self, id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE api_tokens SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            params![Utc::now().to_rfc3339(), id],
        )?;
        Ok(n > 0)
    }

    /// Verify a presented bearer/query token.
    pub fn verify(&self, presented: &str) -> Result<Option<AuthIdentity>> {
        let presented = presented.trim();
        if presented.is_empty() {
            return Ok(None);
        }

        if let Some(node) = &self.node_token {
            if ct_eq(presented.as_bytes(), node.as_bytes()) {
                return Ok(Some(AuthIdentity {
                    source: AuthSource::NodeEnv,
                    label: "node".into(),
                    scope: TokenScope::Admin,
                    token_id: None,
                }));
            }
        }

        let hash = hash_token(presented);
        let row: Option<(String, String, String)> = self
            .conn
            .query_row(
                r#"
                SELECT id, label, scope FROM api_tokens
                WHERE token_hash = ?1 AND revoked_at IS NULL
                "#,
                params![hash],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        if let Some((id, label, scope)) = row {
            let scope = TokenScope::parse(&scope)?;
            return Ok(Some(AuthIdentity {
                source: AuthSource::Minted,
                label,
                scope,
                token_id: Some(id),
            }));
        }
        Ok(None)
    }
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time equality (ApexOS `tokens_match` pattern).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut v = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        v |= x ^ y;
    }
    v == 0
}

/// Extract token from Authorization Bearer, X-Imaginarium-Token, or ?token=.
pub fn extract_presented_token(
    authorization: Option<&str>,
    x_token: Option<&str>,
    query: Option<&str>,
) -> Option<String> {
    if let Some(a) = authorization {
        if let Some(t) = a
            .strip_prefix("Bearer ")
            .or_else(|| a.strip_prefix("bearer "))
        {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    if let Some(t) = x_token.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(t.to_string());
    }
    if let Some(q) = query {
        for part in q.split('&') {
            if let Some(raw) = part.strip_prefix("token=") {
                let decoded = urlencoding_decode(raw);
                if !decoded.is_empty() {
                    return Some(decoded);
                }
            }
        }
    }
    None
}

fn urlencoding_decode(s: &str) -> String {
    // Minimal percent-decode (ApexOS uses percent_encoding crate; keep deps light).
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = || {
                    let hi = from_hex(bytes[i + 1])?;
                    let lo = from_hex(bytes[i + 2])?;
                    Some((hi << 4) | lo)
                };
                if let Some(b) = h() {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// True if the bind address is loopback-only (127.0.0.1 / ::1 / localhost).
pub fn is_loopback_bind(bind: &str) -> bool {
    let host = bind
        .rsplit_once(':')
        .map(|(h, _)| h.trim_matches(|c| c == '[' || c == ']'))
        .unwrap_or(bind);
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "0:0:0:0:0:0:0:1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn mint_and_verify() {
        let dir = tempdir().unwrap();
        let store = TokenStore::open(&dir.path().join("t.db"), None).unwrap();
        let minted = store.mint("hermes", TokenScope::Write).unwrap();
        let id = store.verify(&minted.token).unwrap().unwrap();
        assert_eq!(id.label, "hermes");
        assert!(id.scope.allows(TokenScope::Write));
        assert!(!id.scope.allows(TokenScope::Admin));
    }

    #[test]
    fn node_env_token() {
        let dir = tempdir().unwrap();
        let store =
            TokenStore::open(&dir.path().join("t.db"), Some("secret-node-token".into())).unwrap();
        let id = store.verify("secret-node-token").unwrap().unwrap();
        assert_eq!(id.source, AuthSource::NodeEnv);
        assert!(id.scope.allows(TokenScope::Admin));
    }

    #[test]
    fn extract_bearer_and_query() {
        assert_eq!(
            extract_presented_token(Some("Bearer abc"), None, None).as_deref(),
            Some("abc")
        );
        assert_eq!(
            extract_presented_token(None, None, Some("foo=1&token=xyz")).as_deref(),
            Some("xyz")
        );
    }

    #[test]
    fn loopback_detect() {
        assert!(is_loopback_bind("127.0.0.1:8791"));
        assert!(!is_loopback_bind("0.0.0.0:8791"));
        assert!(!is_loopback_bind("192.168.0.10:8791"));
    }

    #[test]
    fn rate_key_is_per_token_and_skips_bypass() {
        let minted = AuthIdentity {
            source: AuthSource::Minted,
            label: "hermes".into(),
            scope: TokenScope::Write,
            token_id: Some("01ABC".into()),
        };
        assert_eq!(minted.rate_key().as_deref(), Some("minted:01ABC"));
        let node = AuthIdentity {
            source: AuthSource::NodeEnv,
            label: "node".into(),
            scope: TokenScope::Admin,
            token_id: None,
        };
        assert_eq!(node.rate_key().as_deref(), Some("node"));
        let bypass = AuthIdentity {
            source: AuthSource::LocalhostBypass,
            label: "localhost".into(),
            scope: TokenScope::Admin,
            token_id: None,
        };
        assert!(bypass.rate_key().is_none());
    }
}
