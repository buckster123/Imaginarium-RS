//! Per-identity token-bucket for paid (xAI-hitting) requests.
//!
//! Independent of `[limits]` USD spend caps. Spend caps stop a large invoice;
//! this stops a cheap high-QPS loop from slamming the upstream quota.
//!
//! Keys (callers choose):
//! - `minted:{token_id}` — one LAN token
//! - `node` — `IMAGINARIUM_TOKEN` / env node secret
//! - `local` — in-process CLI / MCP (no LAN token)
//!
//! `paid_rpm = 0` disables. Process-local (not shared across `serve` vs `mcp`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::config::LimitsConfig;
use crate::error::{Error, Result};

/// Default refill when `[limits].paid_rpm` is omitted.
pub const DEFAULT_PAID_RPM: u32 = 30;
/// Default burst capacity when `[limits].paid_burst` is omitted.
pub const DEFAULT_PAID_BURST: u32 = 10;

/// POST paths that create an upstream Imagine job (not craft / import / estimate / wait).
pub fn is_paid_upstream(method: &str, path: &str) -> bool {
    if !method.eq_ignore_ascii_case("POST") {
        return false;
    }
    matches!(
        path,
        "/v1/images/generations"
            | "/v1/images/edits"
            | "/v1/videos/generations"
            | "/v1/videos/edits"
            | "/v1/videos/extensions"
    )
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Inner>,
}

struct Inner {
    rpm: u32,
    burst: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    /// `None` when `paid_rpm` is 0 (off). Burst of 0 is treated as 1.
    pub fn from_limits(limits: &LimitsConfig) -> Option<Self> {
        if limits.paid_rpm == 0 {
            return None;
        }
        Some(Self {
            inner: Arc::new(Inner {
                rpm: limits.paid_rpm,
                burst: f64::from(limits.paid_burst.max(1)),
                buckets: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Take one token for `key`. Err is [`Error::RateLimit`] with `retry_after_s`.
    pub fn check(&self, key: &str) -> Result<()> {
        self.check_at(key, Instant::now())
    }

    fn check_at(&self, key: &str, now: Instant) -> Result<()> {
        let mut map = self.inner.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let burst = self.inner.burst;
        let rpm = f64::from(self.inner.rpm);
        let per_sec = rpm / 60.0;

        let bucket = map.entry(key.to_string()).or_insert(Bucket {
            tokens: burst,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.last = now;
        bucket.tokens = (bucket.tokens + elapsed * per_sec).min(burst);
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            return Ok(());
        }
        let need = 1.0 - bucket.tokens;
        let retry = (need / per_sec).ceil() as u64;
        Err(Error::rate_limit(retry.max(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn limiter(rpm: u32, burst: u32) -> RateLimiter {
        RateLimiter::from_limits(&LimitsConfig {
            paid_rpm: rpm,
            paid_burst: burst,
            ..Default::default()
        })
        .expect("rpm > 0")
    }

    #[test]
    fn zero_rpm_is_off() {
        assert!(RateLimiter::from_limits(&LimitsConfig {
            paid_rpm: 0,
            paid_burst: 10,
            ..Default::default()
        })
        .is_none());
    }

    #[test]
    fn burst_then_reject() {
        let lim = limiter(30, 2);
        assert!(lim.check("a").is_ok());
        assert!(lim.check("a").is_ok());
        let err = lim.check("a").unwrap_err();
        match err {
            Error::RateLimit { retry_after_s } => assert!(retry_after_s >= 1),
            other => panic!("expected RateLimit, got {other}"),
        }
    }

    #[test]
    fn keys_are_independent() {
        let lim = limiter(30, 1);
        assert!(lim.check("minted:one").is_ok());
        assert!(lim.check("minted:two").is_ok());
        assert!(lim.check("minted:one").is_err());
        assert!(lim.check("minted:two").is_err());
    }

    #[test]
    fn refill_allows_after_wait() {
        let lim = limiter(60, 1);
        assert!(lim.check("k").is_ok());
        assert!(lim.check("k").is_err());
        let now = Instant::now() + Duration::from_millis(1100);
        assert!(lim.check_at("k", now).is_ok());
    }

    #[test]
    fn paid_paths_only() {
        assert!(is_paid_upstream("POST", "/v1/images/generations"));
        assert!(is_paid_upstream("post", "/v1/videos/extensions"));
        assert!(!is_paid_upstream("GET", "/v1/images/generations"));
        assert!(!is_paid_upstream("POST", "/v1/jobs/abc/wait"));
        assert!(!is_paid_upstream("POST", "/v1/craft/video/render"));
        assert!(!is_paid_upstream("POST", "/v1/estimate"));
        assert!(!is_paid_upstream("POST", "/v1/library/import"));
        assert!(!is_paid_upstream("GET", "/v1/jobs/abc"));
    }
}
