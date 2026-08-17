# Imaginarium-RS — Backlog

**Rematch: 2026-08-17** (Grok audit swarm — core / MCP+CLI / server / UI / ops;
write-up in `docs/audit/2026-08-17-codebase-audit.md`). Previous rematch
2026-08-12 (slices a–d + §11). Original source: 2026-07-28 pre-integration
audit (`SECURITY.md`). That file is the forensic write-up; **this file is the
current ledger.** Do not treat an unchecked box as “never looked at”; the date
on the line is the last verdict.

Provenance: **[confirmed]** traced to current code · **[swarm]** audit-verified.

---

## Next (queued)

Path-forward detail: `docs/audit/2026-08-17-path-forward.md`. Recommended order:

1. **Honesty leftovers** — video `Running` after media-read fail (CORE-02);
   library walk stray/non-dir abort (CORE-01); daily spend reservation (CORE-03);
   `error_type` on MCP/HTTP 400 (MCP-04 / SEC-02).
2. **Studio U1** — `assetSrc` `?i=`; chain `library:{id}`; JobBoard prompt;
   toast `ok=false`.
3. **MCP / CLI parity** — re-init; UTF-8 frame; proxy `voice_id`; HTTP
   `model:auto` on edit/extend; CLI `--model auto`.
4. **CI honesty** — slint build; dist dirty-gate; httpmock + axum oneshot.

Pick one product fork only after those: remote CLI, Slint 6.2 stills, or craft
`no_wait` + music bed. Garden adapters stay parked.

---

## Closed — July gating / honesty (code in tree)

- [x] **G1** Remote media gate (`MediaRef::from_remote_input` + `media_from_node_input`).
- [x] **G2** Safe asset-id on `/v1/library/{id}/content` (no `..` / traversal).
- [x] **G3** Localhost bypass uses `ConnectInfo` loopback; fail-closed.
- [x] **G4** `drawtext` uses `'\''` + 32-caption cap.
- [x] **S1** No `CORS Any` on `/v1/*`.
- [x] **S2** Trace spans are method+path only (no `?token=`).
- [x] **B1** Video poll timeout stays `Running` + `error_type=timeout` (re-pollable).
- [x] **B2** MCP / CLI tracing on stderr.
- [x] **B3** Empty 2xx → parse fail, not silent `ok=true`.
- [x] **B4** Image post-response failures `fail_job_err`; video dir/prompt too.
      *2026-08-17: video **media-read after upsert** still strands — see Still open / CORE-02. Image path stays closed.*
- [x] **B5** `auto_download` miss → `Done` + `ok=false` + `error_type=download`.
- [x] **B6** Terminal upsert not clobbered by stale `running`.
- [x] **C1** Brief/OpenAPI trimmed to shipped routes (SSE / library CRUD still absent — by design).
- [x] **C2** Import 40 MB decoded / 64 MB body — numbers aligned; size estimated **before** decode.
- [x] **C3** Token redaction matches S2.
- [x] **C4** `POST /wait` is 200 terminal / 404 unknown / 502 upstream.
- [x] **C5** HTTP `model:"auto"` via `parse_model_selector`.
      *2026-08-17: **generate + image** only. Video edit/extend HTTP still `ModelId::parse` — see Still open / MCP-06.*
- [x] **C6a** Optional `[limits] max_usd_per_job` / `max_usd_per_day` (omit or 0 = off).
- [x] **C6b / §11** Per-token paid-request token bucket (`paid_rpm` default 30,
      `paid_burst` 10; `0` = off). Minted token / node env / local CLI-MCP.

## Closed — Aug 12 slices a–d

- [x] **Image 2.0 roster** — `grok-imagine-image-2.0`, `quality=low|medium`, $0.04.
- [x] **Video 1.5 roster** — T2V/I2V/R2V (not I2V-only); 1080p T2V/I2V; R2V 720p;
      `reference_audios` preset `voice_id` (max 3). Generate default = 1.5.
- [x] **GET `/v1/jobs/{id}` polls** `video_status_once` (HTTP + MCP proxy `no_wait`).
- [x] **`content_url`** set when a local file landed (`?i=N` for batches).
- [x] **Image transport** failures mark `Failed` (never stranded `Running`).
- [x] **Craft caps** — canvas 4096/edge, 48 segs, still/card 30s, master 20 min;
      ffmpeg killed at 5 min; clip `-t` is output duration; `out_s` clamped.
- [x] **MCP typed args** — string-typed `n`/`duration`/`no_wait` rejected.
- [x] **`imaginarium mcp` honors `--config` / `--data-home`.**
      *Standalone `imaginarium-mcp` binary still ignores them (CLI-05).*
