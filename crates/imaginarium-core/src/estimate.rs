//! Cost estimator (approximate; not a billing API).

use serde::{Deserialize, Serialize};

use crate::models::{self, ModelId};

/// Official Video 1.5 output rates (xAI model card, 2026-08).
pub const VIDEO15_USD_480P: f64 = 0.08;
pub const VIDEO15_USD_720P: f64 = 0.14;
pub const VIDEO15_USD_1080P: f64 = 0.25;

/// Legacy `grok-imagine-video` (flat-ish; 720p is the top end).
pub const VIDEO_USD_480P: f64 = 0.05;
pub const VIDEO_USD_720P: f64 = 0.07;

/// Studio / config default when the caller omitted resolution.
pub const DEFAULT_VIDEO_RES: &str = "720p";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub model: String,
    pub unit: String,
    pub units: f64,
    pub estimated_usd: f64,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

pub fn estimate_image(model: ModelId, n: u32) -> CostEstimate {
    let info = models::get(model);
    let units = f64::from(n.max(1));
    CostEstimate {
        model: model.as_str().into(),
        unit: info.unit.into(),
        units,
        estimated_usd: info.approx_usd_unit * units,
        note: "approx output cost only; input image fees may apply".into(),
        resolution: None,
    }
}

/// `resolution` is `480p` / `720p` / `1080p`. Omitted → [`DEFAULT_VIDEO_RES`]
/// (720p — matches config + studio default; do not quote the 480p floor).
pub fn estimate_video(model: ModelId, duration_s: u32, resolution: Option<&str>) -> CostEstimate {
    let info = models::get(model);
    let units = f64::from(duration_s.max(1));
    let (res, assumed) = normalize_video_res(resolution);
    let per_sec = video_usd_per_sec(model, res);
    let assumed_note = if assumed {
        "; resolution omitted, assumed 720p"
    } else {
        ""
    };
    CostEstimate {
        model: model.as_str().into(),
        unit: info.unit.into(),
        units,
        estimated_usd: per_sec * units,
        note: format!("approx ${per_sec:.2}/s at {res}{assumed_note}; input fees extra"),
        resolution: Some(res.into()),
    }
}

fn normalize_video_res(resolution: Option<&str>) -> (&'static str, bool) {
    match resolution.map(str::trim).filter(|s| !s.is_empty()) {
        None => (DEFAULT_VIDEO_RES, true),
        Some(s) => match s.to_ascii_lowercase().as_str() {
            "480p" | "480" => ("480p", false),
            "720p" | "720" => ("720p", false),
            "1080p" | "1080" => ("1080p", false),
            _ => (DEFAULT_VIDEO_RES, true),
        },
    }
}

pub fn video_usd_per_sec(model: ModelId, resolution: &str) -> f64 {
    match model {
        ModelId::Video15 => match resolution {
            "1080p" => VIDEO15_USD_1080P,
            "720p" => VIDEO15_USD_720P,
            _ => VIDEO15_USD_480P,
        },
        ModelId::Video => match resolution {
            "720p" | "1080p" => VIDEO_USD_720P,
            _ => VIDEO_USD_480P,
        },
        other => models::get(other).approx_usd_unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video15_1080p_is_not_the_480p_floor() {
        let cheap = estimate_video(ModelId::Video15, 8, Some("480p"));
        let mid = estimate_video(ModelId::Video15, 8, Some("720p"));
        let hi = estimate_video(ModelId::Video15, 8, Some("1080p"));
        assert!((cheap.estimated_usd - 8.0 * VIDEO15_USD_480P).abs() < 1e-9);
        assert!((mid.estimated_usd - 8.0 * VIDEO15_USD_720P).abs() < 1e-9);
        assert!((hi.estimated_usd - 8.0 * VIDEO15_USD_1080P).abs() < 1e-9);
        assert!(hi.estimated_usd > mid.estimated_usd);
        assert!(mid.estimated_usd > cheap.estimated_usd);
        assert_eq!(hi.resolution.as_deref(), Some("1080p"));
    }

    #[test]
    fn omitted_resolution_assumes_720p() {
        let e = estimate_video(ModelId::Video15, 10, None);
        assert!((e.estimated_usd - 10.0 * VIDEO15_USD_720P).abs() < 1e-9);
        assert!(e.note.contains("assumed 720p"));
    }

    #[test]
    fn legacy_720p_is_above_480p() {
        let lo = estimate_video(ModelId::Video, 6, Some("480p"));
        let hi = estimate_video(ModelId::Video, 6, Some("720p"));
        assert!((lo.estimated_usd - 6.0 * VIDEO_USD_480P).abs() < 1e-9);
        assert!((hi.estimated_usd - 6.0 * VIDEO_USD_720P).abs() < 1e-9);
    }
}
