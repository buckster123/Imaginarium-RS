//! Local video craft — timeline → ffmpeg → library job (Studio+ 5.3).
//!
//! Render architecture (the cutting-room port — U2a correctness, U2b grammar):
//!
//! 1. **Probe** every clip source with ffprobe — real durations, dimensions,
//!    fps, audio presence. No guessed durations anywhere.
//! 2. **Normalize** each segment into a silent piece on one shared canvas
//!    (`scale=…:force_original_aspect_ratio=decrease,pad=…,fps=…,format=yuv420p`,
//!    identical codec settings). Three segment kinds: `clip` (trim + speed),
//!    `still` (one image → Ken-Burns zoompan), `card` (solid color). Captions,
//!    letterbox bars, and fades are drawn here, segment-local. Segments are
//!    content-hash cached — an unchanged segment is never re-encoded.
//! 3. **Concat** the segments with the concat demuxer and `-c copy` — valid by
//!    construction because every segment has identical stream parameters.
//! 4. **Audio master pass**: one ffmpeg run mixes every audio source (clip audio
//!    placed at its master-clock offset via `adelay`, speed-matched with
//!    `atempo`, plus the optional music bed) with `amix=normalize=0`, then muxes
//!    onto the concatenated video with `-c:v copy`. Segments carry no audio, so
//!    per-segment AAC priming can never accumulate into A/V drift.
//! 5. Optional **loudnorm ship pass**: two-pass EBU R128 (measure → apply with
//!    measured values), video stream-copied.
//!
//! Timeline-level `overlays` use the **master clock** and are mapped (and split)
//! into whichever segments they intersect; segment-level `captions` are
//! segment-local. Both render on every segment they touch — the historic
//! "overlays only on clip 0" defect is unrepresentable in this pipeline.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::error::{Error, Result};
use crate::jobs::JobStore;
use crate::library::Library;
use crate::types::JobResult;

/// Highest timeline contract version this engine understands.
/// 0 (absent) = the pre-versioned U2a shape — parsed identically, all new
/// fields defaulting off.
pub const TIMELINE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// A trimmed video clip (the historic behavior).
    #[default]
    Clip,
    /// One still image animated with a Ken-Burns zoom (`zoom_from` → `zoom_to`).
    Still,
    /// A solid-color title card (captions carry the text).
    Card,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    /// Segment kind — `clip` (default), `still`, or `card`.
    #[serde(default)]
    pub kind: SegmentKind,
    /// Source library job id (required for `clip`/`still`; forbidden on `card`).
    #[serde(default)]
    pub job_id: String,
    /// Inclusive start seconds (clip).
    #[serde(default)]
    pub in_s: f64,
    /// Exclusive end seconds; 0 / omitted = full remaining duration (probed).
    #[serde(default)]
    pub out_s: f64,
    /// Audio gain in dB for this clip's own audio (0 = unchanged).
    #[serde(default)]
    pub gain_db: f64,
    /// Playback speed (clip only), 0.5–2.0. Audio follows via `atempo`.
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// Segment duration in seconds — required for `still` and `card`.
    #[serde(default)]
    pub dur_s: f64,
    /// Ken-Burns start zoom (still only), 1.0–3.0.
    #[serde(default = "default_zoom")]
    pub zoom_from: f64,
    /// Ken-Burns end zoom (still only), 1.0–3.0. Equal to `zoom_from` = static.
    #[serde(default = "default_zoom")]
    pub zoom_to: f64,
    /// Card background color (card only); empty = style `card_bg`, then black.
    #[serde(default)]
    pub card_color: String,
    /// Captions owned by this segment; times are **segment-local** seconds.
    #[serde(default)]
    pub captions: Vec<TextOverlay>,
}

fn default_speed() -> f64 {
    1.0
}
fn default_zoom() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOverlay {
    pub text: String,
    #[serde(default)]
    pub start_s: f64,
    #[serde(default = "default_end")]
    pub end_s: f64,
    #[serde(default = "default_x")]
    pub x: i32,
    #[serde(default = "default_y")]
    pub y: i32,
    /// 0 = inherit (style `caption_fontsize`, then 28).
    #[serde(default)]
    pub fontsize: u32,
    /// Empty = inherit (style `caption_color`, then white).
    #[serde(default)]
    pub color: String,
}

fn default_end() -> f64 {
    3.0
}
fn default_x() -> i32 {
    40
}
fn default_y() -> i32 {
    40
}

/// Music bed mixed under the whole piece on the master clock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrack {
    /// Library job id holding the audio (an imported track — e.g. a Sonus
    /// composition — or any video job whose audio should be borrowed).
    pub job_id: String,
    /// Seconds into the source track to start reading from.
    #[serde(default)]
    pub in_s: f64,
    /// Placement on the master clock (seconds from the start of the video).
    #[serde(default)]
    pub start_s: f64,
    /// Gain in dB (music beds usually want a negative value).
    #[serde(default)]
    pub gain_db: f64,
    /// Fade-in seconds at the start of the bed.
    #[serde(default)]
    pub fade_in_s: f64,
    /// Fade-out seconds at the end of the bed (bed end = track end or video
    /// end, whichever comes first).
    #[serde(default)]
    pub fade_out_s: f64,
}

/// Global styling — config-not-code aesthetics (the cutting-room style block).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CraftStyle {
    /// Default caption fontsize for captions that don't set one (0 = 28).
    #[serde(default)]
    pub caption_fontsize: u32,
    /// Default caption color (empty = white). Named color or #RRGGBB.
    #[serde(default)]
    pub caption_color: String,
    /// Default card background (empty = black). Named color or #RRGGBB.
    #[serde(default)]
    pub card_bg: String,
    /// Cinematic letterbox bars: height of EACH bar as a fraction of canvas
    /// height, 0–0.45. 0 with `letterbox_reveal_s` > 0 = bars open fully.
    #[serde(default)]
    pub letterbox_frac: f64,
    /// Animated reveal: bars open from fully closed over this many seconds at
    /// the start of the piece (master clock).
    #[serde(default)]
    pub letterbox_reveal_s: f64,
    /// Two-pass EBU R128 loudness normalization on the final mix (ship pass).
    #[serde(default)]
    pub loudnorm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTimeline {
    /// Contract version; 0/absent = legacy (identical semantics, defaults off).
    #[serde(default)]
    pub version: u32,
    pub clips: Vec<TimelineClip>,
    #[serde(default)]
    pub audio_fade_in_s: f64,
    #[serde(default)]
    pub audio_fade_out_s: f64,
    #[serde(default)]
    pub video_fade_in_s: f64,
    #[serde(default)]
    pub video_fade_out_s: f64,
    /// Overlays on the **master clock** (absolute seconds across the whole
    /// piece); they render on every segment they intersect.
    #[serde(default)]
    pub overlays: Vec<TextOverlay>,
    /// Optional music bed (see [`AudioTrack`]).
    #[serde(default)]
    pub music: Option<AudioTrack>,
    /// Global styling (see [`CraftStyle`]).
    #[serde(default)]
    pub style: Option<CraftStyle>,
    /// Output canvas width; 0 = derive from the first clip (even-floored).
    #[serde(default)]
    pub width: u32,
    /// Output canvas height; 0 = derive from the first clip (even-floored).
    #[serde(default)]
    pub height: u32,
    /// Output frame rate; 0 = derive from the first clip (rounded, fallback 24).
    #[serde(default)]
    pub fps: u32,
    /// Optional note stored on the craft job.
    #[serde(default)]
    pub note: Option<String>,
}

/// Hard cap on drawtext filters per segment — an unbounded caption list is an
/// ffmpeg-filtergraph DoS vector (audit G4 follow-up).
const MAX_CAPTIONS_PER_SEGMENT: usize = 32;

/// Segment cache ceiling — oldest entries pruned past this.
const SEGMENT_CACHE_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Resolve first media file for a job under library root (YYYY/MM/DD/job_id/).
/// Delegates to the library's indexed resolver (index 0 = the historic
/// first-file behavior) so craft renders and the content route share one walk.
pub fn resolve_job_media(library_root: &Path, job_id: &str) -> Option<PathBuf> {
    crate::library::resolve_job_asset(library_root, job_id, 0)
}

pub fn ffmpeg_available() -> bool {
    tool_available("ffmpeg")
}

pub fn ffprobe_available() -> bool {
    tool_available("ffprobe")
}

fn tool_available(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ffmpeg_version_line() -> String {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".into())
}

// ---------------------------------------------------------------------------
// Probing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct MediaProbe {
    pub duration_s: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Deserialize)]
