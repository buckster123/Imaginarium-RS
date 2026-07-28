//! imaginarium-app — native Slint UI (GPL-3.0). Phase 6.0 / 6.1
//! Main thread = Slint; tokio multi-thread for HTTP (ApexOS pattern).

mod api;

use std::path::PathBuf;
use std::sync::Arc;

use api::NodeClient;
use slint::{ComponentHandle, Image, ModelRc, SharedPixelBuffer, SharedString, VecModel};

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let ui = AppWindow::new()?;

    // Prefill from env
    if let Ok(url) = std::env::var("IMAGINARIUM_URL") {
        ui.set_base_url(url.into());
    }
    if let Ok(tok) = std::env::var("IMAGINARIUM_TOKEN") {
        ui.set_token(tok.into());
    }

    let cache_dir = cache_dir()?;
    ui.set_jobs(ModelRc::new(VecModel::from(Vec::<JobRow>::new())));

    // ── Connect ─────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let handle = rt.handle().clone();
        ui.on_connect_clicked(move || {
            let ui = ui_weak.unwrap();
            let base = ui.get_base_url().to_string();
            let token = ui.get_token().to_string();
            ui.set_busy(true);
            ui.set_status_line("Connecting…".into());
            let ui_weak = ui.as_weak();
            handle.spawn(async move {
                let result = async {
                    let client = NodeClient::new(&base, &token)?;
                    let h = client.health().await?;
                    anyhow::Ok(h)
                }
                .await;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_busy(false);
                        match result {
                            Ok(h) => {
                                let prod = h.product.unwrap_or_else(|| "ok".into());
                                let ver = h.version.unwrap_or_default();
                                ui.set_conn_status("ok".into());
                                ui.set_status_line(format!("Connected · {prod} {ver}").into());
                            }
                            Err(e) => {
                                ui.set_conn_status("error".into());
                                ui.set_status_line(format!("Connect failed: {e}").into());
                            }
                        }
                    }
                })
                .ok();
            });
        });
    }

    // ── Generate ────────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let handle = rt.handle().clone();
        let cache = cache_dir.clone();
        ui.on_generate_clicked(move || {
            let ui = ui_weak.unwrap();
            let base = ui.get_base_url().to_string();
            let token = ui.get_token().to_string();
            let prompt = ui.get_prompt().to_string();
            let model = ui.get_model().to_string();
            let aspect = ui.get_aspect().to_string();
            let resolution = ui.get_resolution().to_string();
            let n = ui.get_n_images().max(1) as u32;
            if prompt.trim().is_empty() {
                ui.set_status_line("Prompt required".into());
                return;
            }
            ui.set_busy(true);
            ui.set_status_line("Generating…".into());
            ui.set_has_preview(false);
            let ui_weak = ui.as_weak();
            let cache = cache.clone();
            handle.spawn(async move {
                let outcome = async {
                    let client = NodeClient::new(&base, &token)?;
                    let job = client
                        .image_generate(
                            &prompt,
                            Some(&model),
                            n,
                            Some(&aspect),
                            Some(&resolution),
                        )
                        .await?;
                    let job_id = job
                        .get("job_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let status = job
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("done")
                        .to_string();
                    let path = if !job_id.is_empty() {
                        client.download_job_content(&job_id, &cache).await.ok()
                    } else {
                        None
                    };
                    let jobs = client.jobs_list(30).await.unwrap_or_default();
                    anyhow::Ok((job_id, status, path, jobs))
                }
                .await;

                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_busy(false);
                        match outcome {
                            Ok((job_id, status, path, jobs)) => {
                                ui.set_last_job_id(job_id.clone().into());
                                ui.set_last_status(status.into());
                                ui.set_status_line(format!("Done · job {job_id}").into());
                                if let Some(p) = path {
                                    match load_preview(&p) {
                                        Ok(img) => {
                                            ui.set_preview_image(img);
                                            ui.set_has_preview(true);
                                        }
                                        Err(e) => {
                                            ui.set_status_line(
                                                format!("Job ok but preview failed: {e}").into(),
                                            );
                                        }
                                    }
                                }
                                set_jobs_on_ui(&ui, jobs);
                            }
                            Err(e) => {
                                ui.set_conn_status("error".into());
                                ui.set_status_line(format!("Generate failed: {e}").into());
                            }
                        }
                    }
                })
                .ok();
            });
        });
    }

    // ── Refresh jobs ────────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let handle = rt.handle().clone();
        ui.on_refresh_jobs_clicked(move || {
            let ui = ui_weak.unwrap();
            let base = ui.get_base_url().to_string();
            let token = ui.get_token().to_string();
            ui.set_busy(true);
            let ui_weak = ui.as_weak();
            handle.spawn(async move {
                let result = async {
                    let client = NodeClient::new(&base, &token)?;
                    client.jobs_list(40).await
                }
                .await;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_busy(false);
                        match result {
                            Ok(jobs) => {
                                ui.set_status_line(format!("{} jobs", jobs.len()).into());
                                set_jobs_on_ui(&ui, jobs);
                            }
                            Err(e) => {
                                ui.set_status_line(format!("Jobs failed: {e}").into());
                            }
                        }
                    }
                })
                .ok();
            });
        });
    }

    // ── Open job preview ────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        let handle = rt.handle().clone();
        let cache = cache_dir.clone();
        ui.on_open_job_clicked(move |job_id| {
            let ui = ui_weak.unwrap();
            let base = ui.get_base_url().to_string();
            let token = ui.get_token().to_string();
            let job_id = job_id.to_string();
            ui.set_busy(true);
            ui.set_status_line(format!("Loading {job_id}…").into());
            let ui_weak = ui_weak.clone();
            let cache = cache.clone();
            handle.spawn(async move {
                let result = async {
                    let client = NodeClient::new(&base, &token)?;
                    client.download_job_content(&job_id, &cache).await
                }
                .await;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_busy(false);
                        match result {
                            Ok(path) => {
                                ui.set_last_job_id(job_id.into());
                                match load_preview(&path) {
                                    Ok(img) => {
                                        ui.set_preview_image(img);
                                        ui.set_has_preview(true);
                                        ui.set_status_line(
                                            format!("Loaded {}", path.display()).into(),
                                        );
                                    }
                                    Err(e) => {
                                        // video or unsupported — try xdg-open
                                        ui.set_has_preview(false);
                                        let _ = open_external(&path);
                                        ui.set_status_line(
                                            format!("Opened externally ({e})").into(),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                ui.set_status_line(format!("Open failed: {e}").into());
                            }
                        }
                    }
                })
                .ok();
            });
        });
    }

    // ── Browser studio ──────────────────────────────────────
    {
        let ui_weak = ui.as_weak();
        ui.on_open_browser_clicked(move || {
            let ui = ui_weak.unwrap();
            let url = ui.get_base_url().to_string();
            let _ = open_external_url(&url);
            ui.set_status_line(format!("Opened {url}").into());
        });
    }

    // Keep runtime alive for the app lifetime
    let _rt = Arc::new(rt);
    ui.run()?;
    Ok(())
}

