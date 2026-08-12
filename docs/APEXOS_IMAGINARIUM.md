# ApexOS-RS ← Imaginarium integration requirements

**Audience:** ApexOS-RS specialized developer agent  
**Source of truth for Imagine API:** Imaginarium-RS `openapi/imaginarium-v1.yaml` + running `imaginarium serve`  
**Native reference client:** Imaginarium-RS `crates/imaginarium-slint` (standalone winit; GPL)  
**Date:** 2026-07-28

This doc is a **feature implementation brief** for embedding Imagine studio capabilities inside ApexOS-RS `ui-slint`. It is not an Imaginarium-RS task list.

---

## Goal

Add an ApexOS native surface (“Imagine” / “Studio” tab or tool window) that talks to a local or LAN **Imaginarium fat node**, so operators can generate/edit images (and later video) without leaving the ApexOS shell.

Agents already use Imaginarium via **MCP/CLI**. This is the **human native** path inside ApexOS.

---

## Non-goals (v1 embed)

- Reimplement xAI client inside agentd
- Port Studio+ craft canvas (paint/mask) or video NLE into Slint
- In-app video playback (use external open / later camera painter work)
- Replacing Vue SPA (remains for browser/LAN studio)

---

## Dependency topology

```
ApexOS ui-slint
    → HTTP client → http://<node>:8791/v1/*
         Authorization: Bearer <IMAGINARIUM_TOKEN>
         or X-Imaginarium-Token
    → (optional) agentd tool bridge that proxies same routes

Imaginarium serve holds XAI_API_KEY (BYOK). ApexOS must NOT require the xAI key in-process for v1.
```

Auth model matches ApexOS agentd LAN tokens (Bearer / header / query). Prefer storing token in ApexOS secure config, not chat logs.

---

## Minimum viable ApexOS surface (v1)

### UI

1. **Connect** — base URL + token + connection status (`GET /health` is public)
2. **Image generate** — prompt, model (or auto), n, aspect_ratio, resolution → `POST /v1/images/generations`
3. **Job status** — show job id, status, error; poll `GET /v1/jobs/{id}` or use wait
4. **Still preview** — fetch `GET /v1/library/{job_id}/content` (Bearer or `?token=`) → `slint::Image` / temp file
5. **Jobs list** — `GET /v1/jobs?limit=40`
6. **Open in browser** (optional) — link to `http://node:8791/` for full Studio+ craft

### API calls (stable)

| Action | Method |
|---|---|
| Health | `GET /health` |
| Models | `GET /v1/models` |
| Estimate | `POST /v1/estimate` |
| Image gen | `POST /v1/images/generations` |
| Image edit | `POST /v1/images/edits` |
| Video gen | `POST /v1/videos/generations` |
| Video edit/extend | `POST /v1/videos/edits`, `.../extensions` |
| Jobs | `GET /v1/jobs`, `GET /v1/jobs/{id}`, `POST /v1/jobs/{id}/wait` |
| Library bytes | `GET /v1/library/{id}/content` |
| Poster thumb | `GET /v1/library/{id}/thumb` (480px JPEG; lazy-built, eager on import) |
| Craft import | `POST /v1/library/import` (≤40 MB decoded; 64 MB body limit; images, video, **audio** — music beds) |
| Video craft render | `POST /v1/craft/video/render` (ffmpeg on node) |

**Craft engine (U2a+U2b, 2026-07-29 — timeline contract `version: 1`).** The
render pipeline normalizes every segment onto one canvas (aspect-fit + pad,
unified fps, `yuv420p`) before a stream-copy concat — mixed-resolution/fps
sources cut cleanly — then mixes ALL audio in one master-clock pass
(`amix normalize=0`): each clip's own audio at its timeline offset
(speed-matched via `atempo`) plus an optional **`music`** bed (`AudioTrack`:
library job id + `in_s`/`start_s`/`gain_db`/fades — import a Sonus track,
reference its job id). Segment kinds: **`clip`** (trim, `speed` 0.5–2.0),
**`still`** (image + Ken-Burns `zoom_from`→`zoom_to`, needs `dur_s`),
**`card`** (solid `card_color`, captions carry the text). The **`style`** block
holds config-not-code aesthetics: caption fontsize/color defaults, `card_bg`,
cinematic `letterbox_frac` bars with an animated `letterbox_reveal_s` open, and
a `loudnorm` two-pass EBU R128 ship pass. Timeline `overlays` are master-clock
seconds and render on every segment they intersect; segment `captions` are
segment-local. Durations are ffprobe-measured. Segments are content-hash cached
(`{data-home}/craft-segcache`, 2 GiB LRU) — tweak one card and only that card
re-encodes. Craft jobs carry full provenance in `meta.json` (contract version,
engine, ffmpeg version, the submitted timeline). Full schema: the OpenAPI
`VideoTimeline`.

**U3 additions (2026-07-29).** `POST /v1/craft/video/render?no_wait=true`
returns a **pending** craft job immediately and renders in the background —
poll `GET /v1/jobs/{id}` like any job (`POST /wait` on a craft job returns the
current row; it never touches upstream). A failed background render flips the
job to `failed` with the error — craft jobs never sit pending forever. Every
visual library job carries a **480px JPEG poster**: `GET /v1/library/{id}/thumb`
(eager on import, rebuilt lazily on demand — pre-U3 content included). The
`thumb.jpg` sidecar is never addressable as a media asset.

