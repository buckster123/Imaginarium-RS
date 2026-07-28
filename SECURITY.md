# Imaginarium-RS — Security Posture & Audit Findings

**Audit date:** 2026-07-28
**Scope:** full workspace (`crates/`), read-only.
**Method:** multi-agent audit swarm (6 dimension finders → adversarial verify → synthesis),
then manual triage/dedup by a reviewer (ApexOS-RS/FORGE) before integration into ApexOS-RS.
**Baseline health at audit time:** all tests pass, clippy clean, headless workspace + GPL
`imaginarium-slint` both compile.

> **Headline.** The code is well-built, but its trust model is **single-user-localhost**.
> Almost every real finding below is a place where the node **trusts the caller too much**.
> That is fine for `imaginarium serve` on loopback for one operator; it is **not** yet safe to
> expose on a LAN behind write/read tokens, or to embed `api_router` into another service —
> which is exactly what the ApexOS integration intends. Fix the **Gating** items before any
> untrusted-token exposure.

Provenance tags: **[confirmed]** = traced to code (and, where noted, reproduced) by the reviewer;
**[swarm]** = reported+verified by the audit swarm, plausible on review, not independently re-run.

---

## Gating — fix before the node is reachable by any untrusted token

### G1. Arbitrary local-file read + exfiltration via `MediaRef::Path` **[confirmed]**
`crates/imaginarium-core/src/types.rs:209` (`MediaRef::from_user_input`),
`crates/imaginarium-core/src/client.rs:155/178` (read+base64), reached from
`crates/imaginarium-server/src/routes.rs:144` (`image_edit`) and
`crates/imaginarium-mcp/src/backend.rs:138`.

`from_user_input` treats **any** string that is not a URL / `data:` / `file_…` id as a **local
filesystem path**, and the server + MCP backends then `std::fs::read` it, base64-encode it, and
ship it upstream to xAI. This is the *normal* API path, not a dev flag.

- **Scenario:** a **write**-scoped LAN token calls `POST /v1/images/edits` with
  `{"images":["/etc/shadow"], "prompt":"..."}`. The node reads that file off its own disk and
  exfiltrates it to xAI. Same for `image`/`video` fields on the video routes and every MCP image/video tool.
- **Fix:** keep the `Path` variant for the **local CLI only**. Add a `MediaRef::from_remote_input`
  that never yields `Path`, and use it in every `routes.rs` handler and the MCP `LocalBackend`.
  Accept only `data:`/`http(s):`/`file_…` from network/agent callers — or confine `Path` reads to
  an allow-listed dir (`canonicalize()` + assert `starts_with(root)`) with a size + media-type check.
  This is the same idea as ApexOS `apexos-confine`.

### G2. Path traversal in `GET /v1/library/{id}/content` **[confirmed]**
`crates/imaginarium-server/src/routes.rs:354` → `crates/imaginarium-server/src/lib.rs:116`
(`job_content_path`); same root cause in `crates/imaginarium-core/src/craft_video.rs:76`
(`resolve_job_media`).

`id` comes straight off the URL and is `day.join(id)`-ed with **no validation** that it is a job
id. `docs/ARCHITECTURE.md:502` claims a "path traversal guard on library" — it does not exist.
Axum's `Path<String>` won't match a literal `/`, so this is bounded, but `..` segments and
percent-encoding should not be trusted to be defeated by the router.

- **Scenario:** crafted `id` escapes the library root to read any media-extension file
  (`.png/.jpg/.mp4/...`) elsewhere on disk under the Read scope.
- **Fix:** reject ids that aren't a plain job token (`id.chars().all(|c| c.is_ascii_alphanumeric() || c=='-' || c=='_')`,
  or look the id up in `JobStore` first), then `canonicalize()` the resolved path and assert it
  `starts_with(library_root)` before reading. (A DB lookup also fixes the O(library) tree-walk-per-request.)

### G3. Localhost Admin bypass trusts a config string, and `api_router` ignores its flag **[confirmed]**
`crates/imaginarium-server/src/auth.rs:52-64`; `crates/imaginarium-server/src/routes.rs:27`
(`api_router(state, _allow_localhost_no_auth)` — parameter discarded).

