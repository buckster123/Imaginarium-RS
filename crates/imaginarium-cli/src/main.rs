//! Imaginarium CLI — `imaginarium`

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use imaginarium_core::client::{
    models_table_json, ImageEditRequest, ImageGenerateRequest, ImagineClient, ResponseFormat,
    VideoEditRequest, VideoExtendRequest, VideoGenerateRequest,
};
use imaginarium_core::config::Config;
use imaginarium_core::estimate::{self, CostEstimate};
use imaginarium_core::jobs::JobStore;
use imaginarium_core::library::Library;
use imaginarium_core::models::{self, ModelId};
use imaginarium_core::types::MediaRef;
use imaginarium_core::{DEFAULT_BIND, PRODUCT, VERSION};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "imaginarium",
    version = VERSION,
    about = "Imaginarium-RS — local-first xAI Imagine studio gateway (CLI / MCP / API)"
)]
struct Cli {
    /// Path to config.toml (default: XDG / IMAGINARIUM_CONFIG)
    #[arg(long, global = true, env = "IMAGINARIUM_CONFIG")]
    config: Option<PathBuf>,

    /// Override data home (default: XDG / IMAGINARIUM_HOME)
    #[arg(long, global = true, env = "IMAGINARIUM_HOME")]
    data_home: Option<PathBuf>,

