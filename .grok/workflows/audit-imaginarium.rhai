let meta = #{
    name: "audit-imaginarium",
    description: "Full-tree audit swarm for Imaginarium-RS: six specialists plus backlog rematch, adversarial verify, synthesized report",
    when_to_use: "When asking for the state of Imaginarium-RS, a security/API audit, or whether BACKLOG.md is still true",
    phases: [
        #{ title: "Scan", detail: "six specialists plus July-2026 backlog rematch" },
        #{ title: "Verify", detail: "one skeptic per finding, fail closed" },
        #{ title: "Report", detail: "write confirmed findings to scratch/report.md" },
    ],
};

const FINDINGS_SCHEMA = #{
    "type": "object",
    "required": ["findings"],
    "properties": #{
        "findings": #{
            "type": "array",
            "maxItems": 6,
            "items": #{
                "type": "object",
                "required": ["file", "issue", "severity", "id"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "severity": #{ "type": "string" },
                    "id": #{ "type": "string" },
                },
            },
        },
    },
};

const REMATCH_SCHEMA = #{
    "type": "object",
    "required": ["items"],
    "properties": #{
        "items": #{
            "type": "array",
            "maxItems": 40,
            "items": #{
                "type": "object",
                "required": ["id", "still_open", "file", "evidence"],
                "properties": #{
                    "id": #{ "type": "string" },
                    "still_open": #{ "type": "boolean" },
                    "file": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                },
            },
        },
    },
};

const VERDICT_SCHEMA = #{
    "type": "object",
    "required": ["real", "reason", "evidence"],
    "properties": #{
        "real": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "evidence": #{ "type": "string" },
    },
};

const SYNTH_SCHEMA = #{
    "type": "object",
    "required": ["title", "headline", "body"],
    "properties": #{
        "title": #{ "type": "string" },
        "headline": #{ "type": "string" },
        "body": #{ "type": "string" },
    },
};

fn starts_with(s, prefix) {
    if type_of(s) != "string" { return false; }
    if type_of(prefix) != "string" { return false; }
    let n = prefix.len();
    if s.len() < n { return false; }
    s.sub_string(0, n) == prefix
}

fn in_scope(path) {
    if starts_with(path, "crates/") { return true; }
    if starts_with(path, "ui-web/src/") { return true; }
    if starts_with(path, "openapi/") { return true; }
    if starts_with(path, "docs/") { return true; }
    if starts_with(path, "SECURITY.md") { return true; }
    if starts_with(path, "BACKLOG.md") { return true; }
    if starts_with(path, "README.md") { return true; }
    false
}

fn preamble() {
    let p = "";
    p += "You are a read-only auditor of Imaginarium-RS.\n";
    p += "Repo root: /home/andre/Projects/Imaginarium-RS\n";
    p += "USE TOOLS. Open files with read_file and grep. Do not answer from memory.\n";
    p += "Do not edit files. Do not run cargo test unless you must confirm a compile claim; prefer reading.\n";
    p += "An empty findings list is valid ONLY after you have actually opened the files you were told to read.\n";
    p += "Scope: crates/, ui-web/src/, openapi/, docs/. file must be a repo-relative path.\n";
    p += "Max 6 findings. Prefer concrete, currently-true defects over style nits.\n";
    p += "BACKLOG.md and SECURITY.md are from 2026-07-28 and are STALE. Many items were later fixed\n";
    p += "(e.g. MediaRef::from_remote_input, stderr tracing, auto model selector, video timeout not Failed).\n";
    p += "Do NOT re-report a July item unless CURRENT code still has the bug. Cite the current line.\n";
    p += "severity must be one of: critical, high, medium, low.\n";
    p += "id is a short slug (e.g. api-quality-leak or G1 if it is a still-open backlog item).\n";
    p
}

let root_note = "/home/andre/Projects/Imaginarium-RS";

phase("Scan");
log("Launching six specialists plus backlog rematch");

let scan_jobs = [];