- [x] **MCP `tools/call` is spawned** — `ping` / `tools/list` not blocked by a wait.
- [x] **Craft cache** — length-prefixed captions; prune mutex + keep-set + touch hits.

---

## Still open

Track-it and rematch leftovers. Not blockers for LAN loopback.

### Honesty (Wave 1)

- [ ] **CORE-02** Video generate/edit/extend media-read after upsert uses `?` — missing `library:` leaves `Running`. `client.rs`
- [ ] **CORE-01** Library walk: non-dir under `YYYY/` / `YYYY/MM/` aborts lookup; fallback nth-media + video-first `00.mp4` can hide `00.png`. `library.rs`
- [ ] **CORE-03** Daily USD cap does not reserve in-flight `usage`; check-then-act. `client.rs` / `jobs.rs`
- [ ] **MCP-04 / SEC-02** Spend / invalid_mode: MCP `-32000` and HTTP `{ok:false,error}` omit `error_type`. `dispatch.rs` / `routes.rs`

### Studio U1 (Wave 2)

- [ ] Web studio adoption: `assetSrc` uses `?i=`; chain loads use `library:` refs;
      JobBoard shows projected prompt. *2026-08-17: still open (UI-01/02/03). Plus toast `ok=false` (UI-04).*

### MCP / CLI (Wave 3)

- [ ] MCP `initialize` only on the first frame; re-init breaks the handshake. `mcp/lib.rs`
- [ ] One invalid UTF-8 byte on stdin kills the MCP session. `mcp/transport.rs`
- [ ] Oversized MCP frame answered with a null-id error. `mcp/transport.rs`
- [ ] **MCP-05** Proxy drops `voice_id` / `voice` (local folds them into `reference_audios`). `backend.rs` / `VideoGenBody`
- [ ] **MCP-06** HTTP video edit/extend reject `model:"auto"`. `routes.rs`
- [ ] **CLI-01** `imaginarium --model auto` is `unknown model`. `cli/main.rs`

### Parked (not Wave 1–3)

- [ ] MCP `job_wait` on craft: local errors, proxy returns current row. `backend.rs`
- [ ] `imaginarium-mcp` binary ignores `--config` / `--data-home`. `mcp/main.rs`
- [ ] Remote CLI (`IMAGINARIUM_URL` + `video gen`) — documented, not built.
- [ ] `library ls` / `import` on CLI + MCP import tool.
- [ ] Slint 6.2 library / 6.3 video / 6.4 linuxkms.
- [ ] Garden fal.ai + ComfyUI adapters (`notes/queued-open-model-upstreams.md`).
- [ ] SSE / library CRUD / `GET /v1/videos/{id}` — **not in OpenAPI; do not build.**

---

## Closed — Aug 17 rematch (docs / confirmation only)

- [x] `estimate_video` quotes by resolution (1.5: $0.08 / $0.14 / $0.25). Official card still tiered. `estimate.rs`
- [x] `b64_json` persisted even when `auto_download` is off. `client.rs`
- [x] `video_extend` rejects duration outside 2–10 (no silent clamp). `models.rs`
- [x] Image AR / resolution / `n` / model validated in `validate_image`. `models.rs`
- [x] Jobs list `limit` capped at 100 (0 → 20). `jobs.rs`
- [x] `auth_headers()` returns Config error on a control char in the API key. `client.rs`
- [x] `library_content` streams the file (chunked + Content-Length). `routes.rs`
- [x] `POST /v1/estimate` is Read-scoped. Wait stays Read. `auth.rs`
- [x] MCP `model:"auto"` on **local** video generate/edit/extend uses `parse_model_selector`.

---

## Studio arc (2026-07-29 →)

Charter lives in ApexOS-RS `docs/imagine-studio.md` and `docs/STUDIO_PLUS.md`.

- [x] **U1 API** `library:{job_id}[#n]` + `?i=` + jobs-list projection
- [x] **U2a** craft normalize→concat + music bed
- [x] **U2b** timeline contract v1
- [x] **U3** thumbs + async craft render
- [x] **A7** `imaginarium_craft_video` MCP tool
- [ ] **U1 web** `assetSrc` uses `?i=`; chain loads use `library:` refs;
      JobBoard shows projected prompt
- [x] *5.1 partial (2026-08-17):* toast, chain bar, Ctrl+Enter, session prefs,
      global busy, download/copy. Still missing: Esc toast, 401 re-lock,
      `ok=false` toast, cancel on `no_wait`. `docs/STUDIO_PLUS.md` checkboxes
      are stale — rematch before rebuilding.