struct ProbeDoc {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

/// Parse an ffprobe rational like `"24000/1001"`; `"0/0"` and malformed input
/// yield None.
fn parse_fps_frac(s: &str) -> Option<f64> {
    let (num, den) = s.trim().split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den <= 0.0 || num <= 0.0 {
        return None;
    }
    Some(num / den)
}

fn parse_ffprobe_json(body: &str) -> Result<MediaProbe> {
    let doc: ProbeDoc =
        serde_json::from_str(body).map_err(|e| Error::other(format!("ffprobe parse: {e}")))?;
    let mut probe = MediaProbe {
        duration_s: 0.0,
        width: 0,
        height: 0,
        fps: 0.0,
        has_video: false,
        has_audio: false,
    };
    let mut stream_duration = 0.0f64;
    for s in &doc.streams {
        match s.codec_type.as_deref() {
            Some("video") if !probe.has_video => {
                probe.has_video = true;
                probe.width = s.width.unwrap_or(0);
                probe.height = s.height.unwrap_or(0);
                probe.fps = s
                    .r_frame_rate
                    .as_deref()
                    .and_then(parse_fps_frac)
                    .or_else(|| s.avg_frame_rate.as_deref().and_then(parse_fps_frac))
                    .unwrap_or(0.0);
                if let Some(d) = s.duration.as_deref().and_then(|d| d.parse::<f64>().ok()) {
                    stream_duration = stream_duration.max(d);
                }
            }
            Some("audio") => probe.has_audio = true,
            _ => {}
        }
    }
    probe.duration_s = doc
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(stream_duration);
    Ok(probe)
}

/// Probe a media file with ffprobe (real duration/dims/fps — never guessed).
pub fn probe_media(path: &Path) -> Result<MediaProbe> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::other(format!("spawn ffprobe: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(Error::other(format!(
            "ffprobe failed on {}: {}",
            path.display(),
            err.chars().take(300).collect::<String>()
        )));
    }
    parse_ffprobe_json(&String::from_utf8_lossy(&output.stdout))
}

// ---------------------------------------------------------------------------
// Planning (pure — the unit-test surface)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub fps: u32,
}

/// Floor to an even dimension (libx264 yuv420p requires even dims), min 2.
fn even_dim(d: u32) -> u32 {
    d.max(2) & !1
}

fn resolve_canvas(tl: &VideoTimeline, first_clip: Option<&MediaProbe>) -> Canvas {
    let (pw, ph, pf) = first_clip
        .map(|p| (p.width, p.height, p.fps))
        .unwrap_or((1280, 720, 24.0));
    let w = if tl.width >= 2 { tl.width } else { pw };
    let h = if tl.height >= 2 { tl.height } else { ph };
    let fps = if tl.fps >= 1 {
        tl.fps.min(120)
    } else {
        let f = pf.round() as u32;
        if f == 0 {
            24
        } else {
            f.min(120)
        }
    };
    Canvas {
        w: even_dim(w),
        h: even_dim(h),
        fps,
    }
}

/// Validate a color destined for a filtergraph: named colors and #RRGGBB only —
/// anything else could smuggle filter options.
fn safe_color(s: &str) -> Result<()> {
    let ok =
        !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '#');
    if ok {
        Ok(())
    } else {
        Err(Error::other(format!(
            "invalid color {:?} — use a named color or #RRGGBB",
            s.chars().take(40).collect::<String>()
        )))
    }
}

/// ffmpeg's hex color spelling is `0xRRGGBB`; accept the web `#RRGGBB` form.
fn ffmpeg_color(s: &str) -> String {
    match s.strip_prefix('#') {
        Some(hex) => format!("0x{hex}"),
        None => s.to_string(),
    }
}

/// Clip lead filter — the cutting-room normalization recipe (aspect-fit onto
/// the canvas, pad the remainder, unify frame rate and pixel format), with
/// optional speed retiming. Every clip segment passes through this — which is
/// what makes the later `-c copy` concat valid. `setpts`
/// runs before the `fps` resample so the output stays constant-frame-rate.
fn clip_lead(c: Canvas, speed: f64) -> String {
    let setpts = if (speed - 1.0).abs() > 1e-9 {
        format!("setpts=PTS/{speed:.3},")
    } else {
        String::new()
    };
    format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,{setpts}fps={fps},format=yuv420p",
        w = c.w,
        h = c.h,
        fps = c.fps
    )
}

/// Still lead — the Ken-Burns recipe: cover-scale + crop to 2× canvas (headroom
/// for the zoom window), then `zoompan` over ONE input frame (never `-loop 1`),
/// linear zoom `from`→`to`, center-locked, emitting exactly `frames` frames at
/// canvas size and rate.
fn still_lead(c: Canvas, zoom_from: f64, zoom_to: f64, frames: u32) -> String {
    let (w2, h2) = (c.w * 2, c.h * 2);
    format!(
        "scale={w2}:{h2}:force_original_aspect_ratio=increase,crop={w2}:{h2},\
         zoompan=z='{zf:.4}+({zt:.4}-{zf:.4})*on/{frames}':d={frames}:\
         x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s={w}x{h}:fps={fps},format=yuv420p",
        w = c.w,
        h = c.h,
        fps = c.fps,
        zf = zoom_from,
        zt = zoom_to,
    )
}

/// Letterbox bar geometry for one segment.
#[derive(Debug, Clone, PartialEq)]
struct LetterboxSpec {
    /// Bar height fraction of canvas height (each bar), 0–0.45.
    frac: f64,
    /// Reveal window in master-clock seconds (0 = static bars).
    reveal_s: f64,
    /// This segment's master-clock offset (drives the reveal expression).
    master_off: f64,
}

/// The bar-height expression: static `H*frac`, or — inside the reveal window —
/// bars opening from fully closed (`H/2`) down to their resting height.
/// Segment-local `t` is offset onto the master clock.
fn letterbox_height_expr(lb: &LetterboxSpec, canvas: Canvas) -> String {
    let fh = (lb.frac * canvas.h as f64).round();
    if lb.reveal_s <= 0.0 || lb.master_off >= lb.reveal_s {
        return format!("{fh}");
    }
    let h2 = canvas.h as f64 / 2.0;
    format!(
        "max({fh}\\,{h2}*(1-min((t+{off:.3})/{r:.3}\\,1)))",
        off = lb.master_off,
        r = lb.reveal_s
    )
}

fn letterbox_filters(lb: &LetterboxSpec, canvas: Canvas) -> Vec<String> {
    let h = letterbox_height_expr(lb, canvas);
    vec![
        format!("drawbox=x=0:y=0:w=iw:h='{h}':color=black:t=fill"),
        format!("drawbox=x=0:y='ih-({h})':w=iw:h='{h}':color=black:t=fill"),
    ]
}

#[derive(Debug, Clone)]
struct SegmentPlan {
    kind: SegmentKind,
    /// Source file (clip/still); None for cards.
    src: Option<PathBuf>,
    in_s: f64,
    /// Source seconds read (clip; = dur × speed).
    src_window: f64,
    /// Output seconds on the master clock (frame-quantized).
    dur: f64,
    /// Offset of this segment on the master clock.
    start_master: f64,
    has_audio: bool,
    gain_db: f64,
    speed: f64,
    zoom_from: f64,
    zoom_to: f64,
    /// Resolved card background (ffmpeg spelling).
    card_color: String,
    /// Segment-local captions with fontsize/color already resolved.
    captions: Vec<TextOverlay>,
    fade_in_s: f64,
    fade_out_s: f64,
    letterbox: Option<LetterboxSpec>,
}

/// Project a master-clock overlay into one segment's local time; None when the
/// overlay does not intersect the segment.
fn overlay_in_segment(ov: &TextOverlay, seg_start: f64, seg_dur: f64) -> Option<TextOverlay> {
    let a = ov.start_s.max(seg_start);
    let b = ov.end_s.min(seg_start + seg_dur);
    if b - a <= 0.001 {
        return None;
    }
    let mut local = ov.clone();
    local.start_s = a - seg_start;
    local.end_s = b - seg_start;
    Some(local)
}

/// Resolve a caption's inherited fields against the style block and validate
/// the color for filtergraph safety.
fn resolve_caption(ov: &TextOverlay, style: &CraftStyle) -> Result<TextOverlay> {
    let mut c = ov.clone();
    if c.fontsize == 0 {
        c.fontsize = if style.caption_fontsize > 0 {
            style.caption_fontsize
        } else {
            28
        };
    }
    if c.color.is_empty() {
        c.color = if style.caption_color.is_empty() {
            "white".into()
        } else {
            style.caption_color.clone()
        };
    }
    safe_color(&c.color)?;
    c.color = ffmpeg_color(&c.color);
    Ok(c)
}

/// Quantize a duration to the canvas frame grid (min one frame) — segment
/// encodes emit whole frames, and the master clock must agree with them.
fn quantize(dur: f64, fps: u32) -> f64 {
    let frames = (dur * fps as f64).round().max(1.0);
    frames / fps as f64
}

