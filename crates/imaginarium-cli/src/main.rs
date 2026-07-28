//! Imaginarium CLI — `imaginarium`

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use imaginarium_core::client::{
    models_table_json, ImageEditRequest, ImageGenerateRequest, ImagineClient, ResponseFormat,
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
    /// Video commands (Phase 2 — not yet implemented)
    #[command(subcommand)]
    Video(VideoCmd),
    /// Local job history
    #[command(subcommand)]
    Jobs(JobsCmd),
    /// Local library helpers
    #[command(subcommand)]
    Library(LibraryCmd),
    /// Run MCP server (Phase 4 stub)
    Mcp {
        /// Proxy all tools to a remote Imaginarium node
        #[arg(long)]
        proxy: Option<String>,
    },
    /// Run HTTP API + UI (Phase 3 stub)
    Serve {
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: String,
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
    Gen {
        #[arg(short = 'p', long)]
        prompt: String,
    },
    I2v {
        #[arg(long)]
        image: String,
        #[arg(short = 'p', long)]
        prompt: Option<String>,
    },
    Status {
        job_id: String,
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
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
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
        Commands::Video(ref cmd) => match cmd {
            VideoCmd::Gen { .. } | VideoCmd::I2v { .. } => {
                bail!("video generation lands in Phase 2 — models catalog is ready (`imaginarium models`)");
            }
            VideoCmd::Status { job_id } => {
                let cfg = load_cfg(&cli)?;
                let store = JobStore::open(&cfg.db_path())?;
                match store.get(&imaginarium_core::types::JobId(job_id.clone()))? {
                    Some(j) => println!("{}", serde_json::to_string_pretty(&j)?),
                    None => bail!("job not found: {job_id}"),
                }
            }
        },
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
        Commands::Mcp { proxy } => {
            let msg = imaginarium_core::mcp_stub_message(proxy.as_deref());
            eprintln!("{msg}");
            bail!("MCP server not implemented yet (Phase 4)");
        }
        Commands::Serve { bind } => {
            eprintln!(
                "serve stub — will bind {bind} in Phase 3 (API) / Phase 5 (Vue UI). default={}",
                DEFAULT_BIND
            );
            bail!("HTTP server not implemented yet (Phase 3)");
        }
    }

    Ok(())
}
