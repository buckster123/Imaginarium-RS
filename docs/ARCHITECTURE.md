# Imaginarium-RS — Architecture Plan (v3, defaults folded)
Date: 2026-07-28
Status: READY FOR READ-OVER — all product defaults locked; scaffold only after Andre signs off
Repo target: `~/Projects/Imaginarium-RS` (GH: `buckster123/Imaginarium-RS`)
CLI bin: `imaginarium` only (no short alias in v1)

---

## 0. Locked decisions

| # | Decision | Lock |
|---|---|---|
| 1 | Name | **Imaginarium-RS** — avoid bare “imagine”/“grok-imagine” branding |
| 2 | Language | **Rust workspace** |
| 3 | OAuth | **Defer** until after MCP MVP (BYOK only in v1) |
| 4 | UI | **Dual surface**: (A) zero-dep **Vue 3** browser SPA embedded by API node, (B) native **Slint** app (winit + linuxkms) |
| 4b | Licenses | **Core/cli/mcp/server = MIT OR Apache-2.0**; **`imaginarium-slint` = GPL-3.0** (clean separation) |
| 5 | Persistence | **Local-first auto-download**; xAI Files / public_url = **opt-in** profile |
| 6 | Server | **LAN + bearer token** multi-node; fat node holds key + library |
| 6b | Default bind | **`127.0.0.1:8791`**; non-loopback (`0.0.0.0` / LAN IP) only via explicit flag, and **requires** token |
| 6c | Job IDs | **Local `job_id` (ULID) always primary**; upstream `request_id` secondary field |
| 7 | Next step | **Andre read-over of this plan**, then Phase 0+1 scaffold on go |

---

## 1. Goal

Fully standalone creative studio toolkit for the xAI Imagine API (image + video), usable by:

- Agents via **MCP** (stdio) and/or **HTTP API**
- Humans via **browser UI** (no install on client) and **native Slint app**
- Multi-node home/lab meshes: one Imaginarium node holds the xAI key + disk cache; others call it with a LAN token

Not a Hermes plugin. Hermes is a client, same as Claude Code / custom systems.

Primary auth to xAI: **BYOK `XAI_API_KEY`** (or key held only on the server node).
Optional later: SuperGrok OAuth (experimental, tier-gated) — out of v1 scope.

---

## 2. Official Imagine surface (source of truth)

Base: `https://api.x.ai/v1`  
Auth: `Authorization: Bearer $XAI_API_KEY`

### 2.1 Endpoints

| Op | Method | Path | Sync? |
|---|---|---|---|
| Image generate | POST | `/v1/images/generations` | sync |
| Image edit (1–3 sources) | POST | `/v1/images/edits` | sync |
| Video generate (T2V/I2V/R2V) | POST | `/v1/videos/generations` | async → `request_id` |
| Video edit | POST | `/v1/videos/edits` | async |
| Video extend | POST | `/v1/videos/extensions` | async |
| Video poll | GET | `/v1/videos/{request_id}` | — |
| Files I/O | Files API + optional `storage_options` on Imagine calls | mixed |
| Batch | Batch API | optional later; media still full price |

### 2.2 Models

| Model | Modes | Notes |
|---|---|---|
| `grok-imagine-image` | T2I, edit | Fast / cheaper |
| `grok-imagine-image-quality` | T2I, edit | 1.x higher-fidelity tier |
| `grok-imagine-image-2.0` | T2I, edit | Aug 2026 model; optional `quality`=`low`\|`medium` |
| `grok-imagine-video` | T2V, I2V, R2V, edit, extend | Legacy; max 720p; no voices |
| `grok-imagine-video-1.5` | T2V, I2V, R2V | Default generate model. 1080p on T2V/I2V; R2V + `reference_audios` cap 720p |

### 2.3 Parameter rules (encode in capability matrix)

**Images**
- `prompt`, `model`, `n`, `aspect_ratio` (incl. `auto`, phone ratios), `resolution` (`1k`|`2k`)
- `quality`: `low` | `medium` — **Image 2.0 only** (omit for upstream default `medium`)
- `response_format`: `url` | `b64_json`
- edits: `image` OR `images[]` (max 3 per API docs; consumer 2.0 UI allows 5); multi-ref tokens `<IMAGE_0>`…
- each media input: `{url}` | `{file_id}` | local path→data-URI (client-side)
- `storage_options` only when cloud profile enabled

