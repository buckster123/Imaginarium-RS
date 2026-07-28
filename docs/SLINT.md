# Imaginarium native UI (Slint) — Phase 6

**Binary:** `imaginarium-app`  
**Crate:** `crates/imaginarium-slint` (**GPL-3.0-only**)  
**Test target:** **winit desktop** on the laptop (primary).  
**Optional later:** `backend-linuxkms-noseat` for Pi/kiosk (same patterns as ApexOS-RS `docs/slint-notes.md`).

Headless stack stays MIT/Apache and does not link this crate by default.

---

## Architecture

```
imaginarium-app (Slint + winit)
        │  HTTP  Authorization: Bearer <token>
        ▼
imaginarium serve  127.0.0.1:8791  /v1/*
        │
        ▼
xAI Imagine (fat node holds XAI_API_KEY)
```

Native UI is an **edge client** of the same LAN API as the Vue SPA. No second business logic path.

Thread model (ApexOS pattern):

- Main thread: `ui.run()` (Slint)
- Background: `tokio` multi-thread runtime for HTTP
- UI updates: `slint::invoke_from_event_loop` + `Weak` handles only

---

## Port map (web → Slint)

| Web (Vue) | Phase 6 Slint | Notes |
|---|---|---|
| Token gate | Connect bar (URL + token) | session/memory optional later |
| Image generate | Form + Run | **6.1** |
| Image result preview | `Image` element from library file path | download via `/v1/library/{id}/content` → temp/cache file |
| Jobs list | `VecModel` + refresh | **6.1** |
| Estimate | status line | optional |
| Video submit | forms only | **6.3** — no in-app player |
| Video preview | open path / external | mpv/xdg-open |
| Image craft canvas | **out of scope** | web-only Studio+ |
| Video craft timeline | **out of scope** | ffmpeg API remains web/CLI |
| Chain bar | simplified action buttons | job id → next mode |
| Settings tokens | later | admin scope |

---

## MVP slices

| Slice | Status intent |
|---|---|
| **6.0** | Crate + winit window + connect/health |
| **6.1** | Image gen + wait + still preview + jobs list |
| **6.2** | Library browse stills |
| **6.3** | Video ops submit + status (open file externally) |
| **6.4** | linuxkms feature + CI note |
| **ApexOS embed** | Separate handoff doc — not this binary |

---

## Build / run (laptop)

```bash
# Node must be up
imaginarium serve --bind 127.0.0.1:8791

# Native UI (from repo root)
cargo run -p imaginarium-slint --bin imaginarium-app

# Or:
./target/debug/imaginarium-app
```

Env (optional defaults in UI):

- `IMAGINARIUM_URL` — default `http://127.0.0.1:8791`
- `IMAGINARIUM_TOKEN` — prefill token field

Deps: `libfontconfig1-dev` (already common on your box).

---

## ApexOS-RS integration

See **`docs/APEXOS_IMAGINARIUM.md`** — requirements drop for the ApexOS-specialized agent.  
Imaginarium side delivers a stable HTTP API; ApexOS owns the in-shell tab/view.

Do **not** block Phase 6 laptop testing on ApexOS landing.
