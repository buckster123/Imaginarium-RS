//! Local asset library helpers (download + layout + craft import).

use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::info;

use crate::error::{Error, Result};
use crate::jobs::JobStore;
use crate::types::{Asset, AssetId, AssetKind, JobId, JobMode, JobResult, JobStatus, MediaRef};

#[derive(Debug, Clone)]
pub struct Library {
    /// Root of the library tree (…/library).
    pub root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `library/YYYY/MM/DD/<job_id>/`
    pub fn ensure_job_dir(&self, job_id: &JobId) -> Result<PathBuf> {
        let when = Utc::now();
        let dir = self
            .root
            .join(when.format("%Y").to_string())
            .join(when.format("%m").to_string())
            .join(when.format("%d").to_string())
            .join(job_id.as_str());
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn write_meta(&self, job_dir: &Path, meta: &impl serde::Serialize) -> Result<PathBuf> {
        let path = job_dir.join("meta.json");
        let body = serde_json::to_vec_pretty(meta)?;
        std::fs::write(&path, body)?;
        Ok(path)
    }

    pub fn write_prompt(&self, job_dir: &Path, prompt: &str) -> Result<PathBuf> {
        let path = job_dir.join("prompt.txt");
        std::fs::write(&path, prompt)?;
        Ok(path)
    }

    /// Import raw bytes as a new completed library job (Studio+ craft / upload).
    /// `provenance` (when given) is stored under `meta.json`'s `provenance` key —
    /// craft renders record their full timeline + engine/tool versions there.
    pub fn import_bytes(
        &self,
        jobs: &JobStore,
        bytes: &[u8],
        filename_hint: &str,
        note: Option<&str>,
        source_job_id: Option<&str>,
        provenance: Option<&serde_json::Value>,
    ) -> Result<JobResult> {
        let ext = extension_from_name(filename_hint)
            .unwrap_or_else(|| sniff_ext(bytes).unwrap_or("bin").to_string());
        let kind = match ext.as_str() {
            "mp4" | "webm" | "mov" | "mkv" => AssetKind::Video,
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "opus" => AssetKind::Audio,
            _ => AssetKind::Image,
        };
        let job_id = JobId::new();
        let dir = self.ensure_job_dir(&job_id)?;
        let file_name = format!("00.{ext}");
        let path = dir.join(&file_name);
        std::fs::write(&path, bytes)?;

        let note = note.unwrap_or("craft import");
        let _ = self.write_prompt(&dir, note);
        let mut meta = serde_json::json!({
            "source": "library_import",
            "filename_hint": filename_hint,
            "source_job_id": source_job_id,
            "bytes": bytes.len(),
        });
        if let Some(p) = provenance {
            meta["provenance"] = p.clone();
        }
        let _ = self.write_meta(&dir, &meta);

        let content_url = format!("/v1/library/{}/content", job_id.as_str());
        let result = JobResult {
            ok: true,
            job_id: job_id.clone(),
            upstream_request_id: None,
            status: JobStatus::Done,
            mode: JobMode::CraftExport,
            model: "local-craft".into(),
            assets: vec![Asset {
                id: AssetId::new(),
                kind,
                local_path: Some(path.display().to_string()),
                content_url: Some(content_url),
                upstream_url: None,
                file_id: None,
                public_url: None,
                mime_type: Some(mime_for_ext(&ext).into()),
            }],
            usage: None,
            error: None,
            error_type: None,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            prompt: Some(note.into()),
        };
        jobs.upsert_result(&result)?;
        info!(job_id = %job_id, path = %path.display(), "library import");
        Ok(result)
    }
}

/// True if `id` is a safe library path segment (a job/asset id): non-empty, bounded,
/// and composed only of `[A-Za-z0-9_-]`. Used to guard every `…/{id}/…` filesystem
/// join against path traversal (`..`, `/`, `\`, percent-encoded separators). ULIDs
/// (Crockford base32) satisfy this.
pub fn is_safe_asset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse a `library:{job_id}` / `library:{job_id}#{n}` chain reference — the
/// first-class way for clients (native UI, MCP agents, chained edits) to feed a
/// node-library asset back into generation without downloading + re-encoding it
/// as a data URL. Returns `(job_id, asset_index)`; None when it isn't a library
/// ref at all OR the id/index is malformed (an unsafe id never resolves).
pub fn parse_library_ref(s: &str) -> Option<(String, u32)> {
    let rest = s.trim().strip_prefix("library:")?;
    let (id, idx) = match rest.split_once('#') {
        Some((id, n)) => (id, n.parse::<u32>().ok()?),
        None => (rest, 0),
    };
    if !is_safe_asset_id(id) {
        return None;
    }
    Some((id.to_string(), idx))
}

/// Media extensions the library serves / resolves, preference-ordered
/// (video first — matches the historic first-file behavior; mov/mkv cover
/// craft imports; audio last so mixed dirs keep resolving their visual asset).
const MEDIA_EXTS: &[&str] = &[
    "mp4", "webm", "mov", "mkv", "png", "jpg", "jpeg", "webp", "mp3", "wav", "flac", "ogg", "m4a",
    "opus",
];

/// Resolve the `index`-th media asset of a job under the library root
/// (`YYYY/MM/DD/{job_id}/`). Index 0 keeps the historic first-file behavior
/// (incl. the legacy `0.*` names); higher indices address `{index:02}.*` so an
/// n>1 image batch is fully reachable. Traversal-guarded by `is_safe_asset_id`.
pub fn resolve_job_asset(library_root: &Path, job_id: &str, index: u32) -> Option<PathBuf> {
    if !is_safe_asset_id(job_id) {
        return None;
    }
    if !library_root.is_dir() {
        return None;
    }
    for year in std::fs::read_dir(library_root).ok()? {
        let year = year.ok()?.path();
        if !year.is_dir() {
            continue;
        }
        for month in std::fs::read_dir(&year).ok()? {
            let month = month.ok()?.path();
            for day in std::fs::read_dir(&month).ok()? {
                let day = day.ok()?.path();
                let job_dir = day.join(job_id);
                if !job_dir.is_dir() {
                    continue;
                }
                // Exact names first: NN.ext (+ the legacy 0.* forms at index 0).
                for ext in MEDIA_EXTS {
                    let p = job_dir.join(format!("{index:02}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
                if index == 0 {
                    for name in ["0.mp4", "0.png"] {
                        let p = job_dir.join(name);
                        if p.is_file() {
                            return Some(p);
                        }
                    }
                }
                // Fallback: the index-th media file in name order (imports with
                // arbitrary names stay addressable).
                let mut media: Vec<PathBuf> = std::fs::read_dir(&job_dir)
                    .ok()?
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.extension()
                                .and_then(|e| e.to_str())
                                .map(|e| MEDIA_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                                .unwrap_or(false)
                    })
                    .collect();
                media.sort();
                return media.into_iter().nth(index as usize);
            }
        }
    }
    None
}

/// Build a MediaRef from a NODE-SIDE input: `library:{job_id}[#n]` resolves to
/// the asset's local path (validated + confined by `resolve_job_asset`), and
/// everything else passes through the strict remote gate (`from_remote_input` —
/// bare filesystem paths stay rejected). This is what the HTTP routes and the
/// MCP local backend call: the ONLY way a remote-supplied string becomes a
/// `MediaRef::Path` is via a validated library id.
pub fn media_from_node_input(s: &str, library_root: &Path) -> Result<MediaRef> {
    if let Some((job_id, idx)) = parse_library_ref(s) {
        let path = resolve_job_asset(library_root, &job_id, idx).ok_or_else(|| {
            Error::other(format!(
                "library asset not found: {job_id}#{idx} (is the job id right, and does it have media?)"
            ))
        })?;
        return Ok(MediaRef::Path {
            path: path.to_string_lossy().into_owned(),
        });
    }
    MediaRef::from_remote_input(s)
}

/// CLI-side variant: `library:` refs resolve exactly like the node input, and
/// anything else keeps the permissive local behavior (bare paths allowed —
/// `from_user_input` is the local CLI's privilege).
pub fn media_from_local_input(s: &str, library_root: &Path) -> MediaRef {
    if let Some((job_id, idx)) = parse_library_ref(s) {
        if let Some(path) = resolve_job_asset(library_root, &job_id, idx) {
            return MediaRef::Path {
                path: path.to_string_lossy().into_owned(),
            };
        }
    }
    MediaRef::from_user_input(s)
}

fn extension_from_name(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn sniff_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("png");
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some("wav");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.starts_with(b"fLaC") {
        return Some("flac");
    }
    if bytes.starts_with(b"OggS") {
        return Some("ogg");
    }
    // mp3: ID3 tag or a bare MPEG frame sync (11 set bits); jpeg's 0xFFD8 is
    // checked above and its second byte fails the 0xE0 mask, so no collision.
    if bytes.starts_with(b"ID3") {
        return Some("mp3");
    }
    if bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0 {
        return Some("mp3");
    }
    if bytes.windows(4).any(|w| w == b"ftyp") {
        return Some("mp4");
    }
    None
}

/// Content type for a library asset extension — the single map shared by
/// imports and the HTTP content route.
pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "opus" => "audio/opus",
        _ => "application/octet-stream",
    }
}

/// Download a URL to `dest`.
pub async fn download_url(client: &reqwest::Client, url: &str, dest: &Path) -> Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let resp = client.get(url).send().await?.error_for_status()?;
    let bytes = resp.bytes().await?;
    let len = bytes.len() as u64;
    std::fs::write(dest, &bytes)?;
    info!(path = %dest.display(), bytes = len, "downloaded asset");
    Ok(len)
}