**Video**
- modes mutually exclusive: prompt-only | `image` | `reference_images` | edit `video` | extend `video`
- **forbid** `image` + `reference_images` / `reference_audios`
- `reference_audios`: up to 3 preset `voice_id`s on **video-1.5 R2V** (audio-only is valid)
- duration 1–15s gen; extend segment 2–10s (default 6) **adds** to original length
- AR: 1:1, 16:9, 9:16, 4:3, 3:4, 3:2, 2:3
- res: 480p | 720p | 1080p (1080p only video-1.5 T2V/I2V; R2V max 720p)
- edit inherits input duration/AR/res (cap ~720p / ~8.7s)
- poll statuses: `pending` | `done` | `failed` | `expired`
- outputs: ephemeral URL; optional file_output when cloud profile on

### 2.4 Pricing ballpark (warn in UI/CLI)

| Model | ~Cost |
|---|---|
| image | $0.02 / out |
| image-quality | $0.05–0.07 / out |
| image-2.0 | $0.04 / out |
| video | $0.05–0.07 / sec |
| video-1.5 | $0.08–0.25 / sec (res) |
| Files storage | $0.025 / GiB / day (opt-in only) |

Docs: https://docs.x.ai/developers/model-capabilities/imagine

---

## 3. Auth model (two layers)

### 3.1 Upstream (Imaginarium → xAI)

| Mode | v1 | Notes |
|---|---|---|
| API key env / config / keyring | **yes** | `XAI_API_KEY` or config |
| Per-request pass-through header | **yes** | `X-XAI-Key` from trusted LAN clients overrides server default (optional policy) |
| OAuth SuperGrok | **no (v1)** | P2+ experimental |

Server policy knobs:
- `upstream.key_source = server | client | either`
- Default for multi-node mesh: **server holds key** so edge nodes never see it

### 3.2 Downstream (agents/UI/nodes → Imaginarium)

LAN bearer token auth (matches your mesh pairing pattern):

```
Authorization: Bearer <IMAGINARIUM_TOKEN>
# or
X-Imaginarium-Token: <token>
```

- Generate on first `imaginarium serve` / `imaginarium token create`
- Store hash at rest (argon2/blake3); show plaintext once
- Multiple tokens with labels + scopes later (`image:write`, `video:write`, `admin`)
- Localhost may allow tokenless in dev (`--allow-localhost-no-auth`) — **off** when binding non-loopback
- TLS optional later (or rely on mesh/tailscale/wireguard); v1 = HTTP over trusted LAN is OK if documented

### 3.3 TOS posture

- BYOK / server-held API key = clean official path
- Tool is a client of xAI API; user responsible for xAI usage policy
- Do not market as “unofficial SuperGrok unlock”
- OAuth (if ever): local single-user only, feature-flagged, big warnings

---

## 4. Persistence strategy (local-first)

### Default profile: `local`

1. Call xAI with normal response URLs (no `storage_options`)
2. On success, **immediately download** media to local library
3. Return **local paths + original ephemeral URL** (URL may die; path is canonical)
4. Job DB records paths, prompts, params, costs, upstream ids

Library layout:
```
$IMAGINARIUM_HOME/              # default ~/.local/share/imaginarium
  config.toml
  tokens.db                     # or sqlite all-in-one
  imaginarium.db                # jobs, assets metadata
  library/
    2026/07/28/
      <job_id>/
        meta.json
        prompt.txt
        00.png | 00.mp4
        thumb.webp              # optional
  cache/                        # temp downloads / uploads
```

### Optional profile: `xai_files`

- Set `storage_options` (private and/or public_url)
- Useful when:
  - chaining edit/extend across nodes without re-uploading bytes
  - sharing a stable URL outside the LAN
- Still **also** local-download by default (belt + suspenders)
- Explicit CLI/UI toggle; never silent

### Optional profile: `direct_upload`

- Client/edge uploads bytes to the **Imaginarium node** (`POST /v1/library/upload`)
- Node may base64/data-URI or re-host for xAI
- Edge never needs public ingress for media