The bypass grants `TokenScope::Admin` and decides "this request is local" by re-reading
`cfg.server.bind` (a **deserialized string**), never the real peer address (the code's own comment
concedes it can't see the peer without `ConnectInfo`). Separately, `api_router` throws away its
`allow_localhost_no_auth` argument and reads `state.cfg` instead.

- **Why it matters for ApexOS:** the intended embed path mounts `api_router` in another axum app.
  An embedder gets **neither** `serve()`'s `!loopback && !has_any_auth() → bail` refusal gate
  **nor** any real loopback check, and its explicit `false` argument is silently ignored.
- **Fix:** take the peer from `ConnectInfo<SocketAddr>` and gate the bypass on
  `peer.ip().is_loopback()`. Make the flag an explicit `AppState` field set by the router
  constructor (stop ignoring the parameter), and move the `has_any_auth()` refusal gate **into**
  `api_router` so embedders inherit it.

### G4. ffmpeg `drawtext` injection via craft overlay text **[swarm]** (escaping bug **[confirmed]**)
`crates/imaginarium-core/src/craft_video.rs:349` (`escape_drawtext`), used at line 233; reached via
the **write**-scoped `POST /v1/craft/video/render`.

`escape_drawtext` turns `'` into `\'`, but ffmpeg single-quoted strings do **not** honor backslash
escaping — the `'` still closes the quote (the benign symptom: captions with apostrophes already
render a stray backslash, finding L-cluster below). The swarm reports this is weaponizable to break
out of the `drawtext` filter into the filtergraph (arbitrary `textfile=` read / injected filters).

- **Fix:** escape the ffmpeg-correct way — replace `'` with the sequence `'\''`
  (close-quote, escaped-quote, reopen) — and/or write the caption to a temp file and use
  `drawtext=textfile=…` instead of `text=`. Bound the overlay count.

---

## Secondary — real, lower blast radius

### S1. Wildcard CORS + the dev bypass **[confirmed, dev-flag-gated]**
`crates/imaginarium-server/src/lib.rs:76-79` (`allow_origin(Any).allow_methods(Any).allow_headers(Any)`).

The swarm reproduced a full "any web page mints an admin token" chain **end-to-end with curl** —
but only with `--allow-localhost-no-auth` **on** (the documented dev invocation,
`docs/MULTI_NODE.md:60`). That flag defaults **off** (`config.rs:136`), and with it off, wildcard
CORS alone is **not** exploitable: auth is a bearer token in a header, not an ambient cookie, so a
foreign origin just gets 401. Treat as a **dev-mode footgun**, not a default-config hole.

- **Fix (cheap, do it anyway):** restrict CORS to the node's own origin(s) for `/v1/*`, and if the
  bypass is kept, reject cross-origin requests (`Origin` allowlist / `Sec-Fetch-Site`) when the
  bypass identity is used.

### S2. LAN tokens logged via `?token=` query **[swarm]**
`crates/imaginarium-server/src/lib.rs:86` (`TraceLayer::new_for_http()` default span records the
full URI incl. query). `docs/MULTI_NODE.md` / `docs/ARCHITECTURE.md:504` claim "redact tokens in logs".

- **Scenario:** `GET /v1/...?token=img_…` under `RUST_LOG=debug` writes the plaintext token to logs.
- **Fix:** custom span recording only `method` + `uri.path()`; prefer the `X-Imaginarium-Token`
  header (or short-lived signed asset URLs) over `?token=` in the SPA.

---

## Notes for ApexOS integration

- Keep the xAI key **only** on the Imaginarium node — the recommended integration is HTTP-client,
  not linking `imaginarium-core` into agentd (`docs/APEXOS_IMAGINARIUM.md` non-goal #1). The
  findings above reinforce keeping the key/read blast-radius in its own process.
- Default bind `8791` does not collide with agentd `8787`.
- If ApexOS ever embeds `api_router`, G3 must be fixed first.

See `BACKLOG.md` for the full engineering task list (these items + correctness + doc-drift + minor).