/// Decode `data:image/png;base64,...` or raw base64 into bytes + suggested ext.
pub fn decode_data_url_or_b64(input: &str) -> Result<(Vec<u8>, String)> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("data:") {
        let (meta, b64) = rest
            .split_once(',')
            .ok_or_else(|| Error::other("malformed data URL"))?;
        let mime = meta.split(';').next().unwrap_or("application/octet-stream");
        let ext = match mime {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "video/mp4" => "mp4",
            "video/webm" => "webm",
            "audio/mpeg" | "audio/mp3" => "mp3",
            "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
            "audio/flac" | "audio/x-flac" => "flac",
            "audio/ogg" => "ogg",
            "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
            "audio/opus" => "opus",
            _ => "bin",
        };
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| Error::other(format!("base64: {e}")))?;
        return Ok((bytes, ext.into()));
    }
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| Error::other(format!("base64: {e}")))?;
    let ext = sniff_ext(&bytes).unwrap_or("bin").to_string();
    Ok((bytes, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_asset_id_accepts_ids_rejects_traversal() {
        assert!(is_safe_asset_id("01KYMRHFX5GSSMTWF5W9RTCT9D"));
        assert!(is_safe_asset_id("craft-01_abc"));
        assert!(!is_safe_asset_id(""));
        assert!(!is_safe_asset_id("a/b"));
        assert!(!is_safe_asset_id("../../etc/passwd"));
        assert!(!is_safe_asset_id("..%2f..%2fetc"));
        assert!(!is_safe_asset_id("a\\b"));
        assert!(!is_safe_asset_id(&"x".repeat(65)));
    }

    #[test]
    fn library_ref_parses_and_guards() {
        assert_eq!(
            parse_library_ref("library:01ABC"),
            Some(("01ABC".into(), 0))
        );
        assert_eq!(
            parse_library_ref(" library:01ABC#2 "),
            Some(("01ABC".into(), 2))
        );
        // Not a library ref → None (falls through to the normal gates).
        assert_eq!(parse_library_ref("https://x/y.png"), None);
        assert_eq!(parse_library_ref("/etc/shadow"), None);
        // Traversal / malformed forms never resolve.
        assert_eq!(parse_library_ref("library:../etc"), None);
        assert_eq!(parse_library_ref("library:a/b"), None);
        assert_eq!(parse_library_ref("library:01ABC#x"), None);
        assert_eq!(parse_library_ref("library:"), None);
    }

    #[test]
    fn resolve_job_asset_addresses_batches() {
        let dir = tempfile::tempdir().unwrap();
        let job = dir.path().join("2026").join("07").join("29").join("01JOB");
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(job.join("00.png"), b"a").unwrap();
        std::fs::write(job.join("01.png"), b"b").unwrap();
        std::fs::write(job.join("meta.json"), b"{}").unwrap();

        let p0 = resolve_job_asset(dir.path(), "01JOB", 0).unwrap();
        let p1 = resolve_job_asset(dir.path(), "01JOB", 1).unwrap();
        assert!(p0.ends_with("00.png"));
        assert!(p1.ends_with("01.png"));
        // Out-of-range index and unknown job → None; meta.json never resolves.
        assert!(resolve_job_asset(dir.path(), "01JOB", 5).is_none());
        assert!(resolve_job_asset(dir.path(), "NOPE", 0).is_none());
        assert!(resolve_job_asset(dir.path(), "../01JOB", 0).is_none());
    }

    #[test]
    fn audio_sniffs_kinds_and_mimes() {
        // header sniffing for the music-bed formats
        assert_eq!(sniff_ext(b"RIFF\x00\x00\x00\x00WAVEfmt "), Some("wav"));
        assert_eq!(sniff_ext(b"fLaC\x00\x00\x00\x22"), Some("flac"));
        assert_eq!(sniff_ext(b"OggS\x00\x02"), Some("ogg"));
        assert_eq!(sniff_ext(b"ID3\x04\x00"), Some("mp3"));
        assert_eq!(sniff_ext(&[0xff, 0xfb, 0x90, 0x00]), Some("mp3"));
        // jpeg keeps winning over the mp3 frame-sync heuristic
        assert_eq!(sniff_ext(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));

        assert_eq!(mime_for_ext("mp3"), "audio/mpeg");
        assert_eq!(mime_for_ext("wav"), "audio/wav");
        assert_eq!(mime_for_ext("mov"), "video/quicktime");

        // data-URL audio imports map to audio extensions
        let (bytes, ext) = decode_data_url_or_b64("data:audio/wav;base64,UklGRg==").unwrap();
        assert_eq!(ext, "wav");
        assert_eq!(&bytes[..4], b"RIFF");
    }

    #[test]
    fn import_kinds_cover_audio() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::new(dir.path().join("library"));
        let jobs = JobStore::open(&dir.path().join("jobs.sqlite")).unwrap();
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
        let r = library
            .import_bytes(&jobs, &wav, "sonus-track.wav", Some("a bed"), None, None)
            .unwrap();
        assert!(matches!(r.assets[0].kind, AssetKind::Audio));
        assert_eq!(r.assets[0].mime_type.as_deref(), Some("audio/wav"));
        // and the imported track resolves as a library asset (music job_id path)
        let p = resolve_job_asset(&library.root, r.job_id.as_str(), 0).unwrap();
        assert!(p.ends_with("00.wav"));
    }

    #[test]
    fn media_from_node_input_resolves_library_and_keeps_the_gate() {
        let dir = tempfile::tempdir().unwrap();
        let job = dir.path().join("2026").join("07").join("29").join("01JOB");
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(job.join("00.png"), b"a").unwrap();

        // library ref → Path, pointing inside the library tree.
        match media_from_node_input("library:01JOB", dir.path()).unwrap() {
            MediaRef::Path { path } => assert!(path.ends_with("00.png")),
            other => panic!("expected Path, got {other:?}"),
        }
        // Unknown job → a clear error, not a silent pass-through.
        assert!(media_from_node_input("library:GHOST", dir.path()).is_err());
        // The remote gate is unchanged: bare paths still rejected, URLs still pass.
        assert!(media_from_node_input("/etc/shadow", dir.path()).is_err());
        assert!(media_from_node_input("https://x/y.png", dir.path()).is_ok());
    }
}