fn set_jobs_on_ui(ui: &AppWindow, jobs: Vec<api::JobInfo>) {
    let rows: Vec<JobRow> = jobs
        .into_iter()
        .map(|j| JobRow {
            id: SharedString::from(j.job_id),
            mode: SharedString::from(j.mode),
            status: SharedString::from(j.status),
            summary: SharedString::from(j.summary),
        })
        .collect();
    ui.set_jobs(ModelRc::new(VecModel::from(rows)));
}

fn cache_dir() -> anyhow::Result<PathBuf> {
    let base = directories::ProjectDirs::from("dev", "buckster", "imaginarium")
        .map(|p| p.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp/imaginarium-slint"));
    let dir = base.join("preview-cache");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn load_preview(path: &std::path::Path) -> anyhow::Result<Image> {
    // Prefer Slint path load for png/jpeg
    if let Ok(img) = Image::load_from_path(path) {
        return Ok(img);
    }
    // Fallback decode via image crate → RGBA
    let dynimg = image::open(path)?;
    let rgba = dynimg.to_rgba8();
    let (w, h) = rgba.dimensions();
    let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
    Ok(Image::from_rgba8(buffer))
}

fn open_external(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
    }
    Ok(())
}

fn open_external_url(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = url;
    }
    Ok(())
}
