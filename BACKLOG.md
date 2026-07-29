# Imaginarium-RS — Backlog

Source: 2026-07-28 pre-integration audit (see `SECURITY.md` for the security findings in full).
The swarm returned 50 findings; after dedup they collapse to ~15 distinct issues. Grouped below
into three suggested PRs plus a track-it tail. Severities are post-triage.

Provenance: **[confirmed]** traced to code by reviewer · **[swarm]** reported by audit swarm, plausible.

---

## PR A — Harden the network trust boundary  *(security; do before any LAN/untrusted-token exposure)*

Full detail + fixes in `SECURITY.md`. Checklist:

- [ ] **G1** `MediaRef::Path` arbitrary local-file read+exfil — reject bare local paths on the
      server + MCP boundary (`data:`/`http(s):`/`file_…` only); add `MediaRef::from_remote_input`.
      `types.rs:209`, `client.rs:155`, `routes.rs:144`, `mcp/backend.rs:138`. **[confirmed]**
- [ ] **G2** Path traversal in `/v1/library/{id}/content` — validate `id` is a job token,
      canonicalize + prefix-check; ideally resolve via `JobStore` (also kills the O(library)
      tree-walk). `routes.rs:354`, `lib.rs:116`, `craft_video.rs:76`. **[confirmed]**
- [ ] **G3** Localhost Admin bypass keys on `cfg.server.bind` string, not the peer; `api_router`
      ignores its `allow_localhost_no_auth` param — use `ConnectInfo` `is_loopback()`, honor the
      param, move the `has_any_auth()` refusal gate into `api_router`. `auth.rs:52-64`,
      `routes.rs:27`. **[confirmed]** — blocks the ApexOS embed path.
- [ ] **G4** ffmpeg `drawtext` escaping is wrong (`'`→`\'` fails in ffmpeg quotes) and injectable —
      use `'\''` or `textfile=`; bound overlay count. `craft_video.rs:349`. escaping **[confirmed]**, breakout **[swarm]**
- [ ] **S1** CORS `allow_origin(Any)` → restrict to node origin for `/v1/*`. `lib.rs:76`. **[confirmed, dev-gated]**
- [ ] **S2** `?token=` logged by `TraceLayer` default span — custom span (`method`+`path` only),
      prefer header token. `lib.rs:86`. **[swarm]**

---

## PR B — Job-lifecycle honesty  *(correctness; failures should never look like success)*

- [ ] **B1** Poll timeout marks a video job terminally `Failed`, and `video_wait` early-returns on
      `Failed` → a **paid** upstream job that merely ran past the 600s window is unrecoverable.
      Keep it `Pending` (or a non-terminal `TimedOut`), or let `video_wait`/`video_status_once`
      re-poll jobs whose `error_type=="timeout"`. `client.rs:883`. **[confirmed]** — money bug.
- [ ] **B2** `imaginarium mcp` writes tracing to **stdout**, corrupting the JSON-RPC stream (the
      MCP lib emits `info!` at default level, so it breaks even without `RUST_LOG`). Add
      `.with_writer(std::io::stderr)` in `init_tracing`. `cli/main.rs:318` (called at :385 before
      the subcommand match). **[confirmed]** — trivial fix, primary agent surface.
- [ ] **B3** 2xx response with an unexpected body shape becomes a silent empty success
      (`ok=true`, no assets). Treat `done` with no extractable `url`/`file_output` (video) or empty
      `data[]` (images) as `failed`/`error_type="parse"` + body snippet. `client.rs:910`. **[swarm]**
- [ ] **B4** Image job left `status=running` if a post-response local step fails (dir/write/parse).
      Ensure the job is finalized (done or failed) on every path. `client.rs:261`. **[swarm]**
- [ ] **B5** Asset download failures are swallowed (`let _ = ...`) — job reported `ok/done` with no
      stored file. Surface a soft-failure marker so the caller knows the bytes aren't local.
      `client.rs:940`. **[swarm]**
- [ ] **B6** Concurrent `upsert_result` on the same `job_id` (each route opens its own SQLite
      connection; `ON CONFLICT DO UPDATE` overwrites all cols) can reset a completed row to
      `running`/empty. Narrow window, self-heals on next poll. `client.rs:773`, `jobs.rs`. **[swarm]** — low.

---

## PR C — Make the brief and the code agree  *(do before building the ApexOS "Imagine" tab against it)*

`docs/APEXOS_IMAGINARIUM.md` and `openapi/imaginarium-v1.yaml` are named the integration
"source of truth" but describe routes/behavior the code doesn't ship. Either implement or trim.

- [ ] **C1** Routes promised-but-absent: SSE job events (`/v1/jobs/{id}/events`), library
      list/get/delete/upload, separate video poll/wait routes. `routes.rs:27` vs brief §API,
      `openapi` omits 9 of 15 routes + both non-Bearer transports. **[swarm]**
- [ ] **C2** Import cap is **40 MB** decoded, not the **64 MB** the brief promises (body limit is
      64 MB; the handler rejects >40). Align the numbers. `routes.rs:401`. **[confirmed]**