/// Build the segment plan from probed sources. Durations come from ffprobe or
/// explicit `dur_s` — there is no fallback constant. Errors are honest.
fn plan_segments(
    tl: &VideoTimeline,
    sources: &[(Option<PathBuf>, &TimelineClip)],
    probes: &[Option<MediaProbe>],
    canvas: Canvas,
) -> Result<(Vec<SegmentPlan>, f64)> {
    if tl.version > TIMELINE_VERSION {
        return Err(Error::other(format!(
            "timeline version {} is newer than this engine (max {TIMELINE_VERSION})",
            tl.version
        )));
    }
    let style = tl.style.clone().unwrap_or_default();
    if !(0.0..=0.45).contains(&style.letterbox_frac) {
        return Err(Error::other(
            "style.letterbox_frac must be between 0 and 0.45",
        ));
    }
    let letterbox_on = style.letterbox_frac > 0.0 || style.letterbox_reveal_s > 0.0;

    let mut plans = Vec::with_capacity(sources.len());
    let mut cursor = 0.0f64;
    let last = sources.len() - 1;
    for (i, ((path, clip), probe)) in sources.iter().zip(probes).enumerate() {
        if !(0.5..=2.0).contains(&clip.speed) {
            return Err(Error::other(format!(
                "segment {i}: speed {:.2} out of range (0.5–2.0)",
                clip.speed
            )));
        }
        if clip.kind != SegmentKind::Clip && (clip.speed - 1.0).abs() > 1e-9 {
            return Err(Error::other(format!(
                "segment {i}: speed applies to clip segments only"
            )));
        }
        if clip.kind != SegmentKind::Still
            && ((clip.zoom_from - 1.0).abs() > 1e-9 || (clip.zoom_to - 1.0).abs() > 1e-9)
        {
            return Err(Error::other(format!(
                "segment {i}: zoom applies to still segments only"
            )));
        }
        let (dur, src_window, has_audio) = match clip.kind {
            SegmentKind::Clip => {
                let probe = probe
                    .as_ref()
                    .ok_or_else(|| Error::other(format!("segment {i}: missing probe for clip")))?;
                let in_s = clip.in_s.max(0.0);
                let window = if clip.out_s > in_s {
                    clip.out_s - in_s
                } else {
                    probe.duration_s - in_s
                };
                if window <= 0.05 {
                    return Err(Error::other(format!(
                        "segment {i} ({}): empty window — in_s {:.3} / out_s {:.3} against source duration {:.3}",
                        clip.job_id, clip.in_s, clip.out_s, probe.duration_s
                    )));
                }
                (
                    quantize(window / clip.speed, canvas.fps),
                    window,
                    probe.has_audio,
                )
            }
            SegmentKind::Still | SegmentKind::Card => {
                if clip.dur_s <= 0.05 {
                    return Err(Error::other(format!(
                        "segment {i}: {} segments need dur_s",
                        if clip.kind == SegmentKind::Still {
                            "still"
                        } else {
                            "card"
                        }
                    )));
                }
                if clip.kind == SegmentKind::Still
                    && !((1.0..=3.0).contains(&clip.zoom_from)
                        && (1.0..=3.0).contains(&clip.zoom_to))
                {
                    return Err(Error::other(format!(
                        "segment {i}: zoom must be between 1.0 and 3.0"
                    )));
                }
                (quantize(clip.dur_s, canvas.fps), 0.0, false)
            }
        };
        let card_color = if clip.kind == SegmentKind::Card {
            let c = if !clip.card_color.is_empty() {
                clip.card_color.clone()
            } else if !style.card_bg.is_empty() {
                style.card_bg.clone()
            } else {
                "black".into()
            };
            safe_color(&c)?;
            ffmpeg_color(&c)
        } else {
            String::new()
        };

        let mut captions: Vec<TextOverlay> = Vec::new();
        for cap in clip.captions.iter().filter(|c| c.end_s - c.start_s > 0.001) {
            captions.push(resolve_caption(cap, &style)?);
        }
        for ov in &tl.overlays {
            if let Some(local) = overlay_in_segment(ov, cursor, dur) {
                captions.push(resolve_caption(&local, &style)?);
            }
        }
        if captions.len() > MAX_CAPTIONS_PER_SEGMENT {
            return Err(Error::other(format!(
                "segment {i}: {} captions exceed the per-segment cap of {MAX_CAPTIONS_PER_SEGMENT}",
                captions.len()
            )));
        }

        plans.push(SegmentPlan {
            kind: clip.kind,
            src: path.clone(),
            in_s: clip.in_s.max(0.0),
            src_window,
            dur,
            start_master: cursor,
            has_audio,
            gain_db: clip.gain_db,
            speed: clip.speed,
            zoom_from: clip.zoom_from,
            zoom_to: clip.zoom_to,
            card_color,
            captions,
            fade_in_s: if i == 0 { tl.video_fade_in_s } else { 0.0 },
            fade_out_s: if i == last { tl.video_fade_out_s } else { 0.0 },
            letterbox: letterbox_on.then_some(LetterboxSpec {
                frac: style.letterbox_frac,
                reveal_s: style.letterbox_reveal_s,
                master_off: cursor,
            }),
        });
        cursor += dur;
    }
    Ok((plans, cursor))
}

// ---------------------------------------------------------------------------
// ffmpeg argument builders (pure)
// ---------------------------------------------------------------------------

/// Every ffmpeg invocation starts here — `-nostdin` is non-negotiable (a craft
/// render must never pause a headless node waiting for console input).
fn base_args() -> Vec<String> {
    ["-nostdin", "-y", "-hide_banner", "-loglevel", "error"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn segment_vf(plan: &SegmentPlan, canvas: Canvas) -> String {
    let mut vf = vec![match plan.kind {
        SegmentKind::Clip => clip_lead(canvas, plan.speed),
        SegmentKind::Still => still_lead(
            canvas,
            plan.zoom_from,
            plan.zoom_to,
            (plan.dur * canvas.fps as f64).round() as u32,
        ),
        // the lavfi color source is already canvas-sized at canvas rate
        SegmentKind::Card => "format=yuv420p".to_string(),
    }];
    if let Some(lb) = &plan.letterbox {
        vf.extend(letterbox_filters(lb, canvas));
    }
    for cap in &plan.captions {
        let escaped = escape_drawtext(&cap.text);
        vf.push(format!(
            "drawtext=text='{escaped}':expansion=none:x={}:y={}:fontsize={}:fontcolor={}:borderw=2:bordercolor=black:enable='between(t\\,{:.3}\\,{:.3})'",
            cap.x, cap.y, cap.fontsize, cap.color, cap.start_s, cap.end_s
        ));
    }
    if plan.fade_in_s > 0.0 {
        vf.push(format!("fade=t=in:st=0:d={:.3}", plan.fade_in_s));
    }
    if plan.fade_out_s > 0.0 {
        let st = (plan.dur - plan.fade_out_s).max(0.0);
        vf.push(format!("fade=t=out:st={st:.3}:d={:.3}", plan.fade_out_s));
    }
    vf.join(",")
}

/// One normalized, **silent** segment. A fixed track timescale keeps every
/// segment's mp4 timebase identical — belt-and-braces for the `-c copy` concat.
fn segment_args(plan: &SegmentPlan, canvas: Canvas, out: &Path) -> Vec<String> {
    let mut args = base_args();
    match plan.kind {
        SegmentKind::Clip => {
            let src = plan.src.as_ref().expect("clip has src");
            if plan.in_s > 0.0 {
                args.extend(["-ss".into(), format!("{:.3}", plan.in_s)]);
            }
            args.extend(["-i".into(), src.display().to_string()]);
            args.extend(["-t".into(), format!("{:.3}", plan.src_window)]);
            args.extend(["-vf".into(), segment_vf(plan, canvas)]);
        }
        SegmentKind::Still => {
            let src = plan.src.as_ref().expect("still has src");
            let frames = (plan.dur * canvas.fps as f64).round() as u32;
            args.extend(["-i".into(), src.display().to_string()]);
            args.extend(["-vf".into(), segment_vf(plan, canvas)]);
            args.extend(["-frames:v".into(), frames.to_string()]);
        }
        SegmentKind::Card => {
            args.extend(["-t".into(), format!("{:.3}", plan.dur)]);
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!(
                    "color=c={}:s={}x{}:r={}",
                    plan.card_color, canvas.w, canvas.h, canvas.fps
                ),
            ]);
            args.extend(["-vf".into(), segment_vf(plan, canvas)]);
        }
    }
    args.extend([
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-crf".into(),
        "23".into(),
        "-video_track_timescale".into(),
        "90000".into(),
        out.display().to_string(),
    ]);
    args
}

