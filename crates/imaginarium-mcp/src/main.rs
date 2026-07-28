//! imaginarium-mcp binary — MCP-over-stdio for Grok Imagine.
//! Logs → stderr. stdout is JSON-RPC only.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let proxy = imaginarium_mcp::proxy_from_args(&args);
    imaginarium_mcp::run(proxy).await
}
