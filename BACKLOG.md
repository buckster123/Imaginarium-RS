# Imaginarium-RS — Backlog

**Rematch: 2026-08-12** (Grok audit swarm + slices a–d on `feat/mcp-craft-tool`).
Original source: 2026-07-28 pre-integration audit (`SECURITY.md`). That file is the
forensic write-up; **this file is the current ledger.** July checkboxes were stale —
many items landed before this rematch. Do not treat an unchecked box below as
"never looked at"; the date on the line is the last verdict.

Provenance: **[confirmed]** traced to current code · **[swarm]** audit-verified.

---

## Next (queued)

- [ ] **§11 per-token rate limit** — token-bucket throttle per minted token (and
      node env token), independent of the estimated-USD spend caps. Needed for
      intense agent loops that stay under a dollar but still slam the xAI quota.
      ARCHITECTURE §11 used to list this as if it shipped; it does not yet.
      Design when the next session rolls over.

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
- [x] **B5** `auto_download` miss → `Done` + `ok=false` + `error_type=download`.
- [x] **B6** Terminal upsert not clobbered by stale `running`.
- [x] **C1** Brief/OpenAPI trimmed to shipped routes (SSE / library CRUD still absent — by design).
- [x] **C2** Import 40 MB decoded / 64 MB body — numbers aligned; size estimated **before** decode.
- [x] **C3** Token redaction matches S2.
- [x] **C4** `POST /wait` is 200 terminal / 404 unknown / 502 upstream.
- [x] **C5** HTTP `model:"auto"` via `parse_model_selector`.
- [x] **C6a** Optional `[limits] max_usd_per_job` / `max_usd_per_day` (omit or 0 = off).

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
- [x] **MCP `tools/call` is spawned** — `ping` / `tools/list` not blocked by a wait.
- [x] **Craft cache** — length-prefixed captions; prune mutex + keep-set + touch hits.

---

## Still open

Track-it and rematch leftovers. Not blockers for LAN loopback.

- [ ] **C6b / §11** Per-token rate limit (see Next).
- [ ] `estimate_video` ignores resolution → 1080p 1.5 under-quoted. `estimate.rs`
- [ ] `b64_json` discarded unless `auto_download` is on. `client.rs`
- [ ] `video_extend` silently clamps duration (2–10) instead of rejecting. `client.rs`
- [ ] Image AR / resolution / model not fully validated against the matrix. `models.rs`
- [ ] Unbounded `?limit` on `/v1/jobs`. `jobs.rs`
- [ ] `auth_headers()` panics on a control char in the API key. `client.rs`
- [ ] `library_content` buffers the whole asset in RAM. `routes.rs`
- [ ] MCP `initialize` only on the first frame; re-init breaks the handshake. `mcp/lib.rs`
- [ ] One invalid UTF-8 byte on stdin kills the MCP session. `mcp/transport.rs`
- [ ] Oversized MCP frame answered with a null-id error. `mcp/transport.rs`
- [ ] A stray file in a job dir can make asset lookup miss. library walk
- [ ] `POST /v1/estimate` is Write-scoped; `POST …/wait` is Read-scoped though it
      polls + writes disk. Scope-map tidy. `auth.rs`
- [ ] MCP `model:"auto"` on local video edit/extend still goes through `ModelId::parse`
      (HTTP generate path is fine).

---

## Studio arc (2026-07-29 →)

Charter lives in ApexOS-RS `docs/imagine-studio.md`.

- [x] **U1** `library:{job_id}[#n]` + `?i=` + jobs-list projection
- [x] **U2a** craft normalize→concat + music bed
- [x] **U2b** timeline contract v1
- [x] **U3** thumbs + async craft render
- [x] **A7** `imaginarium_craft_video` MCP tool
- [ ] Web studio adoption: `assetSrc` uses `?i=`; chain loads use `library:` refs;
      JobBoard shows projected prompt
