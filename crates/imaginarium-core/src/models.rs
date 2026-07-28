//! Model catalog and capability matrix — single source of truth.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Known Imagine model identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    #[serde(rename = "grok-imagine-image")]
    Image,
    #[serde(rename = "grok-imagine-image-quality")]
    ImageQuality,
    #[serde(rename = "grok-imagine-video")]
    Video,
    #[serde(rename = "grok-imagine-video-1.5")]
    Video15,
}

impl ModelId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "grok-imagine-image",
            Self::ImageQuality => "grok-imagine-image-quality",
            Self::Video => "grok-imagine-video",
            Self::Video15 => "grok-imagine-video-1.5",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "grok-imagine-image" | "image" => Ok(Self::Image),
            "grok-imagine-image-quality" | "image-quality" | "quality" => Ok(Self::ImageQuality),
            "grok-imagine-video" | "video" => Ok(Self::Video),
            "grok-imagine-video-1.5" | "video-1.5" | "1.5" | "i2v" => Ok(Self::Video15),
            other => Err(Error::invalid_mode(format!("unknown model: {other}"))),
        }
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub text_to_image: bool,
    pub image_edit: bool,
    pub text_to_video: bool,
    pub image_to_video: bool,
    pub reference_to_video: bool,
    pub video_edit: bool,
    pub video_extend: bool,
    pub max_image_resolution: Option<&'static str>,
    pub max_video_resolution: Option<&'static str>,
    pub max_duration_s: Option<u32>,
    pub max_source_images: Option<u32>,
    pub max_reference_images: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub display: &'static str,
    pub kind: &'static str,
    pub notes: &'static str,
    /// Approximate output USD (image per image, video per second @ default tier).
    pub approx_usd_unit: f64,
    pub unit: &'static str,
    pub capabilities: ModelCapabilities,
}

/// Full static catalog.
pub fn catalog() -> &'static [ModelInfo] {
    &CATALOG
}

pub fn get(id: ModelId) -> &'static ModelInfo {
    CATALOG
        .iter()
        .find(|m| m.id == id.as_str())
        .expect("catalog must contain all ModelId variants")
}

static CATALOG: [ModelInfo; 4] = [
    ModelInfo {
        id: "grok-imagine-image",
        display: "Grok Imagine Image",
        kind: "image",
        notes: "Fast text-to-image and edits; cheaper tier.",
        approx_usd_unit: 0.02,
        unit: "image",
        capabilities: ModelCapabilities {
            text_to_image: true,
            image_edit: true,
            text_to_video: false,
            image_to_video: false,
            reference_to_video: false,
            video_edit: false,
            video_extend: false,
            max_image_resolution: Some("2k"),
            max_video_resolution: None,
            max_duration_s: None,
            max_source_images: Some(3),
            max_reference_images: None,
        },
    },
    ModelInfo {
        id: "grok-imagine-image-quality",
        display: "Grok Imagine Image Quality",
        kind: "image",
        notes: "Higher fidelity stills and edits.",
        approx_usd_unit: 0.05,
        unit: "image",
        capabilities: ModelCapabilities {
            text_to_image: true,
            image_edit: true,
            text_to_video: false,
            image_to_video: false,
            reference_to_video: false,
            video_edit: false,
            video_extend: false,
            max_image_resolution: Some("2k"),
            max_video_resolution: None,
            max_duration_s: None,
            max_source_images: Some(3),
            max_reference_images: None,
        },
    },
    ModelInfo {
        id: "grok-imagine-video",
        display: "Grok Imagine Video",
        kind: "video",
        notes: "T2V, R2V, edit, extend; legacy I2V. Max 720p.",
        approx_usd_unit: 0.05,
        unit: "second",
        capabilities: ModelCapabilities {
            text_to_image: false,
            image_edit: false,
            text_to_video: true,
            image_to_video: true,
            reference_to_video: true,
            video_edit: true,
            video_extend: true,
            max_image_resolution: None,
            max_video_resolution: Some("720p"),
            max_duration_s: Some(15),
            max_source_images: None,
            max_reference_images: Some(7),
        },
    },
    ModelInfo {
        id: "grok-imagine-video-1.5",
        display: "Grok Imagine Video 1.5",
        kind: "video",
        notes: "Image-to-video only. Only model with 1080p.",
        approx_usd_unit: 0.08,
        unit: "second",
        capabilities: ModelCapabilities {
            text_to_image: false,
            image_edit: false,
            text_to_video: false,
            image_to_video: true,
            reference_to_video: false,
            video_edit: false,
            video_extend: false,
            max_image_resolution: None,
            max_video_resolution: Some("1080p"),
            max_duration_s: Some(15),
            max_source_images: None,
            max_reference_images: None,
        },
    },
];

/// Supported image aspect ratios (from xAI docs).
pub const IMAGE_ASPECT_RATIOS: &[&str] = &[
    "1:1", "3:4", "4:3", "9:16", "16:9", "2:3", "3:2", "9:19.5", "19.5:9", "9:20", "20:9", "1:2",
    "2:1", "auto",
];

pub const VIDEO_ASPECT_RATIOS: &[&str] = &["1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3"];