**Principle:** drop cloud deps where possible; cloud is a selectable power tool.

---

## 5. Multi-node topology

```
┌─────────────────────────────┐
│  Imaginarium NODE (fat)     │
│  - XAI_API_KEY              │
│  - disk library             │
│  - axum :8791               │
│  - MCP stdio (local agents) │
│  - browser UI static        │
└────────────▲────────────────┘
             │ LAN + bearer token
    ┌────────┼────────┬──────────────┐
    │        │        │              │
 Hermes   Edge-Pi   Claude     Slint desktop
 (MCP     (HTTP     (HTTP      (HTTP client
  local    API)      API)       to node)
  or HTTP)
```

Roles:
- **Node**: runs `imaginarium serve`
- **Client CLI**: `imaginarium --remote https://node:8791 --token … video gen …`
- **Native app**: Slint talks HTTP to local or remote node
- **Browser**: opens `http://node:8791/` — zero client install
- **MCP on edge**: either
  - (preferred) run MCP on fat node, agents connect remotely if MCP-HTTP enabled, **or**
  - edge MCP is a thin proxy forwarding to node HTTP API (no xAI key on edge)

MCP-HTTP (streamable HTTP) is valuable for multi-node; stdio remains default for single-machine Hermes.

---

## 6. UI strategy (dual surface)

You asked for: dependency-free browser **and** Slint (winit + kms). That’s the right split.

### 6.1 Browser UI — “zero dep client”

**Goal:** any machine on the LAN with a browser can operate the studio. No npm on the client.

**Locked call:** Browser = **embedded Vue 3 SPA** (Vite build → `rust-embed` into server). Slint stays **native only**. Shared design tokens where practical, but **not** one codebase forced into both.

Rejected alternatives (kept for archaeology):
- HTMX/SSR — viable fallback if SPA build ever becomes painful; not v1
- Slint → WASM — not primary web path (poor general-web fit; weak `<video>`/DOM UX)

Browser must work as pure static files against the API (CORS + token in sessionStorage / header). Features:

1. Image studio (prompt, model, AR, res, n, multi-ref drop)
2. Video studio (mode tabs: T2V / I2V / R2V / Edit / Extend)
3. Job board (live SSE/WS status, cost, download, chain actions)
4. Library browser (local assets on the node)
5. Settings (only what token scope allows; admin token for key/token mgmt)

Auth UX: paste LAN token once; stored in sessionStorage (not localStorage by default).

### 6.2 Native Slint app — “app-windows”

Backends:
- **winit** — desktop (Wayland/X11/Windows/macOS)
- **linuxkms** — direct KMS/DRM embedded / appliance / no display server (your Pi/kiosk path)

App modes:
- **Connected**: HTTP client to `imaginarium serve` (local or remote). Thin UI. Preferred.
- **Embedded runtime** (optional later): link `imaginarium-core` in-process for single-binary offline-ish workstation (still needs network to xAI)

Slint strengths here:
- Real native windowing, keyboard, file dialogs
- KMS for headless-display appliances
- Rust-native event loop alongside async jobs

Slint cautions:
- App crate is **GPL-3.0** (locked); headless stack stays MIT/Apache — see §0 / `docs/LICENSING.md`
- Media playback: prefer shell-open / external player / `ffmpeg` thumbnails rather than in-widget GPU video on day one
- Don’t use (and we won’t use) Slint-WASM as the browser strategy

**License (locked):**
- `imaginarium-core`, `imaginarium-cli`, `imaginarium-mcp`, `imaginarium-server`, and server-embedded `ui-web` assets: **MIT OR Apache-2.0** (dual)
- `imaginarium-slint` (native app only): **GPL-3.0**
- Distributors can ship headless node binaries under permissive terms; shipping `imaginarium-app` means GPL obligations for that binary
- egui/iced is a parachute only if Slint becomes untenable — not planned

### 6.3 UI feature parity matrix

| Feature | Browser | Slint | CLI | MCP |
|---|---|---|---|---|
| Image gen/edit | yes | yes | yes | yes |
| All video modes | yes | yes | yes | yes |
| Job live status | SSE | poll/async | wait/status | status/wait |
| Library browse | yes | yes | ls | list tools |
| Token admin | yes (admin) | yes | yes | limited |
| KMS / no compositor | no | **yes** | n/a | n/a |
| Zero install client | **yes** | no | binary | binary |