**Agent surface (MCP).** The proxy plugin's `imaginarium_craft_video` tool
drives the same engine: `{timeline: <VideoTimeline v1>, wait?: false}` →
pending job → `imaginarium_job_status` polls (craft jobs are DB-truth on every
path). The tool description carries the compact grammar — agents need no other
reference.

Body size: server `DefaultBodyLimit` is **64 MB** (craft/edit data-URLs). `POST /v1/library/import` additionally caps the **decoded** payload at **40 MB** (413 above that) — so the effective import ceiling is 40 MB decoded, not 64.

Full route/param/auth reference: **`openapi/imaginarium-v1.yaml`** (regenerated to match shipped code, 2026-07-28).

### Contract notes (verified against code 2026-07-28)

- **`model: "auto"`** — accepted on image/video/estimate (or omit `model`); selects the server default, and video auto-picks by modality (I2V→`video-1.5`, else `video`). Concrete model names still validate; unknown names 400.
- **Tokens are not logged.** `?token=` is accepted for browser `<img>`/`<video>` sources; the request-log span records method + path only (never the query string).
- **`GET /v1/jobs/{id}`** polls upstream once for a non-terminal video job (so `no_wait` + GET works over HTTP / MCP proxy). Terminal, image, and craft rows return as-is. A poll error returns the last known row.
- **`POST /v1/jobs/{id}/wait`** returns the job as-is for already-terminal / synchronous (image) jobs, `404` for an unknown id, and `502` only on a genuine upstream error.
- **Media fields** (`image`, `images[]`, `reference_images`, `video`) accept `library:{job_id}` (a node-library chain ref, resolved server-side — the **preferred** form for chaining a previous generation; `#n` addresses batch asset n), plus `data:` / `http(s):` / `file_…`. A bare local filesystem path is rejected (`400`); local paths work only via the CLI.
- **Multi-asset batches**: `GET /v1/library/{id}/content?i=N` addresses the N-th asset of an n>1 image job (default 0). `GET /v1/jobs` rows carry `prompt` + `assets` (count) so galleries need no N+1 detail fetches.

### Not in v1 — planned, do NOT build against yet

These appear in `docs/ARCHITECTURE.md` but are **not implemented**: SSE job events (`GET /v1/jobs/{id}/events`), library listing / get / delete / upload (`GET`/`DELETE /v1/library`, `GET /v1/library/{id}`, `POST /v1/library/upload`), a standalone video poll route (`GET /v1/videos/{id}`), and per-token rate limits / spend caps. For v1 the ApexOS surface should **poll `GET /v1/jobs/{id}`** (or `POST …/wait`) rather than expect SSE, and treat library assets via `GET /v1/library/{id}/content`.

### Slint patterns (already in ApexOS)

Reuse existing repo knowledge:

- Main thread = Slint; tokio off-main (`docs/slint-notes.md`)
- `invoke_from_event_loop` for model updates
- Workspace / explorer image load paths for still preview
- Image picker patterns for choosing local refs if edit/I2V needs files

---

## Suggested information architecture

| Option | When |
|---|---|
| **A. New top-level nav tab** “Imagine” | First-class studio |
| **B. Tool window** from chat / apps rail | Lighter touch |
| **C. Explorer integration** | “Send to Imagine” on image files only |

Recommend **A or B** for v1; C as follow-on.

---

## Config (proposed ApexOS)

```toml
# illustrative — fit ApexOS config style
[imaginarium]
enabled = true
base_url = "http://127.0.0.1:8791"
# token from env IMAGINARIUM_TOKEN or secret store — never log
token_env = "IMAGINARIUM_TOKEN"
```

---

## v2 (after v1 works)

- Video submit + “open file” for mp4
- Chain affordances (result → I2V / extend) matching Vue ChainBar
- Deep link into browser Studio+ for craft
- Optional agentd tools wrapping Imaginarium HTTP for non-MCP agents
- linuxkms: same views as winit (Imaginarium reference app tests winit first)

---

## Acceptance tests (ApexOS)

1. With Imaginarium serve running + valid token, health shows green in UI  
2. Image gen returns job id; still appears in preview within timeout  
3. Invalid token → clear 401, no panic  
4. Node down → clear offline state  
5. No xAI key required inside ApexOS process  
6. GPL: if linking GPL UI code from Imaginarium-slint, respect license boundary — prefer **reimplement thin views in ApexOS** calling HTTP rather than depending on `imaginarium-slint` crate (keeps ApexOS license posture clean). **HTTP API contract is the integration surface.**

---

## Reference implementations

| Artifact | Path |
|---|---|
| Vue studio | Imaginarium-RS `ui-web/` |
| OpenAPI | Imaginarium-RS `openapi/imaginarium-v1.yaml` |
| Multi-node/auth | Imaginarium-RS `docs/MULTI_NODE.md` |
| Standalone Slint app | Imaginarium-RS `crates/imaginarium-slint/` |
| Phase 6 notes | Imaginarium-RS `docs/SLINT.md` |

---

## Contact assumption

Imaginarium node operators set `XAI_API_KEY` + `IMAGINARIUM_TOKEN` on the fat node. ApexOS only needs URL + LAN token.
