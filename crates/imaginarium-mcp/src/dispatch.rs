//! JSON-RPC dispatch.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::backend::Backend;
use crate::tools;

pub fn handle_initialize(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "imaginarium-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

pub fn tools_list(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "tools": tools::all_tool_schemas() }
    })
}

pub fn method_not_found(req: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "error": { "code": -32601, "message": "method not found" }
    })
}

pub fn parse_error() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": { "code": -32700, "message": "parse error" }
    })
}

pub async fn dispatch_tool(msg: Value, backend: Arc<dyn Backend>) -> Value {
    let id = msg["id"].clone();
    let params = &msg["params"];
    let name = params["name"].as_str().unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = route(name, &args, backend).await;
    match result {
        Ok(v) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": v.to_string() }]
            }
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": e.to_string() }
        }),
    }
}

async fn route(name: &str, args: &Value, backend: Arc<dyn Backend>) -> anyhow::Result<Value> {
    match name {
        "imaginarium_models" => backend.models().await,
        "imaginarium_estimate" => {
            let kind = args["kind"].as_str().unwrap_or("image");
            let model = args["model"].as_str();
            let n = crate::args::u32_or(args, "n", 1)?;
            let duration = crate::args::u32_or(args, "duration", 8)?;
            let resolution = args["resolution"].as_str();
            backend.estimate(kind, model, n, duration, resolution).await
        }
        "imaginarium_image_generate" => backend.image_generate(args).await,
        "imaginarium_image_edit" => backend.image_edit(args).await,
        "imaginarium_video_generate" => backend.video_generate(args).await,
        "imaginarium_video_edit" => backend.video_edit(args).await,
        "imaginarium_video_extend" => backend.video_extend(args).await,
        "imaginarium_craft_video" => backend.craft_video(args).await,
        "imaginarium_job_status" => {
            let job_id = args["job_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("job_id required"))?;
            backend.job_status(job_id).await
        }
        "imaginarium_job_wait" => {
            let job_id = args["job_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("job_id required"))?;
            backend.job_wait(job_id).await
        }
        "imaginarium_jobs_list" => {
            let limit = crate::args::u32_or(args, "limit", 20)? as usize;
            backend.jobs_list(limit).await
        }
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}