---

## 7. Crate architecture

```
Imaginarium-RS/
├── Cargo.toml                          # workspace
├── crates/
│   ├── imaginarium-core/               # MIT/Apache — pure logic
│   │   ├── auth_upstream.rs            # xAI key
│   │   ├── auth_downstream.rs          # LAN tokens (used by server)
│   │   ├── client.rs                   # reqwest xAI client
│   │   ├── models.rs                   # catalog + capability matrix
│   │   ├── image.rs
│   │   ├── video.rs                    # submit/poll/wait
│   │   ├── files_xai.rs                # optional Files API
│   │   ├── library.rs                  # local asset store
│   │   ├── jobs.rs                     # sqlite job records
│   │   ├── estimate.rs                 # cost estimator
│   │   └── config.rs
│   ├── imaginarium-server/             # axum — API + static browser UI + SSE
│   ├── imaginarium-mcp/                # stdio MCP (+ optional HTTP MCP feature)
│   ├── imaginarium-cli/                # clap binary `imaginarium`
│   └── imaginarium-slint/              # native app (winit + kms features)
├── ui-web/                             # Vue 3 + Vite SPA → dist embedded by server
├── openapi/imaginarium-v1.yaml
├── docs/
│   ├── ARCHITECTURE.md
│   ├── MULTI_NODE.md
│   ├── AGENTS.md                       # Hermes/Claude snippets
│   └── LICENSING.md                    # MIT/Apache core vs GPL Slint app
├── fixtures/                           # wiremock JSON from docs
└── README.md
```

### 7.1 Binary entrypoints

| Binary | Role |
|---|---|
| `imaginarium` | CLI + `mcp` + `serve` subcommands (fat binary OK) |
| `imaginarium-app` | Slint native (separate so headless servers don’t pull GUI deps) |

Features on CLI binary:
- default: cli + mcp + serve
- `serve` pulls axum + embedded web
- GUI never required on a headless node

### 7.2 CLI sketch

```bash
imaginarium config init
imaginarium config set upstream.api_key-stdin
imaginarium token create --label hermes-krkn

# default: loopback only (token optional in pure local dev if --allow-localhost-no-auth)
imaginarium serve
# => http://127.0.0.1:8791

# multi-node / LAN: explicit bind + token required
imaginarium serve --bind 0.0.0.0:8791
# refuses to start without at least one token configured

imaginarium image gen -p "..." --model quality --ar 16:9 -n 4
imaginarium image edit --image ./a.png -p "noir"
imaginarium video gen -p "..." --duration 10 --res 720p
imaginarium video i2v --image ./s.png -p "..." --model 1.5 --res 1080p
imaginarium video ref --ref a.png --ref b.png -p "..."
imaginarium video edit --video ./v.mp4 -p "..."
imaginarium video extend --video ./v.mp4 --duration 6 -p "..."
imaginarium video status <job_id>
imaginarium video wait <job_id>
imaginarium library ls
imaginarium library open <job_id>
imaginarium jobs ls
imaginarium mcp
imaginarium mcp --proxy http://fat-node:8791   # edge thin MCP
```

Remote mode:
```bash
export IMAGINARIUM_URL=http://192.168.0.10:8791
export IMAGINARIUM_TOKEN=...
imaginarium video gen -p "..."    # hits remote node, no local xAI key
```

---

## 8. HTTP API (node)

Versioned under `/v1`. **`openapi/imaginarium-v1.yaml` is the shipped-route source of truth** (regenerated 2026-07-28). The sketch in §8.1 is the original design — some entries (SSE `/v1/jobs/{id}/events`, library list/get/delete/upload, `GET /v1/videos/{id}` poll, per-token rate limits) are **not yet implemented**; see the "Not in v1" note in `docs/APEXOS_IMAGINARIUM.md`.

### 8.1 Core

