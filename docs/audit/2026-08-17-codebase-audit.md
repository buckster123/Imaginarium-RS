# Imaginarium-RS — codebase audit (2026-08-17)

**Repo:** `~/Projects/Imaginarium-RS` @ `ff604a7` (`master`, clean)  
**Method:** five read-only specialists (core / MCP+CLI / server-security / UI+Slint / CI-docs-ops) plus orchestrator rematch of every P1 against current source.  
**Live node:** `127.0.0.1:8791` answering `{"ok":true,"product":"Imaginarium-RS","version":"0.1.0"}`. Installed binary `/usr/local/bin/imaginarium` 0.1.0 (mtime 2026-08-17).  
**Cerebro MCP:** down this session. Prefrontal + Imaginarium MCP used.  
**Previous rematch:** 2026-08-12 (`BACKLOG.md`). Do not treat July `SECURITY.md` body as the ledger.

---

## Headline

v0.1 local-first is **real and dogfoodable on loopback**. Phases 0–5 shipped, Studio+ craft 5.2–5.4 is in the tree, Image 2.0 + Video 1.5 is the live roster, G1–G4 and S1–S2 stay closed, and PRs #1–#19 are the honesty trail.

The trust model is still **single-tenant LAN appliance**, not a multi-user service. That is fine for `imaginarium serve` on `127.0.0.1`. It is not yet the contract ApexOS / a Write token on a shared LAN should assume: job honesty has a video-shaped hole, spend caps do not reserve in-flight USD, MCP/HTTP 400s drop `error_type`, the studio still re-uploads chain media as data-URLs, and CI does not compile Slint or rebuild `ui-web/dist`.

**No P0 for loopback single-user.** Next work is a short honesty + studio-adoption wave, not a rewrite and not garden adapters.

---

## As-built (what the tree actually is)

One Rust workspace, 28 `.rs` files, ~11k Rust LOC + Vue studio. Fat binary `imaginarium` = CLI + MCP + `serve`. Native `imaginarium-app` is a separate GPL crate.

| Surface | As-built |
|---|---|
| **core** | xAI client, capability matrix, sqlite jobs, dated library, spend/rate gates, ffmpeg craft v1 |
| **CLI** | local image/video/jobs/tokens/config/serve/mcp. Library = `path` only. No `--remote`. |
| **MCP** | 11 tools, NDJSON 2024-11-05, `tools/call` spawned so ping stays live, `--proxy` thin HTTP |
| **HTTP** | OpenAPI paths match `routes.rs`. No SSE, no library CRUD, no `GET /v1/videos/{id}` |
| **Vue** | Image/Video/Craft/Jobs/Library/Settings. Token in `sessionStorage`. Dist committed + rust-embedded |
| **Slint** | 6.0 + 6.1 only (connect, image gen, jobs, preview). No edit, no video forms, no library browse |

Capability matrix (live `imaginarium_models`, matches `models.rs`):

| Model | Modes | Max res | ~USD |
|---|---|---|---|
| `grok-imagine-image` | T2I, edit | 2k | $0.02 / image |
| `grok-imagine-image-quality` | T2I, edit | 2k | $0.05 / image |
| `grok-imagine-image-2.0` | T2I, edit; `quality` low\|medium | 2k | $0.04 / image |
| `grok-imagine-video` | T2V/I2V/R2V + edit/extend | 720p | $0.05–0.07 / s |
| `grok-imagine-video-1.5` | T2V/I2V/R2V + voices (max 3) | 1080p T2V/I2V; R2V 720p | **$0.08 / $0.14 / $0.25** per s |

Official xAI model card (fetched 2026-08-17) still lists those 1.5 tiers. Do **not** flatten estimates to $0.08/s.