    #[arg(long, global = true, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print version and product info
    Version,
    /// List Imagine models and capability matrix
    Models {
        #[arg(long)]
        json: bool,
    },
    /// Configuration
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Estimate approximate USD cost
    Estimate {
        #[command(subcommand)]
        kind: EstimateCmd,
    },
    /// Image generation / edit
    #[command(subcommand)]
    Image(ImageCmd),
    /// Video generation / edit / extend
    #[command(subcommand)]
    Video(VideoCmd),
    /// Local job history
    #[command(subcommand)]
    Jobs(JobsCmd),
    /// Mint / list / revoke LAN API tokens (ApexOS-compatible)
    #[command(subcommand)]
    Token(TokenCmd),
    /// Local library helpers
    #[command(subcommand)]
    Library(LibraryCmd),
    /// Run MCP server (Phase 4 stub)
    Mcp {
        /// Proxy all tools to a remote Imaginarium node
        #[arg(long)]
        proxy: Option<String>,
    },
    /// Run HTTP API (LAN token gate; non-loopback requires token)
    Serve {
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: String,
        /// Allow unauthenticated requests only when bind is loopback
        #[arg(long)]
        allow_localhost_no_auth: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Write default config.toml if missing
    Init,
    /// Show resolved config paths and redacted settings
    Show,
    /// Print config file path
    Path,
}

#[derive(Subcommand, Debug)]
enum EstimateCmd {
    Image {
        #[arg(long, default_value = "image")]
        model: String,
        #[arg(long, default_value_t = 1)]
        n: u32,
    },
    Video {
        #[arg(long, default_value = "video")]
        model: String,
        #[arg(long, default_value_t = 8)]
        duration: u32,
    },
}

#[derive(Subcommand, Debug)]
enum ImageCmd {
    /// Text-to-image
    Gen {
        #[arg(short = 'p', long)]
        prompt: String,
        #[arg(long, default_value = "image")]
        model: String,
        #[arg(long, default_value_t = 1)]
        n: u32,
        #[arg(long = "ar")]
        aspect_ratio: Option<String>,
        #[arg(long = "res")]
        resolution: Option<String>,
        #[arg(long, value_enum, default_value_t = CliResponseFormat::Url)]
        format: CliResponseFormat,
        #[arg(long)]
        json: bool,
    },
    /// Edit one or more images (max 3)
    Edit {
        #[arg(short = 'p', long)]
        prompt: String,
        /// Source image path, URL, data URI, or file_id (repeatable, max 3)
        #[arg(long = "image", required = true)]
        images: Vec<String>,
        #[arg(long, default_value = "image")]
        model: String,
        #[arg(long, default_value_t = 1)]
        n: u32,
        #[arg(long = "ar")]
        aspect_ratio: Option<String>,
        #[arg(long = "res")]
        resolution: Option<String>,
        #[arg(long, value_enum, default_value_t = CliResponseFormat::Url)]
        format: CliResponseFormat,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum VideoCmd {
    /// Text-to-video
    Gen {
        #[arg(short = 'p', long)]
        prompt: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 8)]
        duration: u32,
        #[arg(long = "ar")]
        aspect_ratio: Option<String>,
        #[arg(long = "res")]
        resolution: Option<String>,
        /// Submit and return immediately (poll with status/wait)
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Image-to-video (defaults to grok-imagine-video-1.5)
    I2v {
        #[arg(long)]
        image: String,
        #[arg(short = 'p', long)]
        prompt: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 8)]
        duration: u32,
        #[arg(long = "ar")]
        aspect_ratio: Option<String>,
        #[arg(long = "res")]
        resolution: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Reference-to-video (one or more --ref images)
    Ref {
        #[arg(short = 'p', long)]
        prompt: String,
        #[arg(long = "ref", required = true)]
        references: Vec<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 8)]
        duration: u32,
        #[arg(long = "ar")]
        aspect_ratio: Option<String>,
        #[arg(long = "res")]
        resolution: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Edit an existing video
    Edit {
        #[arg(short = 'p', long)]
        prompt: String,
        #[arg(long)]
        video: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Extend a video from its last frame (duration = extension segment)
    Extend {
        #[arg(short = 'p', long)]
        prompt: String,
        #[arg(long)]
        video: String,
        #[arg(long, default_value_t = 6)]
        duration: u32,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// One-shot status poll (also checks upstream if pending)
    Status {
        job_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Block until job completes or poll timeout
    Wait {
        job_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum JobsCmd {
    Ls {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Get {
        job_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TokenCmd {
    /// Create a token (prints plaintext once)
    Create {
        #[arg(long, default_value = "default")]
        label: String,
        /// admin | write | read
        #[arg(long, default_value = "write")]
        scope: String,
        #[arg(long)]
        json: bool,
    },
    /// List minted tokens (hashes never shown)
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Revoke by id
    Revoke { id: String },
}

#[derive(Subcommand, Debug)]
enum LibraryCmd {
    /// Print library root path
    Path,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliResponseFormat {
    Url,
    B64,
}

impl From<CliResponseFormat> for ResponseFormat {
    fn from(value: CliResponseFormat) -> Self {
        match value {
            CliResponseFormat::Url => ResponseFormat::Url,
            CliResponseFormat::B64 => ResponseFormat::B64Json,
        }
    }
}

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!("imaginarium={level},imaginarium_core={level}"))
    });
    // Log to stderr: the `mcp` subcommand owns stdout for the JSON-RPC stream, and
    // any log line on stdout corrupts it. stderr is correct for every subcommand.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}

fn load_cfg(cli: &Cli) -> Result<Config> {
    if let Some(home) = &cli.data_home {
        std::env::set_var("IMAGINARIUM_HOME", home);
    }
    if let Some(cfg) = &cli.config {
        std::env::set_var("IMAGINARIUM_CONFIG", cfg);
    }
    Config::load().map_err(Into::into)
}

fn print_estimate(e: &CostEstimate) {
    println!(
        "model={}  units={} {}  ≈ ${:.4}  ({})",
        e.model, e.units, e.unit, e.estimated_usd, e.note
    );
}

fn parse_optional_model(s: Option<&str>) -> Result<(Option<ModelId>, bool)> {
    match s {
        None => Ok((None, false)),
        Some(raw) => Ok((Some(ModelId::parse(raw)?), true)),
    }
}

fn print_job_result(result: &imaginarium_core::types::JobResult, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }
    println!(
        "job_id={} status={} model={}",
        result.job_id,
        result.status.as_str(),
        result.model
    );
    if let Some(rid) = &result.upstream_request_id {
        println!("upstream_request_id={rid}");
    }
    for (i, a) in result.assets.iter().enumerate() {
        println!(
            "  [{i}] kind={:?} local={} upstream={} public={}",
            a.kind,
            a.local_path.as_deref().unwrap_or("-"),
            a.upstream_url.as_deref().unwrap_or("-"),
            a.public_url.as_deref().unwrap_or("-")
        );
    }
    if let Some(err) = &result.error {
        println!("error={err}");
    }
    if let Some(u) = &result.usage {
        if let Some(usd) = u.estimated_usd {
            println!("estimated_usd≈{usd:.4}");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);

    match cli.command {
        Commands::Version => {
            println!("{PRODUCT} v{VERSION}");
            println!("bin=imaginarium  default_bind={DEFAULT_BIND}");
        }
        Commands::Models { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&models_table_json())?);
            } else {
                println!("{PRODUCT} models\n");
                for m in models::catalog() {
                    println!(
                        "  {}  [{}]\n    {}\n    ≈ ${:.3} / {}\n",
                        m.id, m.kind, m.notes, m.approx_usd_unit, m.unit
                    );
                }
            }
        }
        Commands::Config(ref cmd) => match cmd {
            ConfigCmd::Init => {
                let path = Config::init_file()?;
                println!("config ready: {}", path.display());
            }
            ConfigCmd::Path => {
                println!("{}", imaginarium_core::paths::config_path()?.display());
            }
            ConfigCmd::Show => {
                let cfg = load_cfg(&cli)?;
                println!("config_path: {}", cfg.config_path.display());
                println!("data_home:   {}", cfg.data_home.display());
                println!("library:     {}", cfg.library_dir().display());
                println!("db:          {}", cfg.db_path().display());
                println!("base_url:    {}", cfg.base_url());
                println!("bind:        {}", cfg.server.bind);
                println!("storage:     {}", cfg.storage.profile);
                println!("auto_download: {}", cfg.storage.auto_download);
                let key_state = match cfg.resolve_api_key() {
                    Ok(k) => format!("set ({} chars)", k.len()),
                    Err(_) => "missing".into(),
                };
                println!("api_key:     {key_state}");
            }
        },
        Commands::Estimate { kind } => match kind {
            EstimateCmd::Image { model, n } => {
                let m = ModelId::parse(&model)?;
                print_estimate(&estimate::estimate_image(m, n));
            }
            EstimateCmd::Video { model, duration } => {
                let m = ModelId::parse(&model)?;
                print_estimate(&estimate::estimate_video(m, duration));
            }
        },
        Commands::Image(ref cmd) => {
            let cfg = load_cfg(&cli)?;
            let client = ImagineClient::from_config(&cfg)?;
            let library = Library::new(cfg.library_dir());
            let store = JobStore::open(&cfg.db_path())?;
            match cmd {
                ImageCmd::Gen {
                    prompt,
                    model,
                    n,
                    aspect_ratio,
                    resolution,
                    format,
                    json,
                } => {
                    let model = ModelId::parse(model)?;
                    let result = client
                        .image_generate(
                            ImageGenerateRequest {
                                prompt: prompt.clone(),
                                model,
                                n: *n,
                                aspect_ratio: aspect_ratio.clone(),
                                resolution: resolution
                                    .clone()
                                    .or_else(|| Some(cfg.defaults.image_resolution.clone())),
                                response_format: (*format).into(),
                            },
                            &library,
                            Some(&store),
                        )
                        .await
                        .context("image generate failed")?;
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("job_id={} status={}", result.job_id, result.status.as_str());
                        for (i, a) in result.assets.iter().enumerate() {
                            println!(
                                "  [{i}] local={} upstream={}",
                                a.local_path.as_deref().unwrap_or("-"),
                                a.upstream_url.as_deref().unwrap_or("-")
                            );
                        }
                        if let Some(u) = &result.usage {
                            if let Some(usd) = u.estimated_usd {
                                println!("estimated_usd≈{usd:.4}");
                            }
                        }
                    }
                }
                ImageCmd::Edit {
                    prompt,
                    images,
                    model,
                    n,
                    aspect_ratio,
                    resolution,
                    format,
                    json,
                } => {
                    let model = ModelId::parse(model)?;
                    let refs: Vec<MediaRef> = images
                        .iter()
                        .map(|s| MediaRef::from_user_input(s))
                        .collect();
                    let result = client
                        .image_edit(
                            ImageEditRequest {
                                prompt: prompt.clone(),
                                model,
                                images: refs,
                                n: *n,
                                aspect_ratio: aspect_ratio.clone(),
                                resolution: resolution.clone(),
                                response_format: (*format).into(),
                            },
                            &library,
                            Some(&store),
                        )
                        .await
                        .context("image edit failed")?;
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("job_id={} status={}", result.job_id, result.status.as_str());
                        for (i, a) in result.assets.iter().enumerate() {
                            println!("  [{i}] local={}", a.local_path.as_deref().unwrap_or("-"));
                        }
                    }
                }
            }
        }
        Commands::Video(ref cmd) => {
            let cfg = load_cfg(&cli)?;
            match cmd {
                VideoCmd::Gen {
                    prompt,
                    model,
                    duration,
                    aspect_ratio,
                    resolution,
                    no_wait,
                    json,
                } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let (model, explicit) = parse_optional_model(model.as_deref())?;
                    let result = client
                        .video_generate(
                            VideoGenerateRequest {
                                prompt: Some(prompt.clone()),
                                model,
                                explicit_model: explicit,
                                image: None,
                                reference_images: vec![],
                                duration: Some(*duration),
                                aspect_ratio: aspect_ratio.clone().or_else(|| Some("16:9".into())),
                                resolution: resolution
                                    .clone()
                                    .or_else(|| Some(cfg.defaults.video_resolution.clone())),
                            },
                            &library,
                            Some(&store),
                            !*no_wait,
                        )
                        .await
                        .context("video generate failed")?;
                    print_job_result(&result, *json)?;
                }
                VideoCmd::I2v {
                    image,
                    prompt,
                    model,
                    duration,
                    aspect_ratio,
                    resolution,
                    no_wait,
                    json,
                } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let (model, explicit) = parse_optional_model(model.as_deref())?;
                    let result = client
                        .video_generate(
                            VideoGenerateRequest {
                                prompt: prompt.clone(),
                                model,
                                explicit_model: explicit,
                                image: Some(MediaRef::from_user_input(image)),
                                reference_images: vec![],
                                duration: Some(*duration),
                                aspect_ratio: aspect_ratio.clone(),
                                resolution: resolution.clone(),
                            },
                            &library,
                            Some(&store),
                            !*no_wait,
                        )
                        .await
                        .context("video i2v failed")?;
                    print_job_result(&result, *json)?;
                }
                VideoCmd::Ref {
                    prompt,
                    references,
                    model,
                    duration,
                    aspect_ratio,
                    resolution,
                    no_wait,
                    json,
                } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let (model, explicit) = parse_optional_model(model.as_deref())?;
                    let refs: Vec<MediaRef> = references
                        .iter()
                        .map(|s| MediaRef::from_user_input(s))
                        .collect();
                    let result = client
                        .video_generate(
                            VideoGenerateRequest {
                                prompt: Some(prompt.clone()),
                                model,
                                explicit_model: explicit,
                                image: None,
                                reference_images: refs,
                                duration: Some(*duration),
                                aspect_ratio: aspect_ratio.clone().or_else(|| Some("16:9".into())),
                                resolution: resolution
                                    .clone()
                                    .or_else(|| Some(cfg.defaults.video_resolution.clone())),
                            },
                            &library,
                            Some(&store),
                            !*no_wait,
                        )
                        .await
                        .context("video ref failed")?;
                    print_job_result(&result, *json)?;
                }
                VideoCmd::Edit {
                    prompt,
                    video,
                    model,
                    no_wait,
                    json,
                } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let (model, _) = parse_optional_model(model.as_deref())?;
                    let result = client
                        .video_edit(
                            VideoEditRequest {
                                prompt: prompt.clone(),
                                video: MediaRef::from_user_input(video),
                                model,
                            },
                            &library,
                            Some(&store),
                            !*no_wait,
                        )
                        .await
                        .context("video edit failed")?;
                    print_job_result(&result, *json)?;
                }
                VideoCmd::Extend {
                    prompt,
                    video,
                    duration,
                    model,
                    no_wait,
                    json,
                } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let (model, _) = parse_optional_model(model.as_deref())?;
                    let result = client
                        .video_extend(
                            VideoExtendRequest {
                                prompt: prompt.clone(),
                                video: MediaRef::from_user_input(video),
                                duration: Some(*duration),
                                model,
                            },
                            &library,
                            Some(&store),
                            !*no_wait,
                        )
                        .await
                        .context("video extend failed")?;
                    print_job_result(&result, *json)?;
                }
                VideoCmd::Status { job_id, json } => {
                    let store = JobStore::open(&cfg.db_path())?;
                    let library = Library::new(cfg.library_dir());
                    let jid = imaginarium_core::types::JobId(job_id.clone());
                    // If we have credentials, hit upstream once; else local only.
                    let result = match ImagineClient::from_config(&cfg) {
                        Ok(client) => client
                            .video_status_once(&jid, &library, &store)
                            .await
                            .context("status failed")?,
                        Err(_) => store
                            .get(&jid)?
                            .ok_or_else(|| anyhow::anyhow!("job not found: {job_id}"))?,
                    };
                    print_job_result(&result, *json)?;
                }
                VideoCmd::Wait { job_id, json } => {
                    let client = ImagineClient::from_config(&cfg)?;
                    let library = Library::new(cfg.library_dir());
                    let store = JobStore::open(&cfg.db_path())?;
                    let result = client
                        .video_wait(
                            &imaginarium_core::types::JobId(job_id.clone()),
                            &library,
                            &store,
                        )
                        .await
                        .context("wait failed")?;
                    print_job_result(&result, *json)?;
                }
            }
        }
        Commands::Jobs(ref cmd) => {
            let cfg = load_cfg(&cli)?;
            let store = JobStore::open(&cfg.db_path())?;
            match cmd {
                JobsCmd::Ls { limit, json } => {
                    let items = store.list_recent(*limit)?;
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&items)?);
                    } else if items.is_empty() {
                        println!("(no jobs yet)");
                    } else {
                        for it in items {
                            println!(
                                "{}  {}  {}  {}  {}",
                                it.job_id, it.status, it.mode, it.model, it.created_at
                            );
                        }
                    }
                }
                JobsCmd::Get { job_id, json: _ } => {
                    match store.get(&imaginarium_core::types::JobId(job_id.clone()))? {
                        Some(j) => {
                            println!("{}", serde_json::to_string_pretty(&j)?);
                        }
                        None => bail!("job not found: {job_id}"),
                    }
                }
            }
        }
        Commands::Library(LibraryCmd::Path) => {
            let cfg = load_cfg(&cli)?;
            println!("{}", cfg.library_dir().display());
        }
        Commands::Token(ref cmd) => {
            let cfg = load_cfg(&cli)?;
            let store = imaginarium_core::TokenStore::open(
                &cfg.tokens_db_path(),
                cfg.resolve_node_token(),
            )?;
            match cmd {
                TokenCmd::Create { label, scope, json } => {
                    let scope = imaginarium_core::TokenScope::parse(scope)?;
                    let minted = store.mint(label, scope)?;
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&minted)?);
                    } else {
                        println!("id={}", minted.id);
                        println!("label={}", minted.label);
                        println!("scope={}", minted.scope);
                        println!("token={}  (store now — shown once)", minted.token);
                        println!(
                            "export IMAGINARIUM_TOKEN={}  # optional node-wide admin alias",
                            minted.token
                        );
                    }
                }
                TokenCmd::Ls { json } => {
                    let list = store.list()?;
                    if *json {
                        let pub_list: Vec<_> = list
                            .iter()
                            .map(|t| {
                                serde_json::json!({
                                    "id": t.id,
                                    "label": t.label,
                                    "scope": t.scope,
                                    "created_at": t.created_at,
                                })
                            })
                            .collect();
                        println!("{}", serde_json::to_string_pretty(&pub_list)?);
                    } else if list.is_empty() {
                        println!("(no minted tokens — set IMAGINARIUM_TOKEN or create one)");
                    } else {
                        for t in list {
                            println!("{}  {}  {}  {}", t.id, t.scope, t.label, t.created_at);
                        }
                    }
                }
                TokenCmd::Revoke { id } => {
                    if store.revoke(id)? {
                        println!("revoked {id}");
                    } else {
                        bail!("token not found: {id}");
                    }
                }
            }
        }
        Commands::Mcp { proxy } => {
            // Logs must stay on stderr; this takes over stdio for JSON-RPC.
            imaginarium_mcp::run(proxy.clone()).await?;
        }
        Commands::Serve {
            ref bind,
            allow_localhost_no_auth,
        } => {
            let cfg = load_cfg(&cli)?;
            imaginarium_server::serve_from_config(cfg, Some(bind.clone()), allow_localhost_no_auth)
                .await?;
        }
    }

    Ok(())
}