```
GET  /health
GET  /v1/models
POST /v1/estimate

POST /v1/images/generations
POST /v1/images/edits

POST /v1/videos/generations
POST /v1/videos/edits
POST /v1/videos/extensions
GET  /v1/videos/{upstream_or_local_id}
POST /v1/videos/{id}/wait

GET  /v1/jobs
GET  /v1/jobs/{id}
GET  /v1/jobs/{id}/events          # SSE

GET  /v1/library
POST /v1/library/upload
GET  /v1/library/{id}
GET  /v1/library/{id}/content      # bytes
DELETE /v1/library/{id}

POST /v1/tokens                    # admin
GET  /v1/tokens
DELETE /v1/tokens/{id}
```

### 8.2 Response shape (stable for agents)

```json
{
  "ok": true,
  "job_id": "job_...",
  "upstream_request_id": "..." ,
  "status": "done",
  "mode": "image_generate",
  "model": "grok-imagine-image",
  "assets": [
    {
      "id": "asset_...",
      "kind": "image",
      "local_path": "/home/.../library/.../00.png",
      "content_url": "/v1/library/asset_.../content",
      "upstream_url": "https://imgen.x.ai/...",
      "file_id": null,
      "public_url": null
    }
  ],
  "usage": { "estimated_usd": 0.02, "upstream_ticks": null },
  "error": null
}
```

Rules:
- Never inline multi-MB base64 into JSON for agents
- Always provide `content_url` on the node for LAN fetch
- `local_path` only meaningful on the node filesystem (omit or null for remote clients)

---

## 9. MCP tool surface

Granular tools (better for LLMs than one mega-tool):

| Tool | Notes |
|---|---|
| `imaginarium_models` | capability matrix |
| `imaginarium_estimate` | cost before spend |
| `imaginarium_image_generate` | |
| `imaginarium_image_edit` | 1–3 images |
| `imaginarium_video_generate` | mode by fields present |
| `imaginarium_video_edit` | |
| `imaginarium_video_extend` | |
| `imaginarium_job_status` | non-blocking |
| `imaginarium_job_wait` | blocking w/ timeout |
| `imaginarium_library_list` | |
| `imaginarium_library_get` | metadata + content_url |
| `imaginarium_download` | force pull to path (node-local) |

Long video: agents should `generate` → poll `job_status` unless client timeout ≥ 600s.

Hermes snippet:
```yaml
mcp_servers:
  imaginarium:
    command: imaginarium
    args: [mcp]
    env:
      XAI_API_KEY: ${XAI_API_KEY}          # local node
      # or:
      # IMAGINARIUM_URL: http://192.168.0.10:8791
      # IMAGINARIUM_TOKEN: ${IMAGINARIUM_TOKEN}
    timeout: 600
```

Edge thin MCP:
```yaml
args: [mcp, --proxy, http://192.168.0.10:8791]
env:
  IMAGINARIUM_TOKEN: ...
```

---

## 10. Capability matrix (single source of truth)

Implemented once in `imaginarium-core::models`, consumed by CLI/API/MCP/UI.

| Model | T2I | Edit img | T2V | I2V | R2V | Edit vid | Extend | Max res |
|---|---|---|---|---|---|---|---|---|
| image | ✓ | ✓ | | | | | | 2k |
| image-quality | ✓ | ✓ | | | | | | 2k |
| image-2.0 | ✓ | ✓ | | | | | | 2k |
| video | | | ✓ | ✓ | ✓ | ✓ | ✓ | 720p |
| video-1.5 | | | ✓ | ✓ | ✓ | | | **1080p** (R2V 720p) |

Generate modes default to `video-1.5`. Edit/extend stay on `video`.

Reject early with structured error:
`{ ok:false, error_type:"invalid_mode", message:"1080p is not supported on reference-to-video (max 720p)" }`

---

## 11. Security

- Non-loopback bind ⇒ token required (hard error if none configured)
- Default listen: `127.0.0.1:8791`; LAN requires explicit `--bind 0.0.0.0:8791` (or interface IP)
- Admin routes (token mint, upstream-key presence) need admin-scoped token
- Upstream xAI key never returned by API
- Path traversal guard on library
- Rate limit per token (simple token bucket) to stop runaway agent loops
- Optional max_usd_per_job / daily cap in config
- Redact tokens in logs
- CORS allowlist for browser UI origins (default same-host)

