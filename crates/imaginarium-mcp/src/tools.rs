//! MCP tool schemas.

use serde_json::{json, Value};

pub fn all_tool_schemas() -> Vec<Value> {
    vec![
        tool(
            "imaginarium_models",
            "List Grok Imagine models and capability matrix (T2I/I2V/R2V/edit/extend, resolutions).",
            json!({ "type": "object", "properties": {} }),
        ),
        tool(
            "imaginarium_estimate",
            "Estimate approximate USD cost before spending. kind=image|video.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["image", "video"] },
                    "model": { "type": "string", "description": "Model id or alias (image, quality, video, 1.5)" },
                    "n": { "type": "integer", "description": "Image count" },
                    "duration": { "type": "integer", "description": "Video seconds" }
                },
                "required": ["kind"]
            }),
        ),
        tool(
            "imaginarium_image_generate",
            "Text-to-image via Grok Imagine. Returns job_id, local paths, upstream URLs. Auto-downloads to library.",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "model": { "type": "string", "description": "image | quality | full model id" },
                    "n": { "type": "integer", "default": 1 },
                    "aspect_ratio": { "type": "string" },
                    "resolution": { "type": "string", "enum": ["1k", "2k"] }
                },
                "required": ["prompt"]
            }),
        ),
        tool(
            "imaginarium_image_edit",
            "Edit 1–3 source images with a text prompt. images: library:{job_id} refs (chain a previous generation — preferred, never expires), URLs, data: URLs, or file_ids. library:{job_id}#2 addresses the n-th image of a batch.",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "images": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 3 },
                    "model": { "type": "string" },
                    "n": { "type": "integer", "default": 1 },
                    "aspect_ratio": { "type": "string" },
                    "resolution": { "type": "string" }
                },
                "required": ["prompt", "images"]
            }),
        ),
        tool(
            "imaginarium_video_generate",
            "Generate video: text-to-video (prompt only), image-to-video (image=), or reference-to-video (reference_images=). For I2V from a previous generation pass image=library:{job_id} (preferred — resolves on the node, never expires; upstream URLs do). Do not combine image + reference_images. Defaults wait until done; set no_wait=true then use job_status/job_wait. I2V auto-selects video-1.5 (1080p ok).",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "image": { "type": "string", "description": "Start frame for I2V: library:{job_id}, URL, data: URL, or file_id" },
                    "reference_images": { "type": "array", "items": { "type": "string" } },
                    "model": { "type": "string" },
                    "duration": { "type": "integer", "minimum": 1, "maximum": 15 },
                    "aspect_ratio": { "type": "string" },
                    "resolution": { "type": "string", "enum": ["480p", "720p", "1080p"] },
                    "no_wait": { "type": "boolean", "default": false }
                }
            }),
        ),
        tool(
            "imaginarium_video_edit",
            "Edit an existing video with a text prompt. video: library:{job_id} (chain a previous video — preferred), URL, data: URL, or file_id.",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "video": { "type": "string" },
                    "model": { "type": "string" },
                    "no_wait": { "type": "boolean", "default": false }
                },
                "required": ["prompt", "video"]
            }),
        ),
        tool(
            "imaginarium_video_extend",
            "Extend video from last frame. video: library:{job_id} (chain a previous video — preferred), URL, or file_id. duration is extension segment only (2–10s, default 6).",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "video": { "type": "string" },
                    "duration": { "type": "integer", "minimum": 2, "maximum": 10 },
                    "model": { "type": "string" },
                    "no_wait": { "type": "boolean", "default": false }
                },
                "required": ["prompt", "video"]
            }),
        ),
        tool(
            "imaginarium_job_status",
            "One-shot status for a local job_id (polls upstream if pending).",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"]
            }),
        ),
        tool(
            "imaginarium_job_wait",
            "Block until job completes or poll timeout (config poll.timeout_s, default 600). Prefer for long video when client timeout allows.",
            json!({
                "type": "object",
                "properties": {
                    "job_id": { "type": "string" }
                },
                "required": ["job_id"]
            }),
        ),
        tool(
            "imaginarium_jobs_list",
            "List recent local jobs (id, status, mode, model).",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 20 }
                }
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}
