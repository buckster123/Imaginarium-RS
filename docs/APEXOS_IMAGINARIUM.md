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
| Craft import | `POST /v1/library/import` (large body; server allows 64MB) |
| Video craft render | `POST /v1/craft/video/render` (ffmpeg on node) |

Body size: server `DefaultBodyLimit` **64MB** for craft data-URLs.

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