fn concat_args(list_path: &Path, out: &Path) -> Vec<String> {
    let mut args = base_args();
    args.extend([
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.display().to_string(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        out.display().to_string(),
    ]);
    args
}

/// One mixable audio source, placed on the master clock.
#[derive(Debug, Clone)]
struct AudioChain {
    /// ffmpeg input index (0 is the concatenated video).
    input: usize,
    gain_db: f64,
    delay_ms: u64,
    /// Speed retiming to match the video segment (1.0 = none).
    atempo: f64,
    /// Bed-local fades (music only; clips fade via the mix-level fades).
    fade_in_s: f64,
    fade_out_s: f64,
    /// Post-trim, post-atempo length of this source (fade-out anchor).
    local_len_s: f64,
}

/// The master-clock audio mix: every source is format-unified, speed-matched,
/// gained, placed with `adelay`, then mixed with `normalize=0` (load-bearing —
/// the default normalize=1 divides by input count and buries the mix).
fn audio_mix_filter(chains: &[AudioChain], master_dur: f64, fade_in: f64, fade_out: f64) -> String {
    let mut parts = Vec::with_capacity(chains.len() + 1);
    for c in chains {
        let mut f = format!(
            "[{}:a]aformat=sample_rates=48000:channel_layouts=stereo",
            c.input
        );
        if (c.atempo - 1.0).abs() > 1e-9 {
            f.push_str(&format!(",atempo={:.3}", c.atempo));
        }
        f.push_str(&format!(",volume={:.3}dB", c.gain_db));
        if c.fade_in_s > 0.0 {
            f.push_str(&format!(",afade=t=in:st=0:d={:.3}", c.fade_in_s));
        }
        if c.fade_out_s > 0.0 {
            let st = (c.local_len_s - c.fade_out_s).max(0.0);
            f.push_str(&format!(",afade=t=out:st={st:.3}:d={:.3}", c.fade_out_s));
        }
        f.push_str(&format!(",adelay={}:all=1[a{}]", c.delay_ms, c.input));
        parts.push(f);
    }
    let labels: String = chains.iter().map(|c| format!("[a{}]", c.input)).collect();
    let mut mix = format!(
        "{labels}amix=inputs={}:duration=longest:normalize=0",
        chains.len()
    );
    if fade_in > 0.0 {
        mix.push_str(&format!(",afade=t=in:st=0:d={fade_in:.3}"));
    }
    if fade_out > 0.0 {
        let st = (master_dur - fade_out).max(0.0);
        mix.push_str(&format!(",afade=t=out:st={st:.3}:d={fade_out:.3}"));
    }
    mix.push_str("[aout]");
    parts.push(mix);
    parts.join(";")
}

/// Mux pass: silent master video + all audio sources → final file. Video is
/// stream-copied; only audio is encoded, exactly once, on the master clock.
fn mux_args(
    master: &Path,
    audio_inputs: &[(PathBuf, f64, f64)],
    filter: &str,
    out: &Path,
) -> Vec<String> {
    let mut args = base_args();
    args.extend(["-i".into(), master.display().to_string()]);
    for (src, ss, t) in audio_inputs {
        if *ss > 0.0 {
            args.extend(["-ss".into(), format!("{ss:.3}")]);
        }
        args.extend(["-t".into(), format!("{t:.3}")]);
        args.extend(["-i".into(), src.display().to_string()]);
    }
    args.extend([
        "-filter_complex".into(),
        filter.into(),
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        "[aout]".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-movflags".into(),
        "+faststart".into(),
        out.display().to_string(),
    ]);
    args
}

// ---------------------------------------------------------------------------
// Loudness ship pass (two-pass EBU R128)
// ---------------------------------------------------------------------------

const LOUDNORM_TARGET: &str = "I=-16:TP=-1.5:LRA=11";

#[derive(Debug, Clone, PartialEq)]
struct LoudnormStats {
    input_i: f64,
    input_tp: f64,
    input_lra: f64,
    input_thresh: f64,
    target_offset: f64,
}

/// Extract the loudnorm JSON block that the measure pass prints on stderr.
fn parse_loudnorm_stats(stderr: &str) -> Result<LoudnormStats> {
    let start = stderr
        .rfind('{')
        .ok_or_else(|| Error::other("loudnorm measure: no JSON in ffmpeg output"))?;
    let end = stderr[start..]
        .find('}')
        .map(|e| start + e + 1)
        .ok_or_else(|| Error::other("loudnorm measure: unterminated JSON"))?;
    let doc: std::collections::HashMap<String, String> = serde_json::from_str(&stderr[start..end])
        .map_err(|e| Error::other(format!("loudnorm measure parse: {e}")))?;
    let get = |k: &str| -> Result<f64> {
        doc.get(k)
            .and_then(|v| v.parse::<f64>().ok())
            .ok_or_else(|| Error::other(format!("loudnorm measure: missing {k}")))
    };
    Ok(LoudnormStats {
        input_i: get("input_i")?,
        input_tp: get("input_tp")?,
        input_lra: get("input_lra")?,
        input_thresh: get("input_thresh")?,
        target_offset: get("target_offset")?,
    })
}

fn loudnorm_measure_args(input: &Path) -> Vec<String> {
    // NOT base_args: loudnorm prints its stats block at the info log level,
    // so `-loglevel error` would silence the very output this pass exists for.
    let mut args: Vec<String> = ["-nostdin", "-y", "-hide_banner", "-loglevel", "info"]
        .into_iter()
        .map(String::from)
        .collect();
    args.extend([
        "-i".into(),
        input.display().to_string(),
        "-af".into(),
        format!("loudnorm={LOUDNORM_TARGET}:print_format=json"),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]);
    args
}

/// Apply pass with the measured values (`linear=true` = one clean gain ramp);
/// video stream-copied, audio re-encoded once at 48 kHz.
fn loudnorm_apply_args(input: &Path, stats: &LoudnormStats, out: &Path) -> Vec<String> {
    let mut args = base_args();
    args.extend([
        "-i".into(),
        input.display().to_string(),
        "-af".into(),
        format!(
            "loudnorm={LOUDNORM_TARGET}:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}:linear=true",
            stats.input_i, stats.input_tp, stats.input_lra, stats.input_thresh, stats.target_offset
        ),
        "-ar".into(),
        "48000".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-movflags".into(),
        "+faststart".into(),
        out.display().to_string(),
    ]);
    args
}

// ---------------------------------------------------------------------------
// Segment cache (content-hash keyed)
// ---------------------------------------------------------------------------

/// `{data-home}/craft-segcache` — beside the library tree.
fn segment_cache_dir(library: &Library) -> PathBuf {
    library
        .root
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join("craft-segcache")
}

/// Source-file identity for the cache key: path + size + mtime seconds.
fn src_identity(path: &Path) -> String {
    let meta = std::fs::metadata(path).ok();
    let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}|{len}|{mtime}", path.display())
}

