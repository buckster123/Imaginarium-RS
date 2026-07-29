//! Local video craft — timeline → ffmpeg → library job (Studio+ 5.3).

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
    /// Source library job id (must resolve to a local media file).
    pub job_id: String,
    /// Inclusive start seconds.
    #[serde(default)]
    pub in_s: f64,
    /// Exclusive end seconds; 0 / omitted = full remaining duration (best-effort).
    #[serde(default)]
    pub out_s: f64,
    /// Audio gain in dB (0 = unchanged).
    #[serde(default)]
    pub gain_db: f64,
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
    #[serde(default)]
    pub overlays: Vec<TextOverlay>,
    /// Optional note stored on the craft job.
    #[serde(default)]
    pub note: Option<String>,
}

/// Resolve first media file for a job under library root (YYYY/MM/DD/job_id/).
/// Delegates to the library's indexed resolver (index 0 = the historic
/// first-file behavior) so craft renders and the content route share one walk.
pub fn resolve_job_media(library_root: &Path, job_id: &str) -> Option<PathBuf> {
    crate::library::resolve_job_asset(library_root, job_id, 0)
}

pub fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
    if !ffmpeg_available() {
        return Err(Error::other(
            "ffmpeg not found on PATH — install ffmpeg for video craft",
        ));
    }

    let mut sources: Vec<(PathBuf, &TimelineClip)> = Vec::new();
    for c in &timeline.clips {
        if !crate::library::is_safe_asset_id(&c.job_id) {
            return Err(Error::forbidden(format!("invalid job_id: {}", c.job_id)));
        }
        let path = resolve_job_media(library_root, &c.job_id)
            .ok_or_else(|| Error::other(format!("no local media for job_id {}", c.job_id)))?;
        sources.push((path, c));
    }

    let work = std::env::temp_dir().join(format!(
        "imaginarium-craft-{}",
        ulid::Ulid::new().to_string()
    ));
    std::fs::create_dir_all(&work)?;
    let out_path = work.join("out.mp4");

    let result = if sources.len() == 1 {
        render_single(&sources[0].0, sources[0].1, timeline, &out_path)
    } else {
        render_multi(&sources, timeline, &work, &out_path)
    };

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&work);
        return Err(e);
    }

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

fn render_single(path: &Path, clip: &TimelineClip, tl: &VideoTimeline, out: &Path) -> Result<()> {
    let in_s = clip.in_s.max(0.0);
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];
    if in_s > 0.0 {
        args.extend(["-ss".into(), format!("{in_s:.3}")]);
    }
    args.extend(["-i".into(), path.display().to_string()]);
    if clip.out_s > in_s {
        let dur = clip.out_s - in_s;
        args.extend(["-t".into(), format!("{dur:.3}")]);
    }

    let mut vf = Vec::new();
    let mut af = Vec::new();

    let duration_hint = if clip.out_s > in_s {
        clip.out_s - in_s
    } else {
        6.0 // fallback for fade-out start
    };

    if tl.video_fade_in_s > 0.0 {
        vf.push(format!("fade=t=in:st=0:d={:.3}", tl.video_fade_in_s));
    }
    if tl.video_fade_out_s > 0.0 {
        let st = (duration_hint - tl.video_fade_out_s).max(0.0);
        vf.push(format!(
            "fade=t=out:st={st:.3}:d={:.3}",
            tl.video_fade_out_s
        ));
    }
    for ov in &tl.overlays {
        let escaped = escape_drawtext(&ov.text);
        vf.push(format!(
            "drawtext=text='{escaped}':expansion=none:x={}:y={}:fontsize={}:fontcolor=white:borderw=2:bordercolor=black:enable='between(t\\,{:.3}\\,{:.3})'",
            ov.x, ov.y, ov.fontsize, ov.start_s, ov.end_s
        ));
    }

    if clip.gain_db.abs() > 0.001 {
        af.push(format!("volume={:.3}dB", clip.gain_db));
    }
    if tl.audio_fade_in_s > 0.0 {
        af.push(format!("afade=t=in:st=0:d={:.3}", tl.audio_fade_in_s));
    }
    if tl.audio_fade_out_s > 0.0 {
        let st = (duration_hint - tl.audio_fade_out_s).max(0.0);
        af.push(format!(
            "afade=t=out:st={st:.3}:d={:.3}",
            tl.audio_fade_out_s
        ));
    }

    if !vf.is_empty() {
        args.extend(["-vf".into(), vf.join(",")]);
    }
    if !af.is_empty() {
        args.extend(["-af".into(), af.join(",")]);
    }

    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-crf".into(),
        "23".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "128k".into(),
        "-movflags".into(),
        "+faststart".into(),
        out.display().to_string(),
    ]);

    run_ffmpeg(&args)
}

fn render_multi(
    sources: &[(PathBuf, &TimelineClip)],
    tl: &VideoTimeline,
    work: &Path,
    out: &Path,
) -> Result<()> {
    // Normalize each clip to a short mp4 segment, then concat demuxer.
    let mut list_body = String::new();
    for (i, (path, clip)) in sources.iter().enumerate() {
        let seg = work.join(format!("seg_{i:02}.mp4"));
        let mini = VideoTimeline {
            clips: vec![(*clip).clone()],
            audio_fade_in_s: if i == 0 { tl.audio_fade_in_s } else { 0.0 },
            audio_fade_out_s: if i + 1 == sources.len() {
                tl.audio_fade_out_s
            } else {
                0.0
            },
            video_fade_in_s: if i == 0 { tl.video_fade_in_s } else { 0.0 },
            video_fade_out_s: if i + 1 == sources.len() {
                tl.video_fade_out_s
            } else {
                0.0
            },
            overlays: if i == 0 { tl.overlays.clone() } else { vec![] },
            note: None,
        };
        render_single(path, clip, &mini, &seg)?;
        list_body.push_str(&format!("file '{}'\n", seg.display()));
    }
    let list_path = work.join("concat.txt");
    std::fs::write(&list_path, list_body)?;

    let args = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.display().to_string(),
        "-c".into(),
        "copy".into(),
        out.display().to_string(),
    ];
    run_ffmpeg(&args)
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
}
