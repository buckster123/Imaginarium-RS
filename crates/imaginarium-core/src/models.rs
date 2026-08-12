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
    #[serde(rename = "grok-imagine-image-2.0")]
    Image20,
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
            Self::Image20 => "grok-imagine-image-2.0",
            Self::Video => "grok-imagine-video",
            Self::Video15 => "grok-imagine-video-1.5",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "grok-imagine-image" | "image" => Ok(Self::Image),
            "grok-imagine-image-quality" | "image-quality" | "quality" => Ok(Self::ImageQuality),
            "grok-imagine-image-2.0"
            | "grok-imagine-image-2.0-preview"
            | "image-2.0"
            | "image-2"
            | "2.0"
            | "imagine-2" => Ok(Self::Image20),
            "grok-imagine-video" | "video" => Ok(Self::Video),
            "grok-imagine-video-1.5" | "video-1.5" | "1.5" | "i2v" => Ok(Self::Video15),
            other => Err(Error::invalid_mode(format!("unknown model: {other}"))),
        }
    }

    pub fn is_image(self) -> bool {
        matches!(self, Self::Image | Self::ImageQuality | Self::Image20)
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
    /// Preset `voice_id`s on `reference_audios` (Video 1.5 R2V).
    pub max_reference_audios: Option<u32>,
    /// Upstream `quality` (`low` | `medium`) — Image 2.0 only.
    pub quality_param: bool,
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

static CATALOG: [ModelInfo; 5] = [
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
            max_reference_audios: None,
            quality_param: false,
        },
    },
    ModelInfo {
        id: "grok-imagine-image-quality",
        display: "Grok Imagine Image Quality",
        kind: "image",
        notes: "Higher fidelity stills and edits (1.x quality tier).",
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
            max_reference_audios: None,
            quality_param: false,
        },
    },
    ModelInfo {
        id: "grok-imagine-image-2.0",
        display: "Grok Imagine Image 2.0",
        kind: "image",
        notes: "Aug 2026 image model. Optional quality=low|medium (default medium).",
        approx_usd_unit: 0.04,
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
            max_reference_audios: None,
            quality_param: true,
        },
    },
    ModelInfo {
        id: "grok-imagine-video",
        display: "Grok Imagine Video",
        kind: "video",
        notes: "Legacy T2V/I2V/R2V plus edit/extend. Max 720p. No reference_audios.",
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
            max_reference_audios: None,
            quality_param: false,
        },
    },
    ModelInfo {
        id: "grok-imagine-video-1.5",
        display: "Grok Imagine Video 1.5",
        kind: "video",
        notes: "T2V, I2V, R2V. 1080p on T2V/I2V; R2V + voices cap 720p. Preset voice_id via reference_audios (max 3). Output ~$0.08/0.14/0.25 per s at 480/720/1080p.",
        approx_usd_unit: 0.08,
        unit: "second",
        capabilities: ModelCapabilities {
            text_to_image: false,
            image_edit: false,
            text_to_video: true,
            image_to_video: true,
            reference_to_video: true,
            video_edit: false,
            video_extend: false,
            max_image_resolution: None,
            max_video_resolution: Some("1080p"),
            max_duration_s: Some(15),
            max_source_images: None,
            max_reference_images: Some(7),
            max_reference_audios: Some(3),
            quality_param: false,
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
/// Common built-in TTS / Imagine voice ids (case-insensitive). Unknown ids are
/// forwarded upstream; xAI 400s with the live roster.
pub const PRESET_VOICE_IDS: &[&str] = &["ara", "eve", "leo", "rex"];
pub const MAX_REFERENCE_AUDIOS: u32 = 3;

/// Normalize a preset `voice_id`. Empty / junk rejected; unknown names pass.
pub fn parse_voice_id(s: &str) -> Result<String> {
    let t = s.trim().to_ascii_lowercase();
    if t.is_empty() {
        return Err(Error::invalid_mode("voice_id must be non-empty"));
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::invalid_mode(format!("invalid voice_id: {s}")));
    }
    Ok(t)
}

/// Parse caller voice ids: skip blanks, cap at 3, lowercase.
pub fn parse_reference_audios<I, S>(ids: I) -> Result<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Vec::new();
    for s in ids {
        let t = s.as_ref().trim();
        if t.is_empty() {
            continue;
        }
        out.push(parse_voice_id(t)?);
    }
    if out.len() as u32 > MAX_REFERENCE_AUDIOS {
        return Err(Error::invalid_mode(format!(
            "at most {MAX_REFERENCE_AUDIOS} reference voices (tag them <AUDIO_0>… in the prompt)"
        )));
    }
    Ok(out)
}
/// Upstream `quality` values. Only `grok-imagine-image-2.0` accepts this field.
pub const IMAGE_QUALITY_LEVELS: &[&str] = &["low", "medium"];

/// Optional generation-quality knob for Image 2.0 (`quality` in the xAI body).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageQuality {
    Low,
    Medium,
}

impl ImageQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            other => Err(Error::invalid_mode(format!(
                "quality must be low or medium (got {other})"
            ))),
        }
    }
}

/// `None` / empty / `"auto"` → no explicit quality (upstream default is `medium` on 2.0).
pub fn parse_optional_image_quality(s: Option<&str>) -> Result<Option<ImageQuality>> {
    match s.map(str::trim) {
        None | Some("") | Some("auto") => Ok(None),
        Some(q) => ImageQuality::parse(q).map(Some),
    }
}