/// Deterministic content key over everything that shapes the segment's bytes.
fn segment_cache_key(plan: &SegmentPlan, canvas: Canvas) -> String {
    let mut s = format!(
        "u2b1|{}x{}@{}|{:?}|{}|{:.3}|{:.3}|{:.3}|{:.3}|{:.4}|{:.4}|{}|{:.3}|{:.3}",
        canvas.w,
        canvas.h,
        canvas.fps,
        plan.kind,
        plan.src.as_deref().map(src_identity).unwrap_or_default(),
        plan.in_s,
        plan.src_window,
        plan.dur,
        plan.speed,
        plan.zoom_from,
        plan.zoom_to,
        plan.card_color,
        plan.fade_in_s,
        plan.fade_out_s,
    );
    if let Some(lb) = &plan.letterbox {
        s.push_str(&format!(
            "|lb{:.4},{:.3},{:.3}",
            lb.frac, lb.reveal_s, lb.master_off
        ));
    }
    for c in &plan.captions {
        s.push_str(&format!(
            "|cap{}|{:.3}|{:.3}|{}|{}|{}|{}",
            c.text, c.start_s, c.end_s, c.x, c.y, c.fontsize, c.color
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

/// Prune oldest cache entries beyond the byte ceiling. Runs before a render so
/// the render's own (newest) segments are never evicted mid-use.
fn prune_segment_cache(dir: &Path, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = e.metadata().ok()?;
            m.is_file()
                .then(|| (p, m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, len, _)| len).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|(_, _, mtime)| *mtime);
    let mut excess = total - max_bytes;
    for (path, len, _) in files {
        if std::fs::remove_file(&path).is_ok() {
            info!(path = %path.display(), "pruned craft segment cache entry");
            excess = excess.saturating_sub(len);
            if excess == 0 {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

const VIDEO_EXTS: &[&str] = &["mp4", "webm", "mov", "mkv"];
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

fn path_ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Render timeline with ffmpeg and import result into the library.
pub fn render_timeline(
    library: &Library,
    jobs: &JobStore,
    library_root: &Path,
    timeline: &VideoTimeline,
) -> Result<JobResult> {
    if timeline.clips.is_empty() {
        return Err(Error::other("timeline has no clips"));
    }
    if !ffmpeg_available() || !ffprobe_available() {
        return Err(Error::other(
            "ffmpeg/ffprobe not found on PATH — install ffmpeg for video craft",
        ));
    }

    // Resolve + gate every segment source by kind.
    let mut sources: Vec<(Option<PathBuf>, &TimelineClip)> = Vec::new();
    for (i, c) in timeline.clips.iter().enumerate() {
        match c.kind {
            SegmentKind::Card => {
                if !c.job_id.is_empty() {
                    return Err(Error::other(format!(
                        "segment {i}: card segments take no job_id"
                    )));
                }
                sources.push((None, c));
            }
            SegmentKind::Clip | SegmentKind::Still => {
                if !crate::library::is_safe_asset_id(&c.job_id) {
                    return Err(Error::forbidden(format!("invalid job_id: {}", c.job_id)));
                }
                let path = resolve_job_media(library_root, &c.job_id).ok_or_else(|| {
                    Error::other(format!("no local media for job_id {}", c.job_id))
                })?;
                let ext = path_ext(&path);
                match c.kind {
                    SegmentKind::Clip if IMAGE_EXTS.contains(&ext.as_str()) => {
                        return Err(Error::other(format!(
                            "segment {i} ({}) resolves to an image — use kind \"still\" for Ken-Burns image segments",
                            c.job_id
                        )));
                    }
                    SegmentKind::Clip if !VIDEO_EXTS.contains(&ext.as_str()) => {
                        return Err(Error::other(format!(
                            "segment {i} ({}) resolves to a non-video file (.{ext}) — audio belongs in the `music` track",
                            c.job_id
                        )));
                    }
                    SegmentKind::Still if !IMAGE_EXTS.contains(&ext.as_str()) => {
                        return Err(Error::other(format!(
                            "segment {i} ({}) is kind \"still\" but resolves to .{ext} — stills need an image source",
                            c.job_id
                        )));
                    }
                    _ => {}
                }
                sources.push((Some(path), c));
            }
        }
    }

    // Probe clips (stills/cards need no probe — their duration is explicit).
    let mut probes: Vec<Option<MediaProbe>> = Vec::with_capacity(sources.len());
    for (path, clip) in &sources {
        if clip.kind != SegmentKind::Clip {
            probes.push(None);
            continue;
        }
        let path = path.as_ref().expect("clip has src");
        let probe = probe_media(path)?;
        if !probe.has_video || probe.width < 2 || probe.height < 2 {
            return Err(Error::other(format!(
                "clip {}: no usable video stream in {}",
                clip.job_id,
                path.display()
            )));
        }
        if probe.duration_s <= 0.0 {
            return Err(Error::other(format!(
                "clip {}: could not determine duration of {}",
                clip.job_id,
                path.display()
            )));
        }
        probes.push(Some(probe));
    }

    let music = match &timeline.music {
        Some(m) => {
            if !crate::library::is_safe_asset_id(&m.job_id) {
                return Err(Error::forbidden(format!(
                    "invalid music job_id: {}",
                    m.job_id
                )));
            }
            let path = resolve_job_media(library_root, &m.job_id).ok_or_else(|| {
                Error::other(format!("no local media for music job_id {}", m.job_id))
            })?;
            let probe = probe_media(&path)?;
            if !probe.has_audio {
                return Err(Error::other(format!(
                    "music job {} has no audio stream",
                    m.job_id
                )));
            }
            Some((path, probe, m))
        }
        None => None,
    };

    let first_clip_probe = probes.iter().flatten().next();
    let canvas = resolve_canvas(timeline, first_clip_probe);
    let (plans, master_dur) = plan_segments(timeline, &sources, &probes, canvas)?;

    let work = std::env::temp_dir().join(format!(
        "imaginarium-craft-{}",
        ulid::Ulid::new().to_string()
    ));
    std::fs::create_dir_all(&work)?;
    let cache = segment_cache_dir(library);

    let result = render_plans(&plans, canvas, master_dur, timeline, music, &work, &cache);
    let out_path = match result {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work);
            return Err(e);
        }
    };

    let bytes = std::fs::read(&out_path)?;
    let note = timeline
        .note
        .clone()
        .unwrap_or_else(|| "video craft render".into());
    let source = sources
        .iter()
        .map(|(_, c)| c.job_id.as_str())
        .find(|id| !id.is_empty())
        .unwrap_or("");
    let provenance = serde_json::json!({
        "contract_version": TIMELINE_VERSION,
        "engine": "craft-u2b",
        "ffmpeg": ffmpeg_version_line(),
        "timeline": timeline,
    });
    let imported = library.import_bytes(
        jobs,
        &bytes,
        "craft.mp4",
        Some(&note),
        if source.is_empty() {
            None
        } else {
            Some(source)
        },
        Some(&provenance),
    );

    let _ = std::fs::remove_dir_all(&work);
    imported
}

/// Segments (cache-aware) → concat → audio master pass → optional loudnorm.
/// Returns the final file.
#[allow(clippy::too_many_arguments)]
fn render_plans(
    plans: &[SegmentPlan],
    canvas: Canvas,
    master_dur: f64,
    tl: &VideoTimeline,
    music: Option<(PathBuf, MediaProbe, &AudioTrack)>,
    work: &Path,
    cache: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(cache)?;
    prune_segment_cache(cache, SEGMENT_CACHE_MAX_BYTES);

    let mut list_body = String::new();
    for plan in plans {
        let key = segment_cache_key(plan, canvas);
        let cached = cache.join(format!("{key}.mp4"));
        if !cached.is_file() {
            // Render beside the final name, then rename — atomic within the
            // cache dir (a work-dir render + rename would cross filesystems).
            let tmp = cache.join(format!("{key}.{}.part.mp4", ulid::Ulid::new()));
            run_ffmpeg(&segment_args(plan, canvas, &tmp))?;
            std::fs::rename(&tmp, &cached)?;
        } else {
            info!(key = %key, "craft segment cache hit");
        }
        list_body.push_str(&format!("file '{}'\n", cached.display()));
    }
    let list_path = work.join("concat.txt");
    std::fs::write(&list_path, list_body)?;
    let master = work.join("master.mp4");
    run_ffmpeg(&concat_args(&list_path, &master))?;

    // Audio master pass — one mix on the master clock.
    let mut audio_inputs: Vec<(PathBuf, f64, f64)> = Vec::new();
    let mut chains: Vec<AudioChain> = Vec::new();
    for plan in plans {
        if !plan.has_audio {
            continue;
        }
        let input = audio_inputs.len() + 1;
        audio_inputs.push((
            plan.src.clone().expect("audio plan has src"),
            plan.in_s,
            plan.src_window,
        ));
        chains.push(AudioChain {
            input,
            gain_db: plan.gain_db,
            delay_ms: (plan.start_master * 1000.0).round() as u64,
            atempo: plan.speed,
            fade_in_s: 0.0,
            fade_out_s: 0.0,
            local_len_s: plan.dur,
        });
    }
    if let Some((path, probe, m)) = &music {
        let start = m.start_s.max(0.0);
        if start >= master_dur {
            return Err(Error::other(format!(
                "music start_s {:.3} is past the end of the video ({master_dur:.3}s)",
                m.start_s
            )));
        }
        let in_s = m.in_s.max(0.0);
        let local_len = (probe.duration_s - in_s).min(master_dur - start);
        if local_len <= 0.05 {
            return Err(Error::other(format!(
                "music in_s {:.3} is past the end of the track ({:.3}s)",
                m.in_s, probe.duration_s
            )));
        }
        let input = audio_inputs.len() + 1;
        audio_inputs.push((path.clone(), in_s, local_len));
        chains.push(AudioChain {
            input,
            gain_db: m.gain_db,
            delay_ms: (start * 1000.0).round() as u64,
            atempo: 1.0,
            fade_in_s: m.fade_in_s,
            fade_out_s: m.fade_out_s,
            local_len_s: local_len,
        });
    }

    let mixed = if chains.is_empty() {
        master
    } else {
        let filter = audio_mix_filter(&chains, master_dur, tl.audio_fade_in_s, tl.audio_fade_out_s);
        let out = work.join("mixed.mp4");
        run_ffmpeg(&mux_args(&master, &audio_inputs, &filter, &out))?;
        out
    };

    // Loudness ship pass — only meaningful when there is audio to normalize.
    let want_loudnorm = tl.style.as_ref().map(|s| s.loudnorm).unwrap_or(false);
    if want_loudnorm && !chains.is_empty() {
        let stderr = run_ffmpeg_capture(&loudnorm_measure_args(&mixed))?;
        let stats = parse_loudnorm_stats(&stderr)?;
        let shipped = work.join("shipped.mp4");
        run_ffmpeg(&loudnorm_apply_args(&mixed, &stats, &shipped))?;
        return Ok(shipped);
    }
    Ok(mixed)
}

fn run_ffmpeg(args: &[String]) -> Result<()> {
    run_ffmpeg_capture(args).map(|_| ())
}

/// Run ffmpeg; on success return its stderr (the loudnorm measure pass prints
/// its JSON there), on failure surface a bounded stderr excerpt.
fn run_ffmpeg_capture(args: &[String]) -> Result<String> {
    info!(?args, "ffmpeg craft");
    let output = std::process::Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::other(format!("spawn ffmpeg: {e}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        warn!(err = %stderr, "ffmpeg failed");
        return Err(Error::other(format!(
            "ffmpeg failed: {}",
            stderr.chars().take(800).collect::<String>()
        )));
    }
    Ok(stderr)
}

/// Escape caption text for ffmpeg's `drawtext=text='…'` single-quoted argument.
///
/// ffmpeg single-quoted strings treat every byte literally until the next `'`, and a
/// backslash does NOT escape the quote inside them — so the previous `'`→`\'` mapping
/// both rendered wrong (a literal backslash appeared) AND let crafted caption text
/// close the quote and inject filtergraph filters/options. The only correct way to
/// embed a literal `'` is to close the quote, emit an escaped quote, and reopen:
/// `'` → `'\''`. Paired with `:expansion=none` on the filter (which disables
/// drawtext's own `%`/`\` text expansion), all caption text stays inert.
fn escape_drawtext(s: &str) -> String {
    s.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(dur: f64, w: u32, h: u32, fps: f64, audio: bool) -> MediaProbe {
        MediaProbe {
            duration_s: dur,
            width: w,
            height: h,
            fps,
            has_video: true,
            has_audio: audio,
        }
    }

    fn clip(job: &str, in_s: f64, out_s: f64) -> TimelineClip {
        TimelineClip {
            kind: SegmentKind::Clip,
            job_id: job.into(),
            in_s,
            out_s,
            gain_db: 0.0,
            speed: 1.0,
            dur_s: 0.0,
            zoom_from: 1.0,
            zoom_to: 1.0,
            card_color: String::new(),
            captions: vec![],
        }
    }

    fn overlay(text: &str, start_s: f64, end_s: f64) -> TextOverlay {
        TextOverlay {
            text: text.into(),
            start_s,
            end_s,
            x: 40,
            y: 40,
            fontsize: 0,
            color: String::new(),
        }
    }

    fn bare_timeline(clips: Vec<TimelineClip>) -> VideoTimeline {
        VideoTimeline {
            version: TIMELINE_VERSION,
            clips,
            audio_fade_in_s: 0.0,
            audio_fade_out_s: 0.0,
            video_fade_in_s: 0.0,
            video_fade_out_s: 0.0,
            overlays: vec![],
            music: None,
            style: None,
            width: 0,
            height: 0,
            fps: 0,
            note: None,
        }
    }

    const CANVAS: Canvas = Canvas {
        w: 640,
        h: 360,
        fps: 24,
    };

    fn plan_one(tl: &VideoTimeline, probe: MediaProbe) -> Result<(Vec<SegmentPlan>, f64)> {
        let sources: Vec<(Option<PathBuf>, &TimelineClip)> = tl
            .clips
            .iter()
            .map(|c| {
                (
                    (c.kind != SegmentKind::Card).then(|| PathBuf::from("/x/a.mp4")),
                    c,
                )
            })
            .collect();
        let probes: Vec<Option<MediaProbe>> = tl
            .clips
            .iter()
            .map(|c| (c.kind == SegmentKind::Clip).then(|| probe.clone()))
            .collect();
        plan_segments(tl, &sources, &probes, CANVAS)
    }

    #[test]
    fn drawtext_escaping_is_ffmpeg_correct() {
        // apostrophe: close-quote, escaped-quote, reopen
        assert_eq!(escape_drawtext("it's"), "it'\\''s");
    }

    #[test]
    fn drawtext_escaping_neutralizes_injection() {
        // an attempt to break out of text='…' into the filtergraph stays quoted text
        let e = escape_drawtext("x':drawtext=fontfile=/etc/passwd");
        assert!(e.starts_with("x'\\''"));
        // the injected metacharacters survive only as literal caption content
        assert!(e.contains(":drawtext=fontfile=/etc/passwd"));
    }

    #[test]
    fn safe_ids_gate_craft() {
        assert!(!crate::library::is_safe_asset_id("../../etc"));
        assert!(crate::library::is_safe_asset_id("01ABCDEF"));
    }

    #[test]
    fn norm_filter_is_the_cutting_room_recipe() {
        let c = Canvas {
            w: 1280,
            h: 720,
            fps: 24,
        };
        assert_eq!(
            clip_lead(c, 1.0),
            "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,fps=24,format=yuv420p"
        );
        // speed injects setpts BEFORE the fps resample (stays CFR)
        assert_eq!(
            clip_lead(c, 1.5),
            "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,setpts=PTS/1.500,fps=24,format=yuv420p"
        );
    }

    #[test]
    fn still_lead_is_the_ken_burns_recipe() {
        let f = still_lead(CANVAS, 1.0, 1.12, 72);
        // cover-scale to 2× canvas for zoom headroom, ONE input frame drives d=N
        assert!(f.starts_with("scale=1280:720:force_original_aspect_ratio=increase,crop=1280:720,"));
        assert!(f.contains("zoompan=z='1.0000+(1.1200-1.0000)*on/72':d=72:"));
        assert!(f.contains("s=640x360:fps=24"));
        assert!(f.ends_with("format=yuv420p"));
    }

    #[test]
    fn canvas_dims_are_even_floored() {
        let tl = bare_timeline(vec![clip("01A", 0.0, 0.0)]);
        let c = resolve_canvas(&tl, Some(&probe(5.0, 1921, 1081, 30.0, false)));
        assert_eq!((c.w, c.h, c.fps), (1920, 1080, 30));
        // explicit odd request also lands even
        let mut tl2 = bare_timeline(vec![clip("01A", 0.0, 0.0)]);
        tl2.width = 853;
        tl2.height = 481;
        tl2.fps = 25;
        let c2 = resolve_canvas(&tl2, Some(&probe(5.0, 640, 360, 24.0, false)));
        assert_eq!((c2.w, c2.h, c2.fps), (852, 480, 25));
        // no clips at all (stills/cards only) → sane default canvas
        let c3 = resolve_canvas(&tl, None);
        assert_eq!((c3.w, c3.h, c3.fps), (1280, 720, 24));
    }

    #[test]
    fn fps_fracs_parse() {
        assert_eq!(parse_fps_frac("24/1"), Some(24.0));
        let ntsc = parse_fps_frac("24000/1001").unwrap();
        assert!((ntsc - 23.976).abs() < 0.001);
        assert_eq!(parse_fps_frac("0/0"), None);
        assert_eq!(parse_fps_frac("garbage"), None);
    }

    #[test]
    fn ffprobe_json_parses() {
        let body = r#"{
            "streams": [
                {"codec_type":"video","width":1280,"height":720,
                 "r_frame_rate":"24000/1001","avg_frame_rate":"24000/1001",
                 "duration":"8.008000"},
                {"codec_type":"audio","r_frame_rate":"0/0"}
            ],
            "format": {"duration": "8.031000"}
        }"#;
        let p = parse_ffprobe_json(body).unwrap();
        assert!(p.has_video && p.has_audio);
        assert_eq!((p.width, p.height), (1280, 720));
        assert!((p.duration_s - 8.031).abs() < 0.001);
        assert!((p.fps - 23.976).abs() < 0.001);
    }

    #[test]
    fn plan_uses_probed_durations_not_a_constant() {
        // out_s omitted → duration comes from the probe (the 6.0s guess is dead)
        let tl = bare_timeline(vec![clip("01A", 1.0, 0.0)]);
        let (plans, master) = plan_one(&tl, probe(7.5, 640, 360, 24.0, true)).unwrap();
        assert!((plans[0].dur - 6.5).abs() < 1e-9);
        assert!((master - 6.5).abs() < 1e-9);
        // a window past the end of the source is an honest error
        let tl2 = bare_timeline(vec![clip("01A", 9.0, 0.0)]);
        assert!(plan_one(&tl2, probe(7.5, 640, 360, 24.0, true)).is_err());
    }

    #[test]
    fn plan_places_segments_on_the_master_clock() {
        let tl = bare_timeline(vec![clip("01A", 0.0, 4.0), clip("01B", 2.0, 5.5)]);
        let sources: Vec<(Option<PathBuf>, &TimelineClip)> = vec![
            (Some(PathBuf::from("/x/a.mp4")), &tl.clips[0]),
            (Some(PathBuf::from("/x/b.mp4")), &tl.clips[1]),
        ];
        let probes = vec![
            Some(probe(6.0, 640, 360, 24.0, true)),
            Some(probe(6.0, 640, 360, 24.0, false)),
        ];
        let (plans, master) = plan_segments(&tl, &sources, &probes, CANVAS).unwrap();
        assert_eq!(plans.len(), 2);
        assert!((plans[0].start_master - 0.0).abs() < 1e-9);
        assert!((plans[1].start_master - 4.0).abs() < 1e-9);
        assert!((master - 7.5).abs() < 1e-9);
        assert!(plans[0].has_audio && !plans[1].has_audio);
    }

    #[test]
    fn newer_contract_versions_are_rejected() {
        let mut tl = bare_timeline(vec![clip("01A", 0.0, 4.0)]);
        tl.version = TIMELINE_VERSION + 1;
        let err = plan_one(&tl, probe(6.0, 640, 360, 24.0, false)).unwrap_err();
        assert!(err.to_string().contains("newer than this engine"));
    }

    #[test]
    fn speed_reshapes_the_clock_and_validates() {
        // 4s source window at 2× → 2s of output on the master clock
        let mut c = clip("01A", 0.0, 4.0);
        c.speed = 2.0;
        let tl = bare_timeline(vec![c]);
        let (plans, master) = plan_one(&tl, probe(6.0, 640, 360, 24.0, true)).unwrap();
        assert!((plans[0].dur - 2.0).abs() < 1e-9);
        assert!((plans[0].src_window - 4.0).abs() < 1e-9);
        assert!((master - 2.0).abs() < 1e-9);
        // out of atempo's single-instance range → honest error
        let mut c2 = clip("01A", 0.0, 4.0);
        c2.speed = 3.0;
        let tl2 = bare_timeline(vec![c2]);
        assert!(plan_one(&tl2, probe(6.0, 640, 360, 24.0, true)).is_err());
        // speed on a still is a contract error
        let mut s = clip("01B", 0.0, 0.0);
        s.kind = SegmentKind::Still;
        s.dur_s = 3.0;
        s.speed = 1.5;
        let tl3 = bare_timeline(vec![s]);
        assert!(plan_one(&tl3, probe(6.0, 640, 360, 24.0, false)).is_err());
    }

    #[test]
    fn stills_and_cards_quantize_to_the_frame_grid() {
        let mut s = clip("01B", 0.0, 0.0);
        s.kind = SegmentKind::Still;
        s.dur_s = 2.02; // 48.48 frames @24 → 48 → 2.0s exactly
        s.zoom_from = 1.0;
        s.zoom_to = 1.2;
        let mut card = clip("", 0.0, 0.0);
        card.kind = SegmentKind::Card;
        card.dur_s = 3.0;
        let tl = bare_timeline(vec![s, card]);
        let (plans, master) = plan_one(&tl, probe(6.0, 640, 360, 24.0, false)).unwrap();
        assert!((plans[0].dur - 2.0).abs() < 1e-9);
        assert!((plans[1].dur - 3.0).abs() < 1e-9);
        assert!((master - 5.0).abs() < 1e-9);
        assert_eq!(plans[1].card_color, "black");
        // still without dur_s → honest error
        let mut s2 = clip("01B", 0.0, 0.0);
        s2.kind = SegmentKind::Still;
        let tl2 = bare_timeline(vec![s2]);
        assert!(plan_one(&tl2, probe(6.0, 640, 360, 24.0, false)).is_err());
    }

    #[test]
    fn overlays_map_across_segments_on_the_master_clock() {
        // Segments: [0,4) and [4,8). An overlay [5,7) previously vanished
        // (only clip 0 got overlays) — now it lands in segment 1, local [1,3).
        let ov = overlay("hi", 5.0, 7.0);
        assert!(overlay_in_segment(&ov, 0.0, 4.0).is_none());
        let local = overlay_in_segment(&ov, 4.0, 4.0).unwrap();
        assert!((local.start_s - 1.0).abs() < 1e-9);
        assert!((local.end_s - 3.0).abs() < 1e-9);

        // An overlay spanning the cut is split across both segments.
        let span = overlay("cut", 3.0, 6.0);
        let a = overlay_in_segment(&span, 0.0, 4.0).unwrap();
        let b = overlay_in_segment(&span, 4.0, 4.0).unwrap();
        assert!((a.start_s - 3.0).abs() < 1e-9 && (a.end_s - 4.0).abs() < 1e-9);
        assert!((b.start_s - 0.0).abs() < 1e-9 && (b.end_s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn caption_style_resolution_inherits_and_gates() {
        let style = CraftStyle {
            caption_fontsize: 44,
            caption_color: "#FFCC00".into(),
            ..Default::default()
        };
        let r = resolve_caption(&overlay("t", 0.0, 2.0), &style).unwrap();
        assert_eq!(r.fontsize, 44);
        assert_eq!(r.color, "0xFFCC00"); // web spelling → ffmpeg spelling
                                         // explicit per-caption values win
        let mut ov = overlay("t", 0.0, 2.0);
        ov.fontsize = 18;
        ov.color = "red".into();
        let r2 = resolve_caption(&ov, &style).unwrap();
        assert_eq!((r2.fontsize, r2.color.as_str()), (18, "red"));
        // no style, no explicit → the historic defaults
        let r3 = resolve_caption(&overlay("t", 0.0, 2.0), &CraftStyle::default()).unwrap();
        assert_eq!((r3.fontsize, r3.color.as_str()), (28, "white"));
        // a color with filtergraph metacharacters never reaches ffmpeg
        let mut evil = overlay("t", 0.0, 2.0);
        evil.color = "red:enable=0".into();
        assert!(resolve_caption(&evil, &CraftStyle::default()).is_err());
    }

    #[test]
    fn letterbox_exprs_are_static_or_master_clock_revealed() {
        let c = Canvas {
            w: 1280,
            h: 720,
            fps: 24,
        };
        // static bars
        let stat = LetterboxSpec {
            frac: 0.12,
            reveal_s: 0.0,
            master_off: 0.0,
        };
        assert_eq!(letterbox_height_expr(&stat, c), "86");
        // animated open at the head of the piece, offset onto the master clock
        let anim = LetterboxSpec {
            frac: 0.12,
            reveal_s: 1.5,
            master_off: 0.0,
        };
        assert_eq!(
            letterbox_height_expr(&anim, c),
            "max(86\\,360*(1-min((t+0.000)/1.500\\,1)))"
        );
        // a segment starting after the reveal window collapses to static
        let later = LetterboxSpec {
            frac: 0.12,
            reveal_s: 1.5,
            master_off: 4.0,
        };
        assert_eq!(letterbox_height_expr(&later, c), "86");
        // two bars, top and bottom
        let filters = letterbox_filters(&stat, c);
        assert_eq!(filters.len(), 2);
        assert!(filters[0].contains("y=0"));
        assert!(filters[1].contains("y='ih-(86)'"));
    }

    #[test]
    fn caption_cap_is_enforced() {
        let mut c = clip("01A", 0.0, 4.0);
        c.captions = (0..MAX_CAPTIONS_PER_SEGMENT + 1)
            .map(|i| overlay(&format!("c{i}"), 0.0, 1.0))
            .collect();
        let tl = bare_timeline(vec![c]);
        assert!(plan_one(&tl, probe(6.0, 640, 360, 24.0, false)).is_err());
    }

    fn test_plan(kind: SegmentKind) -> SegmentPlan {
        SegmentPlan {
            kind,
            src: (kind != SegmentKind::Card).then(|| PathBuf::from("/x/a.mp4")),
            in_s: 1.0,
            src_window: 3.5,
            dur: 3.5,
            start_master: 0.0,
            has_audio: true,
            gain_db: 0.0,
            speed: 1.0,
            zoom_from: 1.0,
            zoom_to: 1.0,
            card_color: "0x101418".into(),
            captions: vec![],
            fade_in_s: 0.5,
            fade_out_s: 0.0,
            letterbox: None,
        }
    }

    #[test]
    fn segment_args_are_silent_normalized_and_nostdin() {
        let args = segment_args(&test_plan(SegmentKind::Clip), CANVAS, Path::new("/w/s.mp4"));
        assert_eq!(args[0], "-nostdin");
        assert!(args.contains(&"-an".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "-t" && w[1] == "3.500"));
        assert!(args.windows(2).any(|w| w[0] == "-ss" && w[1] == "1.000"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-video_track_timescale" && w[1] == "90000"));
        let vf = args
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.starts_with("scale=640:360:force_original_aspect_ratio=decrease"));
        assert!(vf.contains("fade=t=in:st=0:d=0.500"));

        // a card renders from a lavfi color source at canvas size/rate — no -loop
        let cargs = segment_args(&test_plan(SegmentKind::Card), CANVAS, Path::new("/w/c.mp4"));
        assert!(cargs.iter().any(|a| a == "color=c=0x101418:s=640x360:r=24"));
        assert!(!cargs.contains(&"-loop".to_string()));

        // a still emits exactly its frame count — and also never uses -loop
        let sargs = segment_args(
            &test_plan(SegmentKind::Still),
            CANVAS,
            Path::new("/w/i.mp4"),
        );
        assert!(sargs
            .windows(2)
            .any(|w| w[0] == "-frames:v" && w[1] == "84"));
        assert!(!sargs.contains(&"-loop".to_string()));
    }

    #[test]
    fn concat_and_mux_args_hold_the_invariants() {
        let cargs = concat_args(Path::new("/w/concat.txt"), Path::new("/w/master.mp4"));
        assert_eq!(cargs[0], "-nostdin");
        assert!(cargs.windows(2).any(|w| w[0] == "-c" && w[1] == "copy"));
        assert!(cargs.contains(&"+faststart".to_string()));

        let margs = mux_args(
            Path::new("/w/master.mp4"),
            &[(PathBuf::from("/x/a.mp4"), 0.0, 4.0)],
            "[1:a]anull[aout]",
            Path::new("/w/out.mp4"),
        );
        assert_eq!(margs[0], "-nostdin");
        assert!(margs.windows(2).any(|w| w[0] == "-c:v" && w[1] == "copy"));
        assert!(margs.windows(2).any(|w| w[0] == "-map" && w[1] == "[aout]"));
    }

    #[test]
    fn audio_mix_is_master_clocked_and_unnormalized() {
        let chains = vec![
            AudioChain {
                input: 1,
                gain_db: 0.0,
                delay_ms: 0,
                atempo: 1.5,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                local_len_s: 4.0,
            },
            AudioChain {
                input: 2,
                gain_db: -3.0,
                delay_ms: 4000,
                atempo: 1.0,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                local_len_s: 3.5,
            },
            // the music bed: placed at 0, faded, trimmed to the master window
            AudioChain {
                input: 3,
                gain_db: -8.0,
                delay_ms: 0,
                atempo: 1.0,
                fade_in_s: 1.0,
                fade_out_s: 2.0,
                local_len_s: 7.5,
            },
        ];
        let f = audio_mix_filter(&chains, 7.5, 0.0, 1.5);
        // every source unified before mixing
        assert_eq!(
            f.matches("aformat=sample_rates=48000:channel_layouts=stereo")
                .count(),
            3
        );
        // a sped-up clip's audio follows via atempo
        assert!(f.contains("[1:a]aformat=sample_rates=48000:channel_layouts=stereo,atempo=1.500,"));
        // master-clock placement
        assert!(f.contains("adelay=4000:all=1[a2]"));
        // normalize=0 is load-bearing (default divides by input count)
        assert!(f.contains("amix=inputs=3:duration=longest:normalize=0"));
        // bed fades anchored to the bed's local end
        assert!(f.contains("afade=t=out:st=5.500:d=2.000"));
        // mix-level fade-out anchored to master end
        assert!(f.contains("afade=t=out:st=6.000:d=1.500[aout]"));
        assert!(f.contains("volume=-8.000dB"));
    }

    #[test]
    fn loudnorm_stats_parse_from_measure_stderr() {
        let stderr = r#"
frame= 100 fps=0.0 q=-0.0 size=N/A time=00:00:04.00 bitrate=N/A speed= 512x
[Parsed_loudnorm_0 @ 0x55d]
{
	"input_i" : "-23.62",
	"input_tp" : "-6.47",
	"input_lra" : "4.10",
	"input_thresh" : "-34.13",
	"output_i" : "-16.58",
	"output_tp" : "-2.32",
	"output_lra" : "3.50",
	"output_thresh" : "-27.01",
	"normalization_type" : "dynamic",
	"target_offset" : "0.58"
}
"#;
        let s = parse_loudnorm_stats(stderr).unwrap();
        assert!((s.input_i - -23.62).abs() < 0.001);
        assert!((s.target_offset - 0.58).abs() < 0.001);
        // and the apply args carry the measured values, copy the video
        let args = loudnorm_apply_args(Path::new("/w/m.mp4"), &s, Path::new("/w/s.mp4"));
        let af = args
            .windows(2)
            .find(|w| w[0] == "-af")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(af.contains("measured_I=-23.62"));
        assert!(af.contains("linear=true"));
        assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "copy"));
        // garbage in → honest error
        assert!(parse_loudnorm_stats("no json here").is_err());
    }

    #[test]
    fn cache_keys_are_stable_and_param_sensitive() {
        let plan = test_plan(SegmentKind::Card); // card: no src identity (fs-independent)
        let k1 = segment_cache_key(&plan, CANVAS);
        let k2 = segment_cache_key(&plan, CANVAS);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64); // sha256 hex
                                  // any param that shapes the bytes changes the key
        let mut faster = test_plan(SegmentKind::Card);
        faster.dur = 4.0;
        assert_ne!(segment_cache_key(&faster, CANVAS), k1);
        let mut colored = test_plan(SegmentKind::Card);
        colored.card_color = "0x223344".into();
        assert_ne!(segment_cache_key(&colored, CANVAS), k1);
        let other_canvas = Canvas {
            w: 1280,
            h: 720,
            fps: 24,
        };
        assert_ne!(segment_cache_key(&plan, other_canvas), k1);
        let mut captioned = test_plan(SegmentKind::Card);
        captioned.captions = vec![overlay("hi", 0.0, 1.0)];
        assert_ne!(segment_cache_key(&captioned, CANVAS), k1);
    }

    #[test]
    fn timeline_serde_is_backward_compatible() {
        // A pre-U2a payload (no music/captions/canvas/version) still parses.
        let old = r#"{
            "clips": [{"job_id": "01A", "in_s": 0.0, "out_s": 4.0, "gain_db": 0.0}],
            "overlays": [{"text": "hello", "start_s": 1.0, "end_s": 2.0}],
            "audio_fade_out_s": 1.0
        }"#;
        let tl: VideoTimeline = serde_json::from_str(old).unwrap();
        assert_eq!(tl.version, 0);
        assert_eq!(tl.clips.len(), 1);
        assert_eq!(tl.clips[0].kind, SegmentKind::Clip);
        assert!((tl.clips[0].speed - 1.0).abs() < 1e-9);
        assert!(tl.clips[0].captions.is_empty());
        assert!(tl.music.is_none() && tl.style.is_none());
        assert_eq!((tl.width, tl.height, tl.fps), (0, 0, 0));

        // And the v1 grammar round-trips. (r## — the hex colors contain `"#`.)
        let v1 = r##"{
            "version": 1,
            "clips": [
                {"job_id": "01A", "speed": 1.5, "captions": [{"text": "local", "start_s": 0.5}]},
                {"kind": "still", "job_id": "01S", "dur_s": 3.0, "zoom_from": 1.0, "zoom_to": 1.15},
                {"kind": "card", "dur_s": 2.0, "card_color": "#101418",
                 "captions": [{"text": "THE END", "start_s": 0.2, "end_s": 1.8, "fontsize": 64}]}
            ],
            "music": {"job_id": "01M", "gain_db": -6.0, "fade_out_s": 2.0},
            "style": {"caption_color": "#FFCC00", "letterbox_frac": 0.12,
                      "letterbox_reveal_s": 1.5, "loudnorm": true},
            "width": 1280, "height": 720, "fps": 24
        }"##;
        let tl2: VideoTimeline = serde_json::from_str(v1).unwrap();
        assert_eq!(tl2.version, 1);
        assert_eq!(tl2.clips[1].kind, SegmentKind::Still);
        assert_eq!(tl2.clips[2].kind, SegmentKind::Card);
        assert!(tl2.style.as_ref().unwrap().loudnorm);
        assert_eq!(tl2.music.as_ref().unwrap().job_id, "01M");
        // and kinds serialize snake_case
        let back = serde_json::to_string(&tl2).unwrap();
        assert!(back.contains("\"kind\":\"still\""));
    }

    // -- integration: the full engine against real ffmpeg (skips when absent) --

    fn synth(args: &[&str]) {
        let ok = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-y", "-hide_banner", "-loglevel", "error"])
            .args(args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "ffmpeg synth failed: {args:?}");
    }

    #[test]
    fn e2e_full_grammar_render_with_cache() {
        if !ffmpeg_available() || !ffprobe_available() {
            eprintln!("skipping e2e craft test: ffmpeg/ffprobe not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("library");
        std::fs::create_dir_all(&root).unwrap();
        let library = Library::new(&root);
        let jobs = JobStore::open(&dir.path().join("jobs.sqlite")).unwrap();

        // Two clips with mismatched resolution AND frame rate — exactly the
        // input that broke the old unnormalized `-c copy` concat — plus a
        // still image and a music bed.
        let a = dir.path().join("a.mp4");
        let b = dir.path().join("b.mp4");
        let img = dir.path().join("i.png");
        let m = dir.path().join("m.wav");
        synth(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2:sample_rate=44100",
            "-shortest",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-c:a",
            "aac",
            a.to_str().unwrap(),
        ]);
        synth(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=640x360:rate=25",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            b.to_str().unwrap(),
        ]);
        synth(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=1:size=500x400:rate=1",
            "-frames:v",
            "1",
            img.to_str().unwrap(),
        ]);
        synth(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=6:sample_rate=48000",
            m.to_str().unwrap(),
        ]);

        let ja = library
            .import_bytes(
                &jobs,
                &std::fs::read(&a).unwrap(),
                "a.mp4",
                None,
                None,
                None,
            )
            .unwrap();
        let jb = library
            .import_bytes(
                &jobs,
                &std::fs::read(&b).unwrap(),
                "b.mp4",
                None,
                None,
                None,
            )
            .unwrap();
        let ji = library
            .import_bytes(
                &jobs,
                &std::fs::read(&img).unwrap(),
                "i.png",
                None,
                None,
                None,
            )
            .unwrap();
        let jm = library
            .import_bytes(
                &jobs,
                &std::fs::read(&m).unwrap(),
                "m.wav",
                None,
                None,
                None,
            )
            .unwrap();
        // the wav import lands as an audio asset
        assert!(matches!(jm.assets[0].kind, crate::types::AssetKind::Audio));

        let mut speed_clip = clip(&jb.job_id.to_string(), 0.0, 2.0);
        speed_clip.speed = 2.0; // 2s source → 1s on the master clock
        let mut still = clip(&ji.job_id.to_string(), 0.0, 0.0);
        still.kind = SegmentKind::Still;
        still.dur_s = 1.0;
        still.zoom_from = 1.0;
        still.zoom_to = 1.2;
        let mut card = clip("", 0.0, 0.0);
        card.kind = SegmentKind::Card;
        card.dur_s = 1.0;
        card.card_color = "#101418".into();
        card.captions = vec![TextOverlay {
            text: "THE END".into(),
            start_s: 0.1,
            end_s: 0.9,
            x: 40,
            y: 40,
            fontsize: 40,
            color: String::new(),
        }];
        let mut first = clip(&ja.job_id.to_string(), 0.0, 0.0); // probed → 2s
        first.gain_db = -2.0;
        first.captions = vec![overlay("clip-local", 0.2, 1.0)];

        let tl = VideoTimeline {
            version: 1,
            clips: vec![first, speed_clip, still, card],
            audio_fade_in_s: 0.2,
            audio_fade_out_s: 0.5,
            video_fade_in_s: 0.2,
            video_fade_out_s: 0.3,
            // master-clock overlay living entirely in the SECOND segment —
            // the old engine dropped this one
            overlays: vec![overlay("second act", 2.2, 2.8)],
            music: Some(AudioTrack {
                job_id: jm.job_id.to_string(),
                in_s: 0.0,
                start_s: 0.0,
                gain_db: -6.0,
                fade_in_s: 0.3,
                fade_out_s: 0.5,
            }),
            style: Some(CraftStyle {
                caption_color: "#FFCC00".into(),
                letterbox_frac: 0.1,
                letterbox_reveal_s: 0.8,
                loudnorm: true,
                ..Default::default()
            }),
            width: 0,
            height: 0,
            fps: 0,
            note: Some("u2b e2e".into()),
        };

        let out = render_timeline(&library, &jobs, &root, &tl).unwrap();
        assert!(out.ok);
        let out_path = PathBuf::from(out.assets[0].local_path.as_ref().unwrap());
        let p = probe_media(&out_path).unwrap();
        // canvas derived from the first clip; every segment normalized onto it
        assert_eq!((p.width, p.height), (320, 240));
        assert!((p.fps - 30.0).abs() < 0.5);
        // master duration = 2 (clip) + 1 (2s@2×) + 1 (still) + 1 (card)
        assert!(
            (p.duration_s - 5.0).abs() < 0.35,
            "duration {}",
            p.duration_s
        );
        // the mix + loudnorm passes delivered an audio track
        assert!(p.has_audio);

        // provenance rides the craft job's meta.json
        let meta_path = out_path.parent().unwrap().join("meta.json");
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(meta_path).unwrap()).unwrap();
        assert_eq!(meta["provenance"]["engine"], "craft-u2b");
        assert_eq!(meta["provenance"]["contract_version"], 1);
        assert_eq!(meta["provenance"]["timeline"]["version"], 1);

        // the segment cache filled, and a second render rides it entirely
        let cache = segment_cache_dir(&library);
        let count = std::fs::read_dir(&cache).unwrap().flatten().count();
        assert_eq!(count, 4, "each segment cached once");
        let out2 = render_timeline(&library, &jobs, &root, &tl).unwrap();
        assert!(out2.ok);
        let count2 = std::fs::read_dir(&cache).unwrap().flatten().count();
        assert_eq!(count2, 4, "second render re-used every cached segment");
    }
}