- [ ] **C3** Brief claims token redaction in logs; `?token=` is logged (see S2). Reconcile.
- [ ] **C4** `POST /v1/jobs/{id}/wait` returns **502** for image jobs and unknown ids (the flow the
      brief documents). Return a clean 200/404/400. `routes.rs:339-352`. **[swarm]**
- [ ] **C5** `model:"auto"` is rejected with 400 although the brief specifies model "(or auto)".
      Accept `auto` (drop to server-side default) or update the brief. `models.rs:36`. **[swarm]**
- [ ] **C6** No rate-limit / spend-cap despite `docs/ARCHITECTURE.md §11` promising per-token
      throttle + `max_usd` config. Implement or drop the claim (matters for agent-loop cost
      runaway). `lib.rs:81`. **[swarm]**

---

## Track-it — genuine but minor

- [ ] `estimate_video` ignores resolution → 1080p video-1.5 cost under-reported ~3×. `estimate.rs:28`
- [ ] `b64_json` image payloads discarded unless `auto_download` is on. `client.rs:989`
- [ ] `video_extend` silently clamps duration (2–10) instead of validating like `video_generate`. `client.rs:635`
- [ ] Aspect-ratio / image-resolution / image-model caps not enforced against the matrix. `models.rs:179`
- [ ] Unbounded `?limit` on `/v1/jobs` → dumps whole table (memory). Clamp it. `jobs.rs:110`
- [ ] `auth_headers()` panics on an API key with a control char (kills the MCP process). `client.rs:129` **[confirmed]**
- [ ] `library_content` buffers whole asset in RAM (no streaming) → OOM risk on Nano-tier / big video. `routes.rs:358`
- [ ] Import decodes untrusted base64 *before* the 40 MB cap; craft timelines unbounded. `routes.rs:401`
- [ ] MCP: serial loop — one blocking video call stalls all other tool calls. `mcp/lib.rs:96`
- [ ] MCP: global `--config`/`--data-home` ignored by the `mcp` subcommand. `cli/main.rs:821`
- [ ] MCP: results give no retrievable media handle (`content_url` never set; no library/download
      tools) — agents can't fetch what they generate over the proxy. `mcp/tools.rs:135`
- [ ] MCP: string-typed numeric/bool args silently coerced to defaults instead of rejected. `mcp/dispatch.rs:79`
- [ ] MCP: `initialize` only handled on the first frame; a re-init breaks the handshake. `mcp/lib.rs:57`
- [ ] MCP: one invalid UTF-8 byte on stdin kills the server mid-session. `mcp/transport.rs:34`
- [ ] MCP proxy `job_status` never polls upstream → documented `no_wait` flow never completes over the proxy. `mcp/backend.rs:391`
- [ ] Oversized (>16 MB) frame answered with a null-id error the caller can't correlate. `mcp/transport.rs:52`
- [ ] A single stray file in the library tree makes asset lookup return "not found". `lib.rs:123`
- [ ] `drawtext` apostrophe escaping wrong (benign symptom of G4). `craft_video.rs:352`
- [ ] `POST /v1/estimate` requires Write scope for a pure computation; `/v1/jobs/{id}/wait` is
      Read-scoped though it polls upstream + writes disk. Scope-map tidy. `auth.rs:77-80`

---

## Studio arc (2026-07-29 →) — upstream slices for the ApexOS native studio

Charter + full slice ledger live in ApexOS-RS `docs/imagine-studio.md`; the
upstream (this repo) slices, in order:

- [x] **U1 — client ergonomics**: `library:{job_id}[#n]` MediaRef (node-side
      resolution, kills the download→base64 chain round-trip), `?i=` multi-asset
      addressing on `/v1/library/{id}/content` (n>1 batches fully reachable),
      jobs-list projection carrying `prompt` + `assets` count.
- [x] **U2a — craft engine correctness** (cutting-room port, part 1): per-segment
      normalize filter → concat (fixes mixed-source `-c copy` breakage),
      master-clock single audio pass + a **music-bed audio track**
      (`AssetKind::Audio` + library audio sniff/mime/import/content-type),
      segment-owned captions (makes the lost-overlay-after-clip-0 bug
      unrepresentable), ffprobe durations (kills the 6.0s fallback), `-nostdin`
      + even-dimension pitfalls as tests.
- [x] **U2b — craft engine expressiveness** (part 2): versioned merged timeline
      contract (style block, segment kinds clip/still+Ken-Burns/card, speed,
      letterbox recipes), two-pass loudnorm ship pass, content-hash segment
      caching, provenance field.
- [ ] **U3 — thumbnails/posters** (`thumb.jpg` on completion + `/v1/library/{id}/thumb`)
      + **async craft render** (job id immediately, poll like any job).
- [ ] Web studio adoption pass: `assetSrc` uses `?i=` (fixes the n>1
      duplicate-image render), chain loads switch to `library:` refs (drops the
      blob→dataURL round-trip), JobBoard shows the projected prompt.

_50-finding raw audit output archived out-of-repo; this backlog is the deduped/triaged view._