Colony: Imaginarium is live. ApexOS consumes it over HTTP (PR #290 path); do not link this GPL UI crate into ApexOS. Garden fal.ai / ComfyUI adapters are **queued, not started** (`notes/queued-open-model-upstreams.md`).

---

## Phase status (honest)

| Phase | Verdict |
|---|---|
| 0 Scaffold | Shipped. `fixtures/` empty; no wiremock. |
| 1 Core + CLI images | Shipped. |
| 2 Video full surface | Shipped (1.5 default generate). |
| 3 Server + LAN auth | Shipped minus remote CLI, SSE, library CRUD. |
| 4 MCP | Shipped. Three transport leftovers still open. |
| 5 Browser studio | Human MVP shipped. U1 adoption still open. |
| Studio+ 5.1 | ~70% in Vue; the **doc** is stale, not the craft engine. |
| Studio+ 5.2–5.4 | Shipped. Engine v1 extras (music/style/kinds) are not in the Vue bench. |
| 6 Slint | **6.0–6.1 only.** 6.2/6.3/6.4 not built. Not in CI. |
| 7 Power | Parked. `key_source` / `X-XAI-Key` documented, never read. |
| Garden G-B | Not started. Waiting on ApexRouter `GARDEN.md` §7. |

---

## July / Aug-12 rematch

Gating and the Aug 12 slices stay **closed** unless a line below says otherwise.

| ID | Still closed? | Evidence |
|---|---|---|
| G1 remote Path gate | **Yes** | `MediaRef::from_remote_input` + `media_from_node_input` on every HTTP/MCP media field |
| G2 library id traversal | **Yes** | `is_safe_asset_id` on content/thumb/craft/`library:`. Residual: no `canonicalize` (P3 symlink) |
| G3 ConnectInfo bypass | **Yes** for `serve()`. Embedder still does not inherit the bind gate / 64 MB / redacting trace |
| G4 drawtext | **Yes** | `'\''` + `expansion=none` + caption cap |
| S1 CORS Any | **Yes** | no `CorsLayer` |
| S2 `?token=` in traces | **Yes** | span is method + path. SPA still puts `?token=` on `<img>`/`<video>` |
| B1 timeout stays Running | **Yes** | `client.rs` poll timeout envelope |
| B4 image fail_job | **Yes for images.** **Video media-read after upsert still strands** (CORE-02) |
| B5 download miss | **Yes** | `Done` + `ok=false` + `error_type=download` |
| B6 terminal upsert lock | **Yes** | `jobs.rs` `WHERE NOT (terminal ← non-terminal)` |
| C5 HTTP `model:auto` | **Partial** | generate + image yes; **video edit/extend HTTP still `ModelId::parse`** (MCP-06) |
| C6a/b spend + paid_rpm | **Shipped** | daily sum does not reserve in-flight USD (CORE-03) |
| estimate 1.5 by res | **Yes** | $0.08 / $0.14 / $0.25 — matches current xAI card |
| MCP typed n/duration/no_wait | **Yes** | string `"4"` still rejected |
| MCP `--config` / `--data-home` | **Yes** on `imaginarium mcp`. **`imaginarium-mcp` binary ignores them** |
| GET `/v1/jobs/{id}` polls | **Yes** | HTTP + proxy `no_wait` works |
| **MCP re-init** | **Still open** | `lib.rs` handshake is one-shot |
| **MCP UTF-8 kill** | **Still open** | `read_line` into `String` |
| **MCP oversized null-id** | **Still open** | spec-legal; hosts hang |
| **Library stray walk** | **Still open, worse** | non-dir under `YYYY/` can abort the whole walk |
| Studio U1 adoption | **Still open** | `?i=`, `library:`, JobBoard prompt |

---

## Findings (confirmed)

Severity: **P0** would block loopback dogfood. There are none.  
**P1** = money, stranded jobs, agent/studio lying, or a host that cannot handshake.  
Every item was opened in current source after the swarm.

### P1 — fix next

#### CORE-01 — Library walk dies on a stray file, and fallback can pick the wrong asset
`crates/imaginarium-core/src/library.rs:204-249`

`read_dir(file).ok()?` on a non-directory `YYYY/notes.txt` (or `YYYY/MM/readme`) returns `None` for the **entire** lookup. Inside a job dir, exact `NN.ext` is tried in `MEDIA_EXTS` order (**video before image**), then fallback is “nth media file by name.” A leftover `00.mp4` hides `00.png`; `library:{id}`, `/v1/library/{id}/content`, and craft all share this walk.

**Fix:** `continue` unless the entry is a directory. Resolve only `NN.ext` (+ legacy `0.png`/`0.mp4`). Prefer `JobStore.local_path` when present.

**Test:** tree with `2026/notes.txt` + `2026/08/17/01JOB/00.png` must resolve. `00.png` + `00.mp4` must have a documented winner (image job → png). Extra `aaa.png` must not become `#1`.

#### CORE-02 — Video media-read after upsert leaves `Running`
`crates/imaginarium-core/src/client.rs:522-567` (generate), `:641` (edit), `:708` (extend) vs image-edit `:376-390`

Image edit uses `fail_job_err` when `media_ref_to_image_field` fails. Video upserts `Running`, then `?`s the same read. A missing `library:` file or unreadable path returns `Err` and leaves a `Running` row with **no** `upstream_request_id`. Agents poll forever.

**Fix:** after the first upsert, every error path is `fail_job_err` / `mark_failed`. No bare `?`.

**Test:** temp store + missing `MediaRef`; generate/edit/extend must `Err` **and** `store.get` is `Failed`.

#### CORE-03 — Daily USD cap does not reserve in-flight spend
`client.rs:164-179`, `jobs.rs:153-171`, first upsert `usage: None`

`estimated_spend_since` sums `usage.estimated_usd` on pending/running/done. Usage is attached later (wait=false submit, or complete). Two concurrent 1080p jobs both see $0 today. Sequential job B is admitted while A is `Running` with no usage.

**Fix:** one sqlite/mutex txn: SUM + insert running row **with** reserved `estimated_usd`. Keep the reservation until terminal.

**Test:** `max_usd_per_day = 1.00`; insert Running $0.80 **with** usage; second $0.40 must fail. Two threads vs $1.00 + two $0.80 estimates: one fails.

#### MCP-01 — `initialize` only on the first frame
`crates/imaginarium-mcp/src/lib.rs:40-63, 95-122`

Handshake is special-cased **before** the loop. A second `initialize` is `method not found`. A blank first line or oversized first frame **exits** the process.

**Fix:** handle `initialize` in the main match (idempotent). Do not exit on a recoverable first-frame parse error.

**Test:** two `initialize` frames → both `result.protocolVersion`. `\n` then `initialize` still handshakes.

#### MCP-02 — One invalid UTF-8 byte kills the session
`crates/imaginarium-mcp/src/transport.rs:57-63`, `lib.rs:67-69`

`read_line` into `String` errors on `0xFF`. First frame: `run()` returns. Later: `transport IO` + `break`.

**Fix:** read `Vec<u8>` until `\n` (still 16 MiB cap). On UTF-8/JSON failure, `-32700` and continue.

**Test:** after init, `0xFF\n` then `ping id=9` → parse error then ping result.

#### MCP-04 / SEC-02 — Paid failures drop `error_type` on the wire
`mcp/dispatch.rs:56-68`, `server/routes.rs:58-77` vs `docs/AGENTS.md:93`, `docs/APEXOS_IMAGINARIUM.md:123`

`check_spend` / `check_rate` return `Err` before a job row exists. MCP wraps every `Err` as JSON-RPC `-32000` with **no** `error_type` / `isError`. HTTP `err_response` is `{ok:false, error}` only. 429 is the one path that already emits `error_type=rate_limit`.

Timeouts and download-misses are honest (`Ok(job)` with `ok=false`). Spend/invalid_mode are not.

**Fix:** map `Error` → tool **result** `{content, isError:true}` with `error_type`. HTTP 400 includes `error_type`. JSON-RPC errors only for unknown method / bad JSON.

**Test:** `max_usd_per_job=0.001` + image generate → agent sees `error_type=spend_limit`.

#### MCP-05 — Proxy silently drops `voice_id`
Local `backend.rs:198-208` folds `voice` / `voice_id` into `reference_audios`. Proxy `args.clone()`s to `POST /v1/videos/generations`. `VideoGenBody` (`routes.rs:238-251`) has `reference_audios` only; serde ignores `voice_id`. Agent gets T2V, no error.

**Fix:** rewrite `voice_id`/`voice` in `ProxyBackend::video_generate`, **or** accept the alias on `VideoGenBody`.

**Test:** proxy mock `{prompt, voice_id:"eve"}` body contains `reference_audios:["eve"]`.

#### MCP-06 — HTTP video edit/extend reject `model:"auto"`
`routes.rs:322-328, 369-375` still `ModelId::parse`. Generate + local MCP use `parse_model_selector`. SPA / ApexOS brief say `"auto"` is legal.

**Fix:** `parse_model_selector` in both handlers (omit → legacy `video`).

**Test:** `POST /v1/videos/edits` `{"model":"auto",...}` is not 400 for the model field.

#### MCP-07 — `job_wait` on craft is not the same locally vs proxy
Local `video_wait` errors `job has no upstream_request_id`. HTTP `/wait` returns the current craft row immediately (often still pending). Documented poll path (`job_status`) works on both.

**Fix:** craft / no-upstream: either poll the DB until terminal / `poll.timeout_s`, or return the row with `error_type=not_pollable`. Same on both sides.

#### CLI-01 — `--model auto` is rejected
`cli/src/main.rs:365-370`. README says `--model auto` (or omit). Omit works. `--model auto` → `unknown model: auto`. Studio default is `2.0`; CLI image default is still `image`.

**Fix:** `parse_model_selector`. Treat `None` as omitted (`explicit=false`).

#### UI-01 — `assetSrc` ignores `?i=` (n>1 stills all show asset 0)
`ui-web/src/components/ImageStudio.vue:267-271`, `ChainBar.vue:79-84`. Node already emits `content_url` with `?i=N` (`library.rs:49-55`).

**Fix:** `a.content_url || libraryContentUrl(job) + (i ? '?i='+i : '')` then token.

#### UI-02 — Chain re-downloads as data-URLs instead of `library:{id}`
`ImageStudio.vue:229-240`, `VideoStudio.vue:261-276`. Extending a 1080p clip freezes the tab and re-posts megabytes the node already has.

**Fix:** set `image` / `video` / craft clip to `library:${job_id}`. File-picker stays data-URL.

#### UI-04 — `status=done` toasted as success when `ok=false`
Download miss is `done` + `ok=false` + `error_type=download`. `ImageStudio.vue:344` / `VideoStudio.vue:414` toast `Image ${status}`. `api.js` never reads `error_type`.

**Fix:** if `!result.ok`, `toastErr` + show `error` / `error_type`. Hide media until `assets.length`.

#### SLINT-01 — Connect does not validate the token
`slint/src/main.rs` + `api.rs:52-59`: success = unauthenticated `/health`. Bad token still shows `ok`. Vue `TokenGate` already hits `/v1/models`.

**Fix:** Connect = `GET /v1/models`.

### P2 — real, schedule after the P1 wave

| ID | Title | Where |
|---|---|---|
| CORE-05 | `download_url` no size cap; `ffprobe` no kill | `library.rs:356-365`, `craft_video.rs` probe |
| CORE-06 | 48 segs × 5 min ffmpeg; global craft mutex held across encode | `craft_video.rs` |
| CORE-07 | Video `aspect_ratio` advertised, never validated | `models.rs` `validate_video_generate` |
| MCP-03 | Oversized frame → `id: null` (spec-legal) | `dispatch.rs:41-47` |
| MCP-08 | Proxy `reqwest::Client` has no timeout | `backend.rs:388-394` |
| MCP-09 | No MCP library import (music beds HTTP-only) | tools.rs |
| MCP-10 | Local `job_status` requires `XAI_API_KEY` even for a done/craft row | `backend.rs:65-74` |
| MCP-11 | Wrong JSON type on `quality`/`model`/`kind` silently omitted | `.as_str()` |
| CLI-02 | No `--yes` / confirm on spend | CLI gen paths |
| CLI-03 | No remote CLI (`IMAGINARIUM_URL` is MCP/Slint only) | vs `ARCHITECTURE.md:364-370` |
| CLI-04 | `library` is `path` only | `main.rs:309-313` |
| CLI-05 | `imaginarium-mcp` binary ignores `--config` / `--data-home` | `mcp/src/main.rs` |
| SEC-01 | `local_path` leaked to every Read client | OpenAPI says omit |
| SEC-03 | Read token can hammer GET poll (upstream quota) | `jobs_get` |
| SEC-06 | `api_router` embedder misses 64 MB / redacting trace / bind gate | `lib.rs` vs `routes.rs` |
| SEC-07 | `--allow-localhost-no-auth` + loopback reverse proxy = Admin for the world | `auth.rs` |
| SEC-08 | Unbounded `?no_wait` craft spawns | `routes.rs:660-706` |
| UI-03 | JobBoard ignores projected `prompt` + `error` | `JobBoard.vue` |
| UI-05 | 401 / 429 / offline not product copy; mid-session 401 does not re-lock | `api.js` |
| UI-06 | `?token=` on every media URL (history / Referer) | SPA |
| UI-07 | Video form: T2V+voice silently becomes R2V; 1080p still offered on legacy `video` | `VideoStudio.vue` |
| UI-09 | Craft UI is 5.3; engine is v1 (no `no_wait`, no music bed) | `VideoCraft.vue` |
| UI-10 | Library view is a job list; craft mp4s preview as `<img>` | `LibraryView.vue` |
| OPS-01 | Slint never clippy/test/built in CI | `ci.yml:22-27` |
| OPS-02 | `ui-web` never rebuilt; rust-embed trusts committed `dist/` | no npm job |
| TEST-01 | `client.rs` ~1425 LOC, 4 tests, no httpmock | money path |
| TEST-02 | HTTP API: one stream test, no auth/429/import/poll oneshot | ApexOS contract |
| LIC-01 | `imaginarium-slint` is GPL-3.0-only; repo has no `LICENSE-GPL` | crate vs tree |

### P3 — do not let these crowd the queue

Auth 401 is plaintext (OpenAPI says JSON). No Range on library video. No CSP/`nosniff` on the SPA. Image AR list missing phone ratios. Footer “Studio+ craft coming”. CLI help still says “Phase 4 stub”. `n-images` dead on Slint. `jobs get --json` unused. Video res `1080P` (caps) rejected. Failed jobs excluded from daily sum (matches ARCHITECTURE; not an invoice).

---

## Rejected / do-not-do

- **Do not flatten Video 1.5 to $0.08/s.** Official card (2026-08-17) is still $0.08 / $0.14 / $0.25. The public “$0.08 per second” headline is the 480p floor.
- **Do not re-open G1–G4 / S1–S2.** Rematch them when craft filters or media gates change.
- **Do not treat user `http(s)` media as node SSRF.** The node forwards the URL to xAI (`client.rs:207-209`). Fetching it locally would *create* SSRF.
- **Do not implement ARCHITECTURE §8.1 ghosts** (SSE, library CRUD, `GET /v1/videos/{id}`). OpenAPI is the contract.
- **Do not ship `X-XAI-Key` pass-through.** Documented, unbuilt (`key_source` is never read). Shipping it would let any Write client override the upstream key.
- **Do not start fal.ai / ComfyUI adapters** until ApexRouter `GARDEN.md` §7 says go. Imaginarium owns protocol; ApexRouter owns placement/money.
- **Do not link `imaginarium-slint` into ApexOS** (GPL). HTTP is the seam.
- **Do not add `CorsLayer::allow_origin(Any)`** “for the SPA.” Same-origin.
- **Do not persist the LAN token in `localStorage`.**
- **Do not switch MCP to Content-Length-only.** Cerebro/agentd/AGENTS.md are NDJSON. If you add headers, accept both.
- **Do not grow `craft_video.rs` (~2.5k) with garden/open-model code.**
- **Do not treat unchecked STUDIO_PLUS 5.1 boxes as a rebuild list.** Half are already in Vue.

---

## Coverage (what's actually tested)

75 `#[test]` / `#[tokio::test]`. Density is high on models, estimate-by-res, tokens, rate bucket, library id gates, job terminal-lock, craft filtergraph. Near-zero on the money/auth/HTTP edges ApexOS will break against.

| Crate | Tests | Gap that matters |
|---|---|---|
| core | 68 | `ImagineClient` lifecycle unmocked; spend+store together; stray walk |
| mcp | 5 | no stdio integration; no proxy mapping |
| server | 2 | no auth/429/import/poll oneshot |
| cli | 0 | clap paths, `--model auto`, serve refuse |
| slint | 0 | not in CI |
| ui-web | 0 | `assetSrc` / chain / craft |

CI (`.github/workflows/ci.yml`): `fmt --all`, clippy+test on core/cli/mcp/server `-D warnings`, `cargo build -p imaginarium-cli` (debug). **No slint, no npm, no deny, no `--release`.**  
`default-members` = core+cli, so a laptop `cargo test` skips the LAN stack. PR template omits the server test package CI actually runs.

---

## Doc drift (strike or schedule)

| Doc | Lie |
|---|---|
| `ARCHITECTURE.md:364-370` | Remote CLI / `IMAGINARIUM_URL` + `video gen` — MCP/Slint only |
| `ARCHITECTURE.md:114-118` | `X-XAI-Key` + `key_source` — field stored, never read |
| `ARCHITECTURE.md:318,533` | wiremock `fixtures/` — directory empty |
| `ARCHITECTURE.md:356-359` | `library ls` / `open` — CLI is `path` only |
| `ARCHITECTURE.md:387-406` | §8.1 SSE / library CRUD / `GET /v1/videos/{id}` — OpenAPI correctly omits |
| `cli/src/main.rs:76` | “Phase 4 stub” — MCP is real |
| `STUDIO_PLUS.md:59-70` | 5.1 all `[ ]` — toast/chain/Ctrl+Enter/prefs/busy exist |
| `App.vue:62` | “Studio+ craft coming” — Craft tab shipped |
| `STUDIO_PLUS.md:155` | “document ffmpeg in README” — README never mentions it |
| `LICENSING.md` | GPL-3.0-only slint — no `LICENSE-GPL` in the tree |
| `.grok/workflows/audit-imaginarium.rhai` | still claims “Video 1.5 is I2V-only” — **false** |

Docs that match code and should stay the contract: `openapi/imaginarium-v1.yaml`, `docs/APEXOS_IMAGINARIUM.md` “Not in v1”, `docs/MULTI_NODE.md`, `docs/SLINT.md` slice table (if you add “6.1 done”).

---

## Live / ops snapshot

- Branch `master` = `origin/master`, 45 commits, 19 merged PRs, **zero open GitHub issues**.
- Health flags: none (Prefrontal).
- `/health` → `0.1.0`. Binary on PATH is the same version (rebuilt 2026-08-17).
- ffmpeg is a hard dep for craft + thumbs; missing → craft error. Not in README.

---

## Suggested PR slices (max four after this docs PR)

See [`2026-08-17-path-forward.md`](2026-08-17-path-forward.md). Short version:

1. **Honesty leftovers** — CORE-02, CORE-01, CORE-03, MCP-04/SEC-02 `error_type`.
2. **Studio U1** — UI-01 `?i=`, UI-02 `library:`, UI-03 prompt, UI-04 `ok=false`.
3. **MCP/CLI parity** — MCP-01/02, MCP-05/06, CLI-01.
4. **CI honesty** — slint build, dist dirty-gate, httpmock + axum oneshot.

Then pick one product fork. Do not start three.