pub const IMAGE_RESOLUTIONS: &[&str] = &["1k", "2k"];
pub const VIDEO_RESOLUTIONS: &[&str] = &["480p", "720p", "1080p"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMode {
    TextToVideo,
    ImageToVideo,
    ReferenceToVideo,
}

/// Validate a video generation request against the capability matrix.
pub fn validate_video_generate(
    model: ModelId,
    mode: VideoMode,
    resolution: Option<&str>,
    duration: Option<u32>,
    reference_count: usize,
) -> Result<()> {
    let info = get(model);
    let caps = &info.capabilities;

    match mode {
        VideoMode::TextToVideo if !caps.text_to_video => {
            return Err(Error::invalid_mode(format!(
                "{} does not support text-to-video",
                model.as_str()
            )));
        }
        VideoMode::ImageToVideo if !caps.image_to_video => {
            return Err(Error::invalid_mode(format!(
                "{} does not support image-to-video",
                model.as_str()
            )));
        }
        VideoMode::ReferenceToVideo if !caps.reference_to_video => {
            return Err(Error::invalid_mode(format!(
                "{} does not support reference-to-video",
                model.as_str()
            )));
        }
        _ => {}
    }

    if let Some(max_refs) = caps.max_reference_images {
        if reference_count as u32 > max_refs {
            return Err(Error::invalid_mode(format!(
                "at most {max_refs} reference images allowed"
            )));
        }
    } else if reference_count > 0 && mode == VideoMode::ReferenceToVideo {
        return Err(Error::invalid_mode(format!(
            "{} does not accept reference images",
            model.as_str()
        )));
    }

    if let Some(res) = resolution {
        if !VIDEO_RESOLUTIONS.contains(&res) {
            return Err(Error::invalid_mode(format!(
                "invalid video resolution: {res}"
            )));
        }
        if res == "1080p" && model != ModelId::Video15 {
            return Err(Error::invalid_mode(
                "1080p is only supported on grok-imagine-video-1.5 (image-to-video)",
            ));
        }
        if let Some(max) = caps.max_video_resolution {
            let rank = |r: &str| match r {
                "480p" => 1,
                "720p" => 2,
                "1080p" => 3,
                _ => 0,
            };
            if rank(res) > rank(max) {
                return Err(Error::invalid_mode(format!(
                    "{} max resolution is {max}",
                    model.as_str()
                )));
            }
        }
    }

    if let Some(d) = duration {
        if d < 1 {
            return Err(Error::invalid_mode("duration must be >= 1"));
        }
        if let Some(max) = caps.max_duration_s {
            if d > max {
                return Err(Error::invalid_mode(format!("duration max is {max}s")));
            }
        }
    }

    Ok(())
}

/// Pick default video model from modality when user did not force a model.
pub fn default_video_model_for(mode: VideoMode) -> ModelId {
    match mode {
        VideoMode::ImageToVideo => ModelId::Video15,
        VideoMode::TextToVideo | VideoMode::ReferenceToVideo => ModelId::Video,
    }
}

pub fn default_image_model() -> ModelId {
    ModelId::Image
}

/// Parse an optional model selector from an API / agent caller.
///
/// `None`, an empty string, and the literal `"auto"` all mean "let the server pick
/// the default for this operation" and return `Ok(None)`. Any other value is parsed
/// as a concrete model (error on unknown). This lets a caller pass `model: "auto"`
/// (as `docs/APEXOS_IMAGINARIUM.md` and the SPA do) instead of getting a 400.
pub fn parse_model_selector(s: Option<&str>) -> Result<Option<ModelId>> {
    match s.map(str::trim) {
        None | Some("") | Some("auto") => Ok(None),
        Some(m) => ModelId::parse(m).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_15_rejects_t2v() {
        let err =
            validate_video_generate(ModelId::Video15, VideoMode::TextToVideo, None, Some(8), 0)
                .unwrap_err();
        assert!(matches!(err, Error::InvalidMode(_)));
    }

    #[test]
    fn only_15_allows_1080p() {
        assert!(validate_video_generate(
            ModelId::Video15,
            VideoMode::ImageToVideo,
            Some("1080p"),
            Some(8),
            0
        )
        .is_ok());
        assert!(validate_video_generate(
            ModelId::Video,
            VideoMode::TextToVideo,
            Some("1080p"),
            Some(8),
            0
        )
        .is_err());
    }

    #[test]
    fn model_aliases_parse() {
        assert_eq!(ModelId::parse("quality").unwrap(), ModelId::ImageQuality);
        assert_eq!(ModelId::parse("1.5").unwrap(), ModelId::Video15);
    }

    #[test]
    fn auto_and_empty_select_no_model() {
        assert_eq!(parse_model_selector(None).unwrap(), None);
        assert_eq!(parse_model_selector(Some("auto")).unwrap(), None);
        assert_eq!(parse_model_selector(Some("  ")).unwrap(), None);
        assert_eq!(
            parse_model_selector(Some("quality")).unwrap(),
            Some(ModelId::ImageQuality)
        );
        assert!(parse_model_selector(Some("nope")).is_err());
    }
}
