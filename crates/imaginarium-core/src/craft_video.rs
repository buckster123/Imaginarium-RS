//! Local video craft — timeline → ffmpeg → library job (Studio+ 5.3).
//!
//! Render architecture (the cutting-room port, U2a):
//!
//! 1. **Probe** every source with ffprobe — real durations, dimensions, fps,
//!    audio presence. No guessed durations anywhere.
//! 2. **Normalize** each clip into a silent segment on one shared canvas
//!    (`scale=…:force_original_aspect_ratio=decrease,pad=…,fps=…,format=yuv420p`,
//!    identical codec settings). Captions are drawn here, segment-local.
//! 3. **Concat** the segments with the concat demuxer and `-c copy` — valid by
//!    construction because every segment has identical stream parameters.
//! 4. **Audio master pass**: one ffmpeg run mixes every audio source (clip audio
//!    placed at its master-clock offset via `adelay`, plus the optional music
//!    bed) with `amix=normalize=0`, then muxes onto the concatenated video with
//!    `-c:v copy`. Segments carry no audio, so per-segment AAC priming can
//!    never accumulate into A/V drift.
//!
//! Timeline-level `overlays` use the **master clock** and are mapped (and split)
//! into whichever segments they intersect; clip-level `captions` are
//! segment-local. Both render on every segment they touch — the historic
//! "overlays only on clip 0" defect is unrepresentable in this pipeline.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::{Error, Result};
use crate::jobs::JobStore;
use crate::library::Library;
use crate::types::JobResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    /// Source library job id (must resolve to a local video file).
    pub job_id: String,
    /// Inclusive start seconds.
    #[serde(default)]
    pub in_s: f64,
    /// Exclusive end seconds; 0 / omitted = full remaining duration (probed).
    #[serde(default)]
    pub out_s: f64,
    /// Audio gain in dB for this clip's own audio (0 = unchanged).
    #[serde(default)]
    pub gain_db: f64,
    /// Captions owned by this clip; times are **segment-local** seconds.
    #[serde(default)]
    pub captions: Vec<TextOverlay>,
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
    #[serde(default = "default_fontsize")]
    pub fontsize: u32,
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
fn default_fontsize() -> u32 {
    28
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTimeline {
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

fn resolve_canvas(tl: &VideoTimeline, first: &MediaProbe) -> Canvas {
    let w = if tl.width >= 2 { tl.width } else { first.width };
    let h = if tl.height >= 2 {
        tl.height
    } else {
        first.height
    };
    let fps = if tl.fps >= 1 {
        tl.fps.min(120)
    } else {
        let f = first.fps.round() as u32;
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

/// The cutting-room normalization recipe: aspect-fit onto the canvas, pad the
/// remainder, unify frame rate and pixel format. Every segment passes through
/// this, which is what makes the later `-c copy` concat valid.
fn norm_filter(c: Canvas) -> String {
    format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,fps={fps},format=yuv420p",
        w = c.w,
        h = c.h,
        fps = c.fps
    )
}

#[derive(Debug, Clone)]
struct SegmentPlan {
    src: PathBuf,
    in_s: f64,
    dur: f64,
    /// Offset of this segment on the master clock.
    start_master: f64,
    has_audio: bool,
    gain_db: f64,
    /// Segment-local captions (clip-owned + mapped timeline overlays).
    captions: Vec<TextOverlay>,
    fade_in_s: f64,
    fade_out_s: f64,
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

/// Build the segment plan from probed sources. Durations come from ffprobe —
/// there is no fallback constant. Errors are honest (bad in/out, empty clip).
fn plan_segments(
    tl: &VideoTimeline,
    sources: &[(PathBuf, &TimelineClip)],
    probes: &[MediaProbe],
) -> Result<(Vec<SegmentPlan>, f64)> {
    let mut plans = Vec::with_capacity(sources.len());
    let mut cursor = 0.0f64;
    let last = sources.len() - 1;
    for (i, ((path, clip), probe)) in sources.iter().zip(probes).enumerate() {
        let in_s = clip.in_s.max(0.0);
        let dur = if clip.out_s > in_s {
            clip.out_s - in_s
        } else {
            probe.duration_s - in_s
        };
        if dur <= 0.05 {
            return Err(Error::other(format!(
                "clip {i} ({}): empty window — in_s {:.3} / out_s {:.3} against source duration {:.3}",
                clip.job_id, clip.in_s, clip.out_s, probe.duration_s
            )));
        }
        let mut captions: Vec<TextOverlay> = clip
            .captions
            .iter()
            .filter(|c| c.end_s - c.start_s > 0.001)
            .cloned()
            .collect();
        captions.extend(
            tl.overlays
                .iter()
                .filter_map(|ov| overlay_in_segment(ov, cursor, dur)),
        );
        if captions.len() > MAX_CAPTIONS_PER_SEGMENT {
            return Err(Error::other(format!(
                "clip {i} ({}): {} captions exceed the per-segment cap of {MAX_CAPTIONS_PER_SEGMENT}",
                clip.job_id,
                captions.len()
            )));
        }
        plans.push(SegmentPlan {
            src: path.clone(),
            in_s,
            dur,
            start_master: cursor,
            has_audio: probe.has_audio,
            gain_db: clip.gain_db,
            captions,
            fade_in_s: if i == 0 { tl.video_fade_in_s } else { 0.0 },
            fade_out_s: if i == last { tl.video_fade_out_s } else { 0.0 },
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
    let mut vf = vec![norm_filter(canvas)];
    if plan.fade_in_s > 0.0 {
        vf.push(format!("fade=t=in:st=0:d={:.3}", plan.fade_in_s));
    }
    if plan.fade_out_s > 0.0 {
        let st = (plan.dur - plan.fade_out_s).max(0.0);
        vf.push(format!("fade=t=out:st={st:.3}:d={:.3}", plan.fade_out_s));
    }
    for cap in &plan.captions {
        let escaped = escape_drawtext(&cap.text);
        vf.push(format!(
            "drawtext=text='{escaped}':expansion=none:x={}:y={}:fontsize={}:fontcolor=white:borderw=2:bordercolor=black:enable='between(t\\,{:.3}\\,{:.3})'",
            cap.x, cap.y, cap.fontsize, cap.start_s, cap.end_s
        ));
    }
    vf.join(",")
}

/// One normalized, **silent** segment. A fixed track timescale keeps every
/// segment's mp4 timebase identical — belt-and-braces for the `-c copy` concat.
fn segment_args(plan: &SegmentPlan, canvas: Canvas, out: &Path) -> Vec<String> {
    let mut args = base_args();
    if plan.in_s > 0.0 {
        args.extend(["-ss".into(), format!("{:.3}", plan.in_s)]);
    }
    args.extend(["-i".into(), plan.src.display().to_string()]);
    args.extend(["-t".into(), format!("{:.3}", plan.dur)]);
    args.extend(["-vf".into(), segment_vf(plan, canvas)]);
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
    /// Bed-local fades (music only; clips fade via the mix-level fades).
    fade_in_s: f64,
    fade_out_s: f64,
    /// Post-trim length of this source (fade-out anchor).
    local_len_s: f64,
}

/// The master-clock audio mix: every source is format-unified, gained, placed
/// with `adelay`, then mixed with `normalize=0` (load-bearing — the default
/// normalize=1 divides by input count and buries the mix).
fn audio_mix_filter(chains: &[AudioChain], master_dur: f64, fade_in: f64, fade_out: f64) -> String {
    let mut parts = Vec::with_capacity(chains.len() + 1);
    for c in chains {
        let mut f = format!(
            "[{}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume={:.3}dB",
            c.input, c.gain_db
        );
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

    let mut sources: Vec<(PathBuf, &TimelineClip)> = Vec::new();
    for (i, c) in timeline.clips.iter().enumerate() {
        if !crate::library::is_safe_asset_id(&c.job_id) {
            return Err(Error::forbidden(format!("invalid job_id: {}", c.job_id)));
        }
        let path = resolve_job_media(library_root, &c.job_id)
            .ok_or_else(|| Error::other(format!("no local media for job_id {}", c.job_id)))?;
        let ext = path_ext(&path);
        if IMAGE_EXTS.contains(&ext.as_str()) {
            return Err(Error::other(format!(
                "clip {i} ({}) resolves to an image — still/Ken-Burns segments arrive in a later slice; craft clips must be video",
                c.job_id
            )));
        }
        if !VIDEO_EXTS.contains(&ext.as_str()) {
            return Err(Error::other(format!(
                "clip {i} ({}) resolves to a non-video file (.{ext}) — audio belongs in the `music` track",
                c.job_id
            )));
        }
        sources.push((path, c));
    }

    let mut probes = Vec::with_capacity(sources.len());
    for (path, clip) in &sources {
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
        probes.push(probe);
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

    let canvas = resolve_canvas(timeline, &probes[0]);
    let (plans, master_dur) = plan_segments(timeline, &sources, &probes)?;

    let work = std::env::temp_dir().join(format!(
        "imaginarium-craft-{}",
        ulid::Ulid::new().to_string()
    ));
    std::fs::create_dir_all(&work)?;

    let result = render_plans(&plans, canvas, master_dur, timeline, music, &work);
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
        .first()
        .map(|(_, c)| c.job_id.as_str())
        .unwrap_or("");
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
    );

    let _ = std::fs::remove_dir_all(&work);
    imported
}

/// Segments → concat → (optional) audio master pass. Returns the final file.
fn render_plans(
    plans: &[SegmentPlan],
    canvas: Canvas,
    master_dur: f64,
    tl: &VideoTimeline,
    music: Option<(PathBuf, MediaProbe, &AudioTrack)>,
    work: &Path,
) -> Result<PathBuf> {
    let mut list_body = String::new();
    for (i, plan) in plans.iter().enumerate() {
        let seg = work.join(format!("seg_{i:02}.mp4"));
        run_ffmpeg(&segment_args(plan, canvas, &seg))?;
        list_body.push_str(&format!("file '{}'\n", seg.display()));
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
        audio_inputs.push((plan.src.clone(), plan.in_s, plan.dur));
        chains.push(AudioChain {
            input,
            gain_db: plan.gain_db,
            delay_ms: (plan.start_master * 1000.0).round() as u64,
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
            fade_in_s: m.fade_in_s,
            fade_out_s: m.fade_out_s,
            local_len_s: local_len,
        });
    }

    if chains.is_empty() {
        return Ok(master);
    }
    let filter = audio_mix_filter(&chains, master_dur, tl.audio_fade_in_s, tl.audio_fade_out_s);
    let out = work.join("out.mp4");
    run_ffmpeg(&mux_args(&master, &audio_inputs, &filter, &out))?;
    Ok(out)
}

fn run_ffmpeg(args: &[String]) -> Result<()> {
    info!(?args, "ffmpeg craft");
    let output = std::process::Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::other(format!("spawn ffmpeg: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        warn!(%err, "ffmpeg failed");
        return Err(Error::other(format!(
            "ffmpeg failed: {}",
            err.chars().take(800).collect::<String>()
        )));
    }
    Ok(())
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
            job_id: job.into(),
            in_s,
            out_s,
            gain_db: 0.0,
            captions: vec![],
        }
    }

    fn bare_timeline(clips: Vec<TimelineClip>) -> VideoTimeline {
        VideoTimeline {
            clips,
            audio_fade_in_s: 0.0,
            audio_fade_out_s: 0.0,
            video_fade_in_s: 0.0,
            video_fade_out_s: 0.0,
            overlays: vec![],
            music: None,
            width: 0,
            height: 0,
            fps: 0,
            note: None,
        }
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
            norm_filter(c),
            "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,fps=24,format=yuv420p"
        );
    }

    #[test]
    fn canvas_dims_are_even_floored() {
        let tl = bare_timeline(vec![clip("01A", 0.0, 0.0)]);
        let c = resolve_canvas(&tl, &probe(5.0, 1921, 1081, 30.0, false));
        assert_eq!((c.w, c.h, c.fps), (1920, 1080, 30));
        // explicit odd request also lands even
        let mut tl2 = bare_timeline(vec![clip("01A", 0.0, 0.0)]);
        tl2.width = 853;
        tl2.height = 481;
        tl2.fps = 25;
        let c2 = resolve_canvas(&tl2, &probe(5.0, 640, 360, 24.0, false));
        assert_eq!((c2.w, c2.h, c2.fps), (852, 480, 25));
        // fps fallback when the probe has none
        let c3 = resolve_canvas(&tl, &probe(5.0, 640, 360, 0.0, false));
        assert_eq!(c3.fps, 24);
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
        let sources = vec![(PathBuf::from("/x/a.mp4"), &tl.clips[0])];
        let (plans, master) =
            plan_segments(&tl, &sources, &[probe(7.5, 640, 360, 24.0, true)]).unwrap();
        assert!((plans[0].dur - 6.5).abs() < 1e-9);
        assert!((master - 6.5).abs() < 1e-9);
        // a window past the end of the source is an honest error
        let tl2 = bare_timeline(vec![clip("01A", 9.0, 0.0)]);
        let sources2 = vec![(PathBuf::from("/x/a.mp4"), &tl2.clips[0])];
        assert!(plan_segments(&tl2, &sources2, &[probe(7.5, 640, 360, 24.0, true)]).is_err());
    }

    #[test]
    fn plan_places_segments_on_the_master_clock() {
        let tl = bare_timeline(vec![clip("01A", 0.0, 4.0), clip("01B", 2.0, 5.5)]);
        let sources: Vec<(PathBuf, &TimelineClip)> = vec![
            (PathBuf::from("/x/a.mp4"), &tl.clips[0]),
            (PathBuf::from("/x/b.mp4"), &tl.clips[1]),
        ];
        let probes = vec![
            probe(6.0, 640, 360, 24.0, true),
            probe(6.0, 640, 360, 24.0, false),
        ];
        let (plans, master) = plan_segments(&tl, &sources, &probes).unwrap();
        assert_eq!(plans.len(), 2);
        assert!((plans[0].start_master - 0.0).abs() < 1e-9);
        assert!((plans[1].start_master - 4.0).abs() < 1e-9);
        assert!((master - 7.5).abs() < 1e-9);
        assert!(plans[0].has_audio && !plans[1].has_audio);
    }

    #[test]
    fn overlays_map_across_segments_on_the_master_clock() {
        // Segments: [0,4) and [4,8). An overlay [5,7) previously vanished
        // (only clip 0 got overlays) — now it lands in segment 1, local [1,3).
        let ov = TextOverlay {
            text: "hi".into(),
            start_s: 5.0,
            end_s: 7.0,
            x: 40,
            y: 40,
            fontsize: 28,
        };
        assert!(overlay_in_segment(&ov, 0.0, 4.0).is_none());
        let local = overlay_in_segment(&ov, 4.0, 4.0).unwrap();
        assert!((local.start_s - 1.0).abs() < 1e-9);
        assert!((local.end_s - 3.0).abs() < 1e-9);

        // An overlay spanning the cut is split across both segments.
        let span = TextOverlay {
            text: "cut".into(),
            start_s: 3.0,
            end_s: 6.0,
            x: 0,
            y: 0,
            fontsize: 28,
        };
        let a = overlay_in_segment(&span, 0.0, 4.0).unwrap();
        let b = overlay_in_segment(&span, 4.0, 4.0).unwrap();
        assert!((a.start_s - 3.0).abs() < 1e-9 && (a.end_s - 4.0).abs() < 1e-9);
        assert!((b.start_s - 0.0).abs() < 1e-9 && (b.end_s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn caption_cap_is_enforced() {
        let mut c = clip("01A", 0.0, 4.0);
        c.captions = (0..MAX_CAPTIONS_PER_SEGMENT + 1)
            .map(|i| TextOverlay {
                text: format!("c{i}"),
                start_s: 0.0,
                end_s: 1.0,
                x: 0,
                y: 0,
                fontsize: 28,
            })
            .collect();
        let tl = bare_timeline(vec![c]);
        let sources = vec![(PathBuf::from("/x/a.mp4"), &tl.clips[0])];
        assert!(plan_segments(&tl, &sources, &[probe(6.0, 640, 360, 24.0, false)]).is_err());
    }

    #[test]
    fn segment_args_are_silent_normalized_and_nostdin() {
        let plan = SegmentPlan {
            src: PathBuf::from("/x/a.mp4"),
            in_s: 1.0,
            dur: 3.5,
            start_master: 0.0,
            has_audio: true,
            gain_db: 0.0,
            captions: vec![],
            fade_in_s: 0.5,
            fade_out_s: 0.0,
        };
        let canvas = Canvas {
            w: 640,
            h: 360,
            fps: 24,
        };
        let args = segment_args(&plan, canvas, Path::new("/w/seg_00.mp4"));
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
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                local_len_s: 4.0,
            },
            AudioChain {
                input: 2,
                gain_db: -3.0,
                delay_ms: 4000,
                fade_in_s: 0.0,
                fade_out_s: 0.0,
                local_len_s: 3.5,
            },
            // the music bed: placed at 0, faded, trimmed to the master window
            AudioChain {
                input: 3,
                gain_db: -8.0,
                delay_ms: 0,
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
    fn timeline_serde_is_backward_compatible() {
        // A pre-U2a payload (no music/captions/canvas fields) still parses.
        let old = r#"{
            "clips": [{"job_id": "01A", "in_s": 0.0, "out_s": 4.0, "gain_db": 0.0}],
            "overlays": [{"text": "hello", "start_s": 1.0, "end_s": 2.0}],
            "audio_fade_out_s": 1.0
        }"#;
        let tl: VideoTimeline = serde_json::from_str(old).unwrap();
        assert_eq!(tl.clips.len(), 1);
        assert!(tl.clips[0].captions.is_empty());
        assert!(tl.music.is_none());
        assert_eq!((tl.width, tl.height, tl.fps), (0, 0, 0));

        // And the new fields round-trip.
        let new = r#"{
            "clips": [{"job_id": "01A", "captions": [{"text": "local", "start_s": 0.5}]}],
            "music": {"job_id": "01M", "gain_db": -6.0, "fade_out_s": 2.0},
            "width": 1280, "height": 720, "fps": 24
        }"#;
        let tl2: VideoTimeline = serde_json::from_str(new).unwrap();
        assert_eq!(tl2.clips[0].captions.len(), 1);
        assert_eq!(tl2.music.as_ref().unwrap().job_id, "01M");
        assert_eq!((tl2.width, tl2.height, tl2.fps), (1280, 720, 24));
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
    fn e2e_mixed_sources_concat_with_music_bed() {
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
        // input that broke the old unnormalized `-c copy` concat.
        let a = dir.path().join("a.mp4");
        let b = dir.path().join("b.mp4");
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
            "sine=frequency=220:duration=3:sample_rate=48000",
            m.to_str().unwrap(),
        ]);

        let ja = library
            .import_bytes(&jobs, &std::fs::read(&a).unwrap(), "a.mp4", None, None)
            .unwrap();
        let jb = library
            .import_bytes(&jobs, &std::fs::read(&b).unwrap(), "b.mp4", None, None)
            .unwrap();
        let jm = library
            .import_bytes(&jobs, &std::fs::read(&m).unwrap(), "m.wav", None, None)
            .unwrap();
        // the wav import lands as an audio asset
        assert!(matches!(jm.assets[0].kind, crate::types::AssetKind::Audio));

        let tl = VideoTimeline {
            clips: vec![
                TimelineClip {
                    job_id: ja.job_id.to_string(),
                    in_s: 0.0,
                    out_s: 0.0, // probed → 2s
                    gain_db: -2.0,
                    captions: vec![TextOverlay {
                        text: "clip-local".into(),
                        start_s: 0.2,
                        end_s: 1.0,
                        x: 10,
                        y: 10,
                        fontsize: 20,
                    }],
                },
                TimelineClip {
                    job_id: jb.job_id.to_string(),
                    in_s: 0.0,
                    out_s: 0.0,
                    gain_db: 0.0,
                    captions: vec![],
                },
            ],
            audio_fade_in_s: 0.2,
            audio_fade_out_s: 0.5,
            video_fade_in_s: 0.2,
            video_fade_out_s: 0.3,
            // master-clock overlay that lives entirely in the SECOND segment —
            // the old engine dropped this one
            overlays: vec![TextOverlay {
                text: "second act".into(),
                start_s: 2.5,
                end_s: 3.5,
                x: 20,
                y: 40,
                fontsize: 24,
            }],
            music: Some(AudioTrack {
                job_id: jm.job_id.to_string(),
                in_s: 0.0,
                start_s: 0.0,
                gain_db: -6.0,
                fade_in_s: 0.3,
                fade_out_s: 0.5,
            }),
            width: 0,
            height: 0,
            fps: 0,
            note: Some("u2a e2e".into()),
        };

        let out = render_timeline(&library, &jobs, &root, &tl).unwrap();
        assert!(out.ok);
        let out_path = PathBuf::from(out.assets[0].local_path.as_ref().unwrap());
        let p = probe_media(&out_path).unwrap();
        // canvas derived from the first clip; both segments normalized onto it
        assert_eq!((p.width, p.height), (320, 240));
        assert!((p.fps - 30.0).abs() < 0.5);
        // master duration = 2s + 2s (probed, not guessed)
        assert!(
            (p.duration_s - 4.0).abs() < 0.35,
            "duration {}",
            p.duration_s
        );
        // the mix pass delivered an audio track
        assert!(p.has_audio);
    }
}