---

## 12. Phased delivery

### Phase 0 — Scaffold
- workspace, licenses, CI (fmt/clippy/test)
- config + dirs
- static model catalog command
- OpenAPI stub

### Phase 1 — Core + CLI images (local-first)
- upstream client
- image gen/edit
- library download
- sqlite jobs
- wiremock fixtures

### Phase 2 — Video full surface
- T2V/I2V/R2V/edit/extend
- poll/wait
- auto model route
- cost estimator + `--yes` / confirm

### Phase 3 — Server + LAN auth
- axum API
- token mint/auth middleware
- SSE job events
- remote CLI mode
- library content routes

### Phase 4 — MCP
- stdio tools
- `--proxy` thin mode
- Hermes/Claude docs
- timeout guidance

### Phase 5 — Browser UI
- Vite SPA embedded
- full studio + job board + library
- token gate screen

### Phase 6 — Slint native
- winit desktop MVP (connect to node)
- job list + generate forms
- linuxkms feature flag / CI build note
- licensing doc finalized

### Phase 7 — Optional power features
- xai_files profile
- MCP-over-HTTP
- OAuth experimental
- batch API helper
- daily spend caps dashboard
- tailscale-oriented install notes

**Agent-complete MVP = end of Phase 4.**  
**Human studio MVP = end of Phase 5.**  
**Appliance/kiosk = Phase 6.**

---

## 13. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Ephemeral URL expiry | local-first download always |
| MCP timeouts | status/wait split; server-side jobs |
| Mode/model illegal combos | core capability matrix |
| Agent cost runaway | estimate tool + token rate limits + max duration config |
| Slint license vs MIT core | separate crate; LICENSE clarity before publish |
| Huge base64 in LLM context | content_url only; never dump b64 in MCP results |
| Edge node disk full | library LRU/quota config |
| Schema drift upstream | fixtures + `--print-curl` + version pin notes |
| Browser token theft on shared machine | sessionStorage; short-lived tokens; admin rotation |

---

## 14. Reference code (read-only)

Hermes (patterns, not dependency):
- `~/.hermes/hermes-agent/plugins/video_gen/xai/__init__.py`
- `~/.hermes/hermes-agent/plugins/image_gen/xai/__init__.py`
- `~/.hermes/hermes-agent/tools/xai_http.py`

Official:
- https://docs.x.ai/developers/model-capabilities/imagine
- https://docs.x.ai/developers/rest-api-reference/inference/videos
- https://docs.x.ai/developers/rest-api-reference/inference/images
- https://docs.x.ai/developers/model-capabilities/imagine/files

---

## 15. Locked defaults checklist (folded 2026-07-28)

| Topic | Locked choice |
|---|---|
| Slint license | GPL-3.0 app crate; MIT OR Apache-2.0 headless stack |
| Browser SPA | Vue 3 + Vite, embedded via `rust-embed` |
| Default bind | `127.0.0.1:8791`; LAN bind explicit + token mandatory |
| Binary name | `imaginarium` only (no `imgr` alias in v1) |
| Job identity | Local ULID `job_id` primary; upstream `request_id` secondary |
| Scaffold timing | **After Andre read-over + explicit go** — not before |

No product forks left open for v1. Optional later (not blockers): MCP-over-HTTP details, token scope enum exact names, OpenAPI field-level draft (can land in Phase 0/3).

---

## 16. Read-over guide

When you review, focus on:

1. Multi-node auth + “server holds key” vs pass-through `X-XAI-Key` policy
2. Local-first library paths and whether edge clients only ever see `content_url`
3. MCP tool list granularity
4. Phase order (agent MVP = Phase 4; browser = 5; Slint = 6)
5. Anything missing for your existing LAN mesh pairing model

Then say go (or mark deltas) → Phase 0+1 scaffold at `~/Projects/Imaginarium-RS`.

---

## 17. One-sentence product definition

**Imaginarium-RS** is a local-first, multi-node Rust studio and agent gateway for xAI’s Imagine image/video API — CLI + MCP + LAN-token HTTP API + zero-install Vue browser UI + optional GPL Slint native/kiosk app — with the xAI key living on a fat node and assets landing on disk by default.
