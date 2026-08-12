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
                    "model": { "type": "string", "description": "Model id or alias (image, quality, 2.0, video, 1.5)" },
                    "n": { "type": "integer", "description": "Image count" },
                    "duration": { "type": "integer", "description": "Video seconds" },
                    "resolution": { "type": "string", "enum": ["480p", "720p", "1080p"], "description": "Video only. Omitted → 720p (not the 480p floor)." }
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
                    "model": { "type": "string", "description": "image | quality | 2.0 | full model id" },
                    "n": { "type": "integer", "default": 1 },
                    "aspect_ratio": { "type": "string" },
                    "resolution": { "type": "string", "enum": ["1k", "2k"] },
                    "quality": { "type": "string", "enum": ["low", "medium"], "description": "Image 2.0 only; omit for upstream default medium" }
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
                    "resolution": { "type": "string" },
                    "quality": { "type": "string", "enum": ["low", "medium"], "description": "Image 2.0 only; omit for upstream default medium" }
                },
                "required": ["prompt", "images"]
            }),
        ),
        tool(
            "imaginarium_video_generate",
            "Generate video via grok-imagine-video-1.5 by default. Modes: text-to-video (prompt only), image-to-video (image=), reference-to-video (reference_images and/or reference_audios). Audio-only R2V is valid (prompt + voices, no images). Tag refs in the prompt as <IMAGE_0>… and <AUDIO_0>…. Do not combine image + reference_*. 1080p is T2V/I2V only; R2V + voices cap at 720p. For I2V from a previous generation pass image=library:{job_id}. Defaults wait until done; set no_wait=true then job_status/job_wait.",
            json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "image": { "type": "string", "description": "Start frame for I2V: library:{job_id}, URL, data: URL, or file_id" },
                    "reference_images": { "type": "array", "items": { "type": "string" } },
                    "reference_audios": { "type": "array", "items": { "type": "string" }, "maxItems": 3, "description": "Video 1.5 preset voice_ids (eve, ara, leo, rex, …). Tag <AUDIO_0>…" },
                    "voice_id": { "type": "string", "description": "Shorthand for a single reference_audios entry" },
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
            "Extend video from last frame. video: library:{job_id} (chain a previous video — preferred), URL, or file_id. duration is extension segment only (2–10s, default 6). Values outside that range are rejected, not clamped.",
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
            "imaginarium_craft_video",
            "Cut finished library jobs into ONE edited video on the node (free, ffmpeg — no upstream spend). \
             timeline = VideoTimeline v1: {version:1, clips:[segments], music?, style?, fades?}. Segment kinds: \
             {job_id, in_s?, out_s?, speed? 0.5-2, gain_db?} = video clip · {kind:'still', job_id, dur_s, zoom_to? 1-3} \
             = Ken-Burns image · {kind:'card', dur_s, card_color?} = title card. Every segment takes captions: \
             [{text, start_s, end_s}] in SEGMENT-LOCAL seconds. music: {job_id, gain_db?, fade_in_s?, fade_out_s?} \
             = an audio library job mixed under everything (import a track first, or use one already imported). \
             style: {letterbox_frac? 0-0.45, letterbox_reveal_s?, loudnorm?, card_bg?, caption_color?}. \
             Timeline-level video/audio_fade_in/out_s fade the whole piece. Default (wait=false) returns a \
             pending job immediately — poll imaginarium_job_status until done, then the result is a normal \
             library job (content/thumb routes, chainable). wait=true blocks until rendered.",
            json!({
                "type": "object",
                "properties": {
                    "timeline": { "type": "object", "description": "VideoTimeline v1 (see tool description)" },
                    "wait": { "type": "boolean", "default": false }
                },
                "required": ["timeline"]
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
                    "limit": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100, "description": "Capped at 100." }
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