/// Reject `quality` on models that do not advertise `quality_param`.
pub fn validate_image_quality(model: ModelId, quality: Option<ImageQuality>) -> Result<()> {
    if quality.is_some() && !get(model).capabilities.quality_param {
        return Err(Error::invalid_mode(
            "quality is only supported on grok-imagine-image-2.0",
        ));
    }
    Ok(())
}

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
    audio_count: usize,
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

    if audio_count > 0 {
        match caps.max_reference_audios {
            None => {
                return Err(Error::invalid_mode(format!(
                    "{} does not accept reference_audios (preset voices are Video 1.5 R2V)",
                    model.as_str()
                )));
            }
            Some(max) if audio_count as u32 > max => {
                return Err(Error::invalid_mode(format!(
                    "at most {max} reference voices allowed"
                )));
            }
            Some(_) => {}
        }
    }

    if let Some(res) = resolution {
        if !VIDEO_RESOLUTIONS.contains(&res) {
            return Err(Error::invalid_mode(format!(
                "invalid video resolution: {res}"
            )));
        }
        if res == "1080p" {
            if model != ModelId::Video15 {
                return Err(Error::invalid_mode(
                    "1080p is only supported on grok-imagine-video-1.5 (text-to-video and image-to-video)",
                ));
            }
            if mode == VideoMode::ReferenceToVideo {
                return Err(Error::invalid_mode(
                    "1080p is not supported on reference-to-video (max 720p)",
                ));
            }
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

/// Default generate model when the caller omitted / passed `auto`.
/// T2V, I2V, and R2V all pick Video 1.5. Edit/extend do not use this helper
/// (they stay on legacy `Video` at their call sites).
pub fn default_video_model_for(_mode: VideoMode) -> ModelId {
    ModelId::Video15
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
    fn video_15_allows_t2v_and_r2v() {
        assert!(validate_video_generate(
            ModelId::Video15,
            VideoMode::TextToVideo,
            Some("1080p"),
            Some(8),
            0,
            0
        )
        .is_ok());
        assert!(validate_video_generate(
            ModelId::Video15,
            VideoMode::ReferenceToVideo,
            Some("720p"),
            Some(8),
            3,
            1
        )
        .is_ok());
        assert!(validate_video_generate(
            ModelId::Video15,
            VideoMode::ReferenceToVideo,
            Some("1080p"),
            Some(8),
            1,
            0
        )
        .is_err());
        assert!(validate_video_generate(
            ModelId::Video,
            VideoMode::ReferenceToVideo,
            None,
            Some(8),
            1,
            1
        )
        .is_err());
    }

    #[test]
    fn only_15_allows_1080p_on_t2v_i2v() {
        assert!(validate_video_generate(
            ModelId::Video15,
            VideoMode::ImageToVideo,
            Some("1080p"),
            Some(8),
            0,
            0
        )
        .is_ok());
        assert!(validate_video_generate(
            ModelId::Video,
            VideoMode::TextToVideo,
            Some("1080p"),
            Some(8),
            0,
            0
        )
        .is_err());
    }

    #[test]
    fn default_generate_model_is_15() {
        assert_eq!(
            default_video_model_for(VideoMode::TextToVideo),
            ModelId::Video15
        );
        assert_eq!(
            default_video_model_for(VideoMode::ReferenceToVideo),
            ModelId::Video15
        );
    }

    #[test]
    fn voice_ids_normalize() {
        assert_eq!(parse_voice_id("Eve").unwrap(), "eve");
        assert!(parse_voice_id("").is_err());
        assert_eq!(parse_reference_audios(["eve", "", "ARA"]).unwrap().len(), 2);
        assert!(parse_reference_audios(["a", "b", "c", "d"]).is_err());
    }

    #[test]
    fn model_aliases_parse() {
        assert_eq!(ModelId::parse("quality").unwrap(), ModelId::ImageQuality);
        assert_eq!(ModelId::parse("1.5").unwrap(), ModelId::Video15);
        assert_eq!(ModelId::parse("2.0").unwrap(), ModelId::Image20);
        assert_eq!(ModelId::parse("image-2.0").unwrap(), ModelId::Image20);
        assert_eq!(
            ModelId::parse("grok-imagine-image-2.0").unwrap(),
            ModelId::Image20
        );
        assert_eq!(
            ModelId::parse("grok-imagine-image-2.0-preview").unwrap(),
            ModelId::Image20
        );
        assert!(ModelId::Image20.is_image());
        assert!(!ModelId::Video.is_image());
    }

    #[test]
    fn catalog_covers_every_model_id() {
        for id in [
            ModelId::Image,
            ModelId::ImageQuality,
            ModelId::Image20,
            ModelId::Video,
            ModelId::Video15,
        ] {
            assert_eq!(get(id).id, id.as_str());
        }
        assert_eq!(catalog().len(), 5);
        assert!((get(ModelId::Image20).approx_usd_unit - 0.04).abs() < f64::EPSILON);
        assert!(get(ModelId::Image20).capabilities.quality_param);
        assert!(!get(ModelId::ImageQuality).capabilities.quality_param);
        assert!(get(ModelId::Video15).capabilities.text_to_video);
        assert!(get(ModelId::Video15).capabilities.reference_to_video);
        assert_eq!(
            get(ModelId::Video15).capabilities.max_reference_audios,
            Some(3)
        );
    }

    #[test]
    fn quality_param_only_on_image_20() {
        assert!(validate_image_quality(ModelId::Image20, Some(ImageQuality::Low)).is_ok());
        assert!(validate_image_quality(ModelId::Image20, None).is_ok());
        assert!(validate_image_quality(ModelId::Image, Some(ImageQuality::Medium)).is_err());
        assert!(validate_image_quality(ModelId::ImageQuality, Some(ImageQuality::Low)).is_err());
        assert_eq!(ImageQuality::parse("low").unwrap(), ImageQuality::Low);
        assert_eq!(parse_optional_image_quality(Some("auto")).unwrap(), None);
        assert!(ImageQuality::parse("high").is_err());
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
