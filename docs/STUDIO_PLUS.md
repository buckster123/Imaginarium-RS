# Imaginarium Studio+ (post-MVP craft layer)

**Status:** locked 2026-07-28  
**Product fence:** Kdenlive-light + PaintShop-ish — **not** Premiere/PS/AE parity  
**Parent:** Imaginarium-RS remains local-first AI Imagine gateway + library + agents  
**Human studio MVP:** Phase 5 (done). Studio+ = Phase 5.x incremental craft.

---

## 0. Why

AI gen/edit hits ~80%. The last 20% is human hands:

- crop / reframe before I2V  
- paint a mask so AI edit doesn’t invent chaos outside the subject  
- trim dead heads/tails, fade audio, simple cuts  
- overlay text / sticker / watermark  
- merge two clips, export, feed back into extend/edit  

Today that means: download → external app → re-import. Studio+ closes the loop **in the same library / job graph**.

---

## 1. Explicit non-goals (v1–v1.5)

| Out of scope | Why |
|---|---|
| Full layer PS / blend modes zoo | multi-month DCC |
| Multi-track color grade / scopes | Resolve territory |
| Real-time GPU effects engine | after-effects arc later |
| Plugin marketplace | later |
| Cloud collab / multi-user locks | LAN single-node first |
| Replacing AI edit | AI stays; craft **alongside** |

**Later arc (explicitly parked):** “After Effects–ish” motion graphics addon — only after 5.2–5.3 see real use.

---

## 2. Product metaphor

```
┌─────────────────────────────────────────────────┐
│  Imaginarium Studio                             │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ AI Gen   │→ │ Library  │← │ Craft bench   │  │
│  │ AI Edit  │  │ Jobs     │  │ image / video │  │
│  └──────────┘  └────┬─────┘  └───────┬───────┘  │
│                     │  chain assets  │          │
│                     └────────────────┘          │
└─────────────────────────────────────────────────┘
```

Every craft export creates a **new library job/asset** (ULID), never silently mutates the original (non-destructive default). Optional “replace working copy” later.

---

## 3. Phased slices

### 5.1 — Daily-driver polish *(next / in progress)*

UX that helps every gen tonight — no craft engines yet.

- [x] Toast / banner for success & errors (not only inline `p.err`)
- [ ] Global busy / progress for long video waits (elapsed + cancel when no_wait path)
- [ ] **Chain actions** on result: Use as I2V still · Use as edit source · Use as extend source · Send to Jobs
- [ ] Stronger job status labels + auto-refresh Jobs while pending
- [ ] Keyboard: `Ctrl/Cmd+Enter` generate, `Esc` clear error toast
- [ ] Empty states + friendlier offline/401 copy
- [ ] Result preview: download button, open content URL, copy job id
- [ ] Remember last model/AR/res in sessionStorage (per tab)

### 5.2 — Image craft MVP (“PaintShop light”)

Browser canvas; export → library via API.

| Tool | Notes |
|---|---|
| Crop / rotate 90 / flip | aspect presets 1:1 16:9 9:16 |
| Adjust | exposure, contrast, saturation, simple levels |
| Paint + eraser | hard/soft brush, color, size |
| Mask layer | grayscale mask export for future AI edit-with-mask |
| Text overlay | basic title/caption |
| Export | PNG/WebP/JPEG → `POST` new asset or local data-URL → image edit/gen chain |

**Stack:** Vue + Canvas 2D (WebGL only if adjust pipeline needs it).  
**Server:** optional `libvips` later for large stills; v1 can do client-side export + upload path as data-URL to existing edit/library endpoints (or new `POST /v1/library/import`).

### 5.3 — Video craft MVP (“Kdenlive light”)

Browser timeline UI; **encode on fat node via ffmpeg**.

| Tool | Notes |
|---|---|
| Import 1–N library clips | from job ids / upload |
| Trim in/out per clip | frame-ish via seconds UI |
| Ordered cut list | simple single-track |
| Fade audio / fade video | ffmpeg afade/xfade or fade filters |
| Volume | per-clip gain |
| Text / image overlay | drawtext, overlay — limited fonts on node |
| Export | EDL/JSON decision list → `POST /v1/craft/video/render` → job + mp4 in library |

**Stack:**
- UI: lightweight timeline component (custom divs; no full OE-like engine)
- Server: `ffmpeg` on PATH (document dependency); spawn with timeout; progress via job poll
- Never block agent context with base64 video

### 5.4 — AI ↔ craft closed loop

- Image craft mask → AI image edit (when API supports mask / as soft guide in prompt+ref)
- Craft still → one-click I2V
- Trimmed video end → Extend
- Merged export → AI video edit
- “Send to craft” from any result card

### 5.5+ (parked)

- Multi-track  
- Keyframed motion / AE-ish  
- Color wheels  
- LUT import  
- Slint native craft surface  

---

## 4. API additions (incremental)

```
POST /v1/library/import          # multipart or data-URL → library asset + job
POST /v1/craft/image/export      # optional server-side vips path
POST /v1/craft/video/render      # body: timeline JSON → async job
GET  /v1/jobs/{id}               # already exists — craft jobs use same store
```

Timeline JSON sketch (5.3):

```json
{
  "width": 1280,
  "height": 720,
  "fps": 24,
  "audio_fade_in_s": 0.3,
  "audio_fade_out_s": 0.5,
  "clips": [
    { "job_id": "…", "in_s": 0.0, "out_s": 4.2, "gain_db": 0 },
    { "job_id": "…", "in_s": 0.5, "out_s": 3.0, "gain_db": -1.5 }
  ],
  "overlays": [
    { "type": "text", "text": "frenship protocol", "start_s": 0.5, "end_s": 3.0, "x": 40, "y": 40 }
  ]
}
```

---

## 5. UX principles

1. **Non-destructive** — originals stay; craft writes new job ids  
2. **Library is source of truth** — craft opens assets by job_id, not anonymous blobs  
3. **Agents stay clean** — MCP never receives giant frames; craft is human UI (+ optional thin MCP `craft_render` later)  
4. **ffmpeg is a hard dep for 5.3+ video export** — document in README; graceful error if missing  
5. **Scope fence** — if a feature needs a plugin architecture or AE graph, it’s 5.5+ / separate arc  

---

## 6. Success metrics (dogfood)

| Signal | Target |
|---|---|
| Chain “result → I2V” without leaving UI | 5.1 |
| Fix a still with crop+adjust and re-I2V | 5.2 |
| Trim + fade + export a 6s clip without Kdenlive | 5.3 |
| Full loop still → I2V → trim → extend | 5.4 |

---

## 7. Implementation order (locked)

1. **5.1 polish** (this session / next commits)  
2. **5.2 image craft**  
3. **`POST /v1/library/import`** (shared primitive)  
4. **5.3 video craft + ffmpeg render**  
5. **5.4 chain glue**  

License: craft stays in MIT/Apache headless + Vue (no Slint required for Studio+).

---

## 8. One-liner for Cerebro / README

> Studio+ = post-MVP human craft bench (PaintShop-light images + Kdenlive-light video via ffmpeg), chained to Imagine AI gen/edit inside the same local library — not Adobe parity.