let p_api = preamble();
p_api += "DIMENSION: xAI Imagine API fidelity.\n";
p_api += "Read: crates/imaginarium-core/src/client.rs, models.rs, estimate.rs, types.rs,\n";
p_api += "crates/imaginarium-mcp/src/tools.rs, backend.rs,\n";
p_api += "crates/imaginarium-server/src/routes.rs, crates/imaginarium-cli/src/main.rs.\n";
p_api += "Known official API (Aug 2026): POST /v1/images/generations and /v1/images/edits;\n";
p_api += "POST /v1/videos/generations, /edits, /extensions; GET /v1/videos/{request_id}.\n";
p_api += "Image body: model, prompt, n, aspect_ratio, resolution 1k|2k, response_format url|b64_json,\n";
p_api += "storage_options, user. Image 2.0 ONLY adds quality=low|medium (default medium).\n";
p_api += "Edit: image XOR images[]; multi-ref tokens <IMAGE_0>. Official docs still max 3 source images.\n";
p_api += "Video: duration 1-15, aspect_ratio, resolution 480p|720p|1080p, image (I2V) XOR reference_images (R2V),\n";
p_api += "x-idempotency-key, poll status pending|done|failed|expired. Video 1.5 is T2V/I2V/R2V; 1080p is T2V/I2V only (R2V 720p).\n";
p_api += "Hunt: wrong endpoints, dropped fields, sending quality on non-2.0, 1080p on legacy video,\n";
p_api += "I2V+R2V combined, silent clamp vs validate, estimate vs real pricing ($0.02/$0.05/$0.04 image,\n";
p_api += "$0.05/s video, $0.08/s 1.5), b64 discarded, usage ticks ignored, response parse holes.\n";
scan_jobs.push(#{
    prompt: p_api,
    label: "scan:xai-api",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_sec = preamble();
p_sec += "DIMENSION: security and trust boundary.\n";
p_sec += "Read: crates/imaginarium-core/src/types.rs, library.rs, tokens.rs, craft_video.rs,\n";
p_sec += "crates/imaginarium-server/src/auth.rs, routes.rs, lib.rs,\n";
p_sec += "crates/imaginarium-mcp/src/backend.rs, transport.rs.\n";
p_sec += "July G1 (path exfil) was addressed with from_remote_input + media_from_node_input.\n";
p_sec += "Confirm EVERY network/MCP media field uses the remote gate, not from_user_input.\n";
p_sec += "Check library:{job_id} resolution cannot escape the library root.\n";
p_sec += "Hunt: path traversal on /v1/library/{id}/content and thumb, craft resolve_job_media,\n";
p_sec += "SSRF via caller-supplied http URLs fetched by the node, token in query logs,\n";
p_sec += "CORS, ffmpeg drawtext injection, oversized decode-before-cap, auth bypass on bind-string.\n";
scan_jobs.push(#{
    prompt: p_sec,
    label: "scan:security",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_job = preamble();
p_job += "DIMENSION: job lifecycle and money honesty.\n";
p_job += "Read: crates/imaginarium-core/src/client.rs (finalize_image_job, finish_video_job,\n";
p_job += "complete_video_from_body, video_wait, video_status_once), jobs.rs, estimate.rs, library.rs.\n";
p_job += "July B1 claimed timeout marked Failed (unrecoverable paid job). Check CURRENT timeout path.\n";
p_job += "Hunt: ok=true with no assets, running stuck on local post-steps, download fail swallowed,\n";
p_job += "SQLite last-writer clobber of a done row, estimate ignoring resolution/quality/n,\n";
p_job += "wait route 502 on image jobs, no spend cap, proxy job_status never polling upstream.\n";
scan_jobs.push(#{
    prompt: p_job,
    label: "scan:jobs-money",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_surf = preamble();
p_surf += "DIMENSION: MCP / CLI / HTTP / OpenAPI surface consistency.\n";
p_surf += "Read: crates/imaginarium-mcp/src/*.rs, crates/imaginarium-cli/src/main.rs,\n";
p_surf += "crates/imaginarium-server/src/routes.rs, openapi/imaginarium-v1.yaml,\n";
p_surf += "docs/AGENTS.md, docs/APEXOS_IMAGINARIUM.md, ui-web/src/components/ImageStudio.vue.\n";
p_surf += "Hunt: schema vs handler drift (quality, model aliases 2.0, library: refs),\n";
p_surf += "MCP proxy holes (job_status no poll, no content_url, config flags ignored),\n";
p_surf += "stdio JSON-RPC: stdout logs, UTF-8 kill, re-init handshake, serial blocking video,\n";
p_surf += "string args silently defaulted, promised-but-absent routes in the ApexOS brief.\n";
scan_jobs.push(#{
    prompt: p_surf,
    label: "scan:surfaces",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_craft = preamble();
p_craft += "DIMENSION: craft/ffmpeg engine and local library.\n";
p_craft += "Read: crates/imaginarium-core/src/craft_video.rs, library.rs, jobs.rs,\n";
p_craft += "crates/imaginarium-server/src/routes.rs (import, craft render, thumb),\n";
p_craft += "crates/imaginarium-mcp craft tool + backend.\n";
p_craft += "Hunt: command injection, unsanitized job ids in ffmpeg args, unbounded timelines,\n";
p_craft += "filtergraph injection via captions/overlays, temp-file races, cache-key collisions,\n";
p_craft += "audio/video stream-copy hazards, import decode-before-size-cap, RAM-buffer whole assets.\n";
scan_jobs.push(#{
    prompt: p_craft,
    label: "scan:craft",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_auth = preamble();
p_auth += "DIMENSION: auth, tokens, LAN embed.\n";
p_auth += "Read: crates/imaginarium-core/src/tokens.rs, config.rs,\n";
p_auth += "crates/imaginarium-server/src/auth.rs, lib.rs, routes.rs.\n";
p_auth += "July G3: localhost Admin bypass keyed off cfg.server.bind string, api_router ignored the flag.\n";
p_auth += "Check whether ConnectInfo peer.ip().is_loopback() is used NOW.\n";
p_auth += "Hunt: scope map mistakes (estimate Write, wait Read-but-writes), token compare not constant-time,\n";
p_auth += "plaintext tokens on disk, query-string token logging, missing refusal when non-loopback and no tokens.\n";
scan_jobs.push(#{
    prompt: p_auth,
    label: "scan:auth",
    capability_mode: "read-only",
    output_schema: FINDINGS_SCHEMA,
});

let p_rem = "";
p_rem += "You are a read-only rematch auditor of Imaginarium-RS at /home/andre/Projects/Imaginarium-RS.\n";
p_rem += "USE TOOLS. Read BACKLOG.md and SECURITY.md, then OPEN THE CITED CURRENT SOURCE and decide\n";
p_rem += "whether each item is still_open. Do not trust the markdown checkboxes — they are stale.\n";
p_rem += "For EVERY id below, return one item {id, still_open, file, evidence}.\n";
p_rem += "still_open=true only if you read current code and the defect is still present. Cite a line.\n";
p_rem += "still_open=false if the fix is in tree (describe the fix in evidence).\n";
p_rem += "IDs to rematch: G1 G2 G3 G4 S1 S2 B1 B2 B3 B4 B5 B6 C1 C2 C3 C4 C5 C6\n";
p_rem += "and track-it: estimate_video_res, b64_discard, extend_clamp, matrix_unenforced,\n";
p_rem += "jobs_limit_unbounded, auth_headers_panic, library_content_ram, import_decode_first,\n";
p_rem += "mcp_serial, mcp_config_ignored, mcp_no_content_url, mcp_arg_coerce, mcp_reinit,\n";
p_rem += "mcp_utf8_kill, mcp_proxy_no_poll, mcp_oversize_nullid, library_stray_file, wait_scope.\n";
p_rem += "If an id is too vague after 2 file reads, still_open=false and evidence='could not confirm'.\n";
scan_jobs.push(#{
    prompt: p_rem,
    label: "scan:backlog-rematch",
    capability_mode: "read-only",
    output_schema: REMATCH_SCHEMA,
});

let scan_results = parallel(scan_jobs);

let findings = [];
let rematch_open = 0;
let rematch_closed = 0;
let rematch_lines = [];

let si = 0;
for r in scan_results {
    if r != () && r.success && r.output != () {
        if r.output.findings != () {
            for f in r.output.findings {
                if f != () && f.file != () && f.issue != () && in_scope(f.file) {
                    findings.push(f);
                } else if f != () && f.file != () && !in_scope(f.file) {
                    log("dropped out-of-scope finding file=" + f.file);
                }
            }
        }
        if r.output.items != () {
            for it in r.output.items {
                if it == () { continue; }
                if it.still_open == true {
                    rematch_open += 1;
                    let ev = "";
                    if it.evidence != () { ev = it.evidence; }
                    let fp = "BACKLOG.md";
                    if it.file != () && it.file != "" { fp = it.file; }
                    rematch_lines.push("- OPEN " + it.id + " @ " + fp + " — " + ev);
                    if in_scope(fp) {
                        findings.push(#{
                            file: fp,
                            issue: "Still-open July-2026 item " + it.id + ": " + ev,
                            severity: "high",
                            id: it.id,
                        });
                    }
                } else {
                    rematch_closed += 1;
                    let ev2 = "";
                    if it.evidence != () { ev2 = it.evidence; }
                    rematch_lines.push("- closed " + it.id + " — " + ev2);
                }
            }
        }
    } else {
        log("scan slot " + si.to_string() + " failed or empty");
    }
    si += 1;
}

log("scan produced " + findings.len().to_string() + " in-scope findings; rematch open=" + rematch_open.to_string() + " closed=" + rematch_closed.to_string());

const MAX_VERIFY = 32;
if findings.len() > MAX_VERIFY {
    log("truncating findings from " + findings.len().to_string() + " to " + MAX_VERIFY.to_string() + " for verify panel");
    let trimmed = [];
    let k = 0;
    for f in findings {
        if k < MAX_VERIFY { trimmed.push(f); }
        k += 1;
    }
    findings = trimmed;
}

phase("Verify");
let confirmed = [];
if findings.len() > 0 {
    let vjobs = [];
    let vi = 0;
    for f in findings {
        let blob = json_encode(f);
        let vp = "";
        vp += "You are an adversarial verifier for Imaginarium-RS at /home/andre/Projects/Imaginarium-RS.\n";
        vp += "USE TOOLS. Independently open the cited file with read_file (and grep neighbors if needed).\n";
        vp += "The claim to refute or confirm is this JSON:\n";
        vp += blob;
        vp += "\nSet real=true ONLY if current code still exhibits the defect and you quote evidence.\n";
        vp += "If the July-2026 backlog already fixed it, or the claim is speculative, real=false.\n";
        vp += "Default real=false when unsure. evidence must cite file:line or a quoted snippet.\n";
        vjobs.push(#{
            prompt: vp,
            label: "verify:" + vi.to_string(),
            capability_mode: "read-only",
            output_schema: VERDICT_SCHEMA,
        });
        vi += 1;
    }
    let verdicts = parallel(vjobs);
    let i = 0;
    for v in verdicts {
        if v != () && v.success && v.output != () && v.output.real == true
            && v.output.evidence != () && v.output.evidence != "" {
            let row = findings[i];
            if v.output.reason != () {
                row.reason = v.output.reason;
            }
            row.evidence = v.output.evidence;
            confirmed.push(row);
        }
        i += 1;
    }
}
log(confirmed.len().to_string() + "/" + findings.len().to_string() + " findings survived verification");

phase("Report");
let synth_prompt = "";
synth_prompt += "Write a concise audit report for Imaginarium-RS (Aug 2026, post Image-2.0 roster).\n";
synth_prompt += "Confirmed findings JSON:\n";
synth_prompt += json_encode(confirmed);
synth_prompt += "\nRematch notes JSON:\n";
synth_prompt += json_encode(rematch_lines);
synth_prompt += "\nCounts: confirmed=" + confirmed.len().to_string();
synth_prompt += " scanned=" + findings.len().to_string();
synth_prompt += " rematch_open=" + rematch_open.to_string();
synth_prompt += " rematch_closed=" + rematch_closed.to_string();
synth_prompt += "\nProduce markdown in body. Structure:\n";
synth_prompt += "1. Headline (trust model + whether July backlog is mostly stale)\n";
synth_prompt += "2. Confirmed open issues grouped by severity, each with file and one-line fix\n";
synth_prompt += "3. July rematch: what is still open vs fixed\n";
synth_prompt += "4. Suggested next PR slices (max 4), highest leverage first\n";
synth_prompt += "Do not invent issues that are not in the confirmed JSON. If confirmed is empty, say so\n";
synth_prompt += "and summarize rematch_closed as the health signal.\n";

let synth = agent(synth_prompt, #{
    label: "synth:report",
    capability_mode: "read-only",
    output_schema: SYNTH_SCHEMA,
});

let report = "# Imaginarium-RS audit\n\n";
report += "Confirmed " + confirmed.len().to_string() + " / " + findings.len().to_string() + " after adversarial verify.\n";
report += "July rematch: open=" + rematch_open.to_string() + " closed=" + rematch_closed.to_string() + ".\n\n";
if synth != () && synth.success && synth.output != () && synth.output.body != () {
    if synth.output.headline != () {
        report += "## Headline\n\n" + synth.output.headline + "\n\n";
    }
    report += synth.output.body;
} else {
    report += "(synthesizer failed; dumping confirmed JSON)\n\n";
    report += json_encode(confirmed);
    report += "\n\n## Rematch\n\n";
    for line in rematch_lines {
        report += line + "\n";
    }
}

let path = write_scratch_file("report.md", report);
complete(#{
    path: path,
    summary: confirmed.len().to_string() + " confirmed, rematch open " + rematch_open.to_string(),
    confirmed_count: confirmed.len(),
    scanned_count: findings.len(),
    rematch_open: rematch_open,
    rematch_closed: rematch_closed,
});
