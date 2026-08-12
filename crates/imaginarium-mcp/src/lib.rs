//! Imaginarium MCP library — stdio JSON-RPC (Cerebro/ApexOS wire format).

mod args;
mod backend;
mod dispatch;
mod tools;
mod transport;

use std::sync::Arc;

use anyhow::{Context, Result};
use imaginarium_core::config::Config;
use tracing::info;

use backend::{Backend, LocalBackend, ProxyBackend};
use transport::{Frame, StdioTransport};

/// Run MCP stdio server until stdin closes.
///
/// - Local mode: needs `XAI_API_KEY` / config (holds key on this machine).
/// - Proxy mode: `proxy_url` or env `IMAGINARIUM_URL` + `IMAGINARIUM_TOKEN`.
pub async fn run(proxy_url: Option<String>) -> Result<()> {
    let proxy_url = proxy_url
        .or_else(|| std::env::var("IMAGINARIUM_URL").ok())
        .filter(|s| !s.trim().is_empty());

    let backend: Arc<dyn Backend> = if let Some(url) = proxy_url {
        let token = std::env::var("IMAGINARIUM_TOKEN")
            .context("IMAGINARIUM_TOKEN required when using proxy/URL mode")?;
        info!(%url, "imaginarium-mcp proxy mode");
        Arc::new(ProxyBackend::new(&url, &token))
    } else {
        let cfg = Config::load().map_err(|e| anyhow::anyhow!(e))?;
        info!(data = %cfg.data_home.display(), "imaginarium-mcp local mode");
        Arc::new(LocalBackend::new(cfg)?)
    };

    let mut transport = StdioTransport::new();

    let init_req = match transport.read().await? {
        Frame::Value(v) => v,
        Frame::Eof => {
            info!("stdin closed before initialize");
            return Ok(());
        }
        Frame::ParseError(e) => {
            tracing::error!("malformed initialize: {e}");
            transport.write(&dispatch::parse_error()).await?;
            return Ok(());
        }
        Frame::Oversized { bytes } => {
            tracing::error!("oversized initialize ({bytes} bytes)");
            transport.write(&dispatch::parse_error()).await?;
            return Ok(());
        }
    };

    let init_resp = if init_req["method"].as_str() == Some("initialize") {
        dispatch::handle_initialize(&init_req)
    } else {
        dispatch::method_not_found(&init_req)
    };
    transport.write(&init_resp).await?;

    loop {
        let msg = match transport.read().await {
            Err(e) => {
                tracing::error!("transport IO: {e}");
                break;
            }
            Ok(Frame::Eof) => break,
            Ok(Frame::ParseError(e)) => {
                tracing::warn!("malformed frame: {e}");
                transport.write(&dispatch::parse_error()).await?;
                continue;
            }
            Ok(Frame::Oversized { bytes }) => {
                tracing::warn!("oversized frame ({bytes} bytes)");
                transport.write(&dispatch::parse_error()).await?;
                continue;
            }
            Ok(Frame::Value(v)) => v,
        };

        let is_notification = msg["id"].is_null()
            || msg["method"]
                .as_str()
                .map(|m| m.starts_with("notifications/"))
                .unwrap_or(false);
        if is_notification {
            continue;
        }

        let method = msg["method"].as_str().unwrap_or("").to_string();
        match method.as_str() {
            "tools/list" => {
                transport.write(&dispatch::tools_list(&msg)).await?;
            }
            "ping" => {
                transport
                    .write(&serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": msg["id"],
                        "result": {}
                    }))
                    .await?;
            }
            "tools/call" => {
                // Don't hold the stdio loop on a long video wait — hosts send
                // ping / tools/list while a generate is in flight.
                let backend = Arc::clone(&backend);
                let writer = transport.writer();
                tokio::spawn(async move {
                    let resp = dispatch::dispatch_tool(msg, backend).await;
                    if let Err(e) = writer.write(&resp).await {
                        tracing::error!("mcp write after tools/call: {e}");
                    }
                });
            }
            _ => {
                transport.write(&dispatch::method_not_found(&msg)).await?;
            }
        }
    }

    info!("imaginarium-mcp exiting");
    Ok(())
}

/// Parse `--proxy URL` from argv (for binary main).
pub fn proxy_from_args(args: &[String]) -> Option<String> {
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--proxy" || args[i] == "-p" {
            return args.get(i + 1).cloned();
        }
        if let Some(rest) = args[i].strip_prefix("--proxy=") {
            return Some(rest.to_string());
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initialize_schema() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        let resp = dispatch::handle_initialize(&req);
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "imaginarium-mcp");
    }

    #[test]
    fn tools_list_has_core_surface() {
        let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        let resp = dispatch::tools_list(&req);
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"imaginarium_image_generate"));
        assert!(names.contains(&"imaginarium_video_generate"));
        assert!(names.contains(&"imaginarium_job_wait"));
        assert!(names.len() >= 10);
    }

    #[test]
    fn proxy_arg_parse() {
        let args = vec![
            "imaginarium-mcp".into(),
            "--proxy".into(),
            "http://x:8791".into(),
        ];
        assert_eq!(proxy_from_args(&args).as_deref(), Some("http://x:8791"));
    }
}
