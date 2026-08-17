# Imaginarium-RS — path forward (2026-08-17)

Parent: [`2026-08-17-codebase-audit.md`](2026-08-17-codebase-audit.md).  
Ledger: [`BACKLOG.md`](../../BACKLOG.md).

This is a sequencing document, not a new architecture. The locked plan in
`docs/ARCHITECTURE.md` still holds. We are closing honesty leftovers and
picking the next *product* fork after v0.1 dogfood.

---

## Recommendation

**(a) Honesty → studio adoption → MCP parity → CI, then one product fork.**

That order matches how the node is used today: Andre + agents on loopback,
ApexOS already speaking HTTP, Slint and garden adapters not on the critical
path.

| Option | When to pick it instead |
|---|---|
| **(a) Honesty-first** (recommended) | Default. Jobs must not strand; spend must mean something; studio must stop re-uploading. |
| **(b) Studio-first** | You are in the browser every night and the data-URL chain / n>1 thumbs are the pain. Still do CORE-02 in the same week — stranded video jobs will bite the SPA too. |
| **(c) Agent-first** | Hermes / ApexOS MCP is the main surface this week. Then MCP-01/02/04/05/06 + CLI-01 before Vue. |

Do **not** start fal.ai / ComfyUI, SSE, library CRUD, linuxkms, or Slint 6.3
until the P1 wave is green.

---

## Wave 0 — this PR (docs only)

- Land this folder + BACKLOG rematch.
- Strike or footnote the known lies (remote CLI, `X-XAI-Key`, “Phase 4 stub”,
  “craft coming”, workflow “1.5 is I2V-only”) in a follow-up docs slice if
  they do not fit here.
- Add `LICENSE-GPL` (or `COPYING`) next to the GPL crate. Cheap compliance.

No code. Merge when you have read the headline.

---

## Wave 1 — honesty leftovers (one or two PRs)

The Aug 12 trail was “no stranded, silent, or clobbered jobs.” Video and
spend still have holes.

| Slice | IDs | Why first |
|---|---|---|
| **1a Job honesty** | CORE-02, CORE-01 | Stranded `Running` + missing assets make agents re-spend. |
| **1b Money honesty** | CORE-03, MCP-04 / SEC-02 | Daily cap must reserve; `error_type=spend_limit` must appear on MCP + HTTP 400. |

Acceptance:

- Missing `library:` on video generate/edit/extend → `Failed`, never eternal `Running`.
- `2026/notes.txt` in the library tree cannot hide `2026/08/17/<job>/00.png`.
- Two concurrent jobs cannot both pass `max_usd_per_day` when the sum would exceed it.
- `imaginarium_image_generate` under a tiny `max_usd_per_job` returns tool text with `error_type=spend_limit` (not only `-32000`).
- HTTP 400 spend/invalid_mode body includes `error_type`.

Keep each PR on a feature branch. Tests first: the swarm listed them.

---

## Wave 2 — studio U1 (the actual BACKLOG leftover)

The API already has `?i=`, `library:{job_id}`, and jobs-list `prompt`.
The SPA ignores all three.

| Slice | IDs |
|---|---|
| **2a Addressing** | UI-01 `assetSrc` / `content_url?i=`, UI-02 chain sends `library:{id}` |
| **2b Honesty in the chrome** | UI-04 `ok=false`, UI-03 JobBoard prompt + error, UI-05 401 re-lock / 429 copy |

Acceptance:

- n=2 image gen shows two different stills.
- “→ I2V” / “→ Extend” POSTs `library:<id>`, not a data-URL (network tab).
- Download-miss job toasts the error, does not look like success.
- Jobs table shows a truncated prompt without opening JSON.

Tick the STUDIO_PLUS 5.1 boxes that are already true (toast, chain, Ctrl+Enter,
prefs, download) so the next session does not rebuild them. Footer string
dies in this wave.

Out of scope here: engine-v1 music/style UI, Slint craft, SSE.

---

## Wave 3 — MCP / CLI parity (agents + README)

| Slice | IDs |
|---|---|
| **3a Transport** | MCP-01 re-init, MCP-02 UTF-8 (MCP-03 oversized id is P3; do it if you are already in `transport.rs`) |
| **3b Proxy = local** | MCP-05 `voice_id`, MCP-06 HTTP `model:auto` on edit/extend, MCP-07 craft wait |
| **3c CLI** | CLI-01 `--model auto`; optionally flip image default to `2.0` to match the studio + skill |

Acceptance:

- Second `initialize` on the same stdio is not `-32601`.
- `0xFF\n` then `ping` keeps the session.
- MCP `--proxy` + `voice_id: eve` reaches the node as `reference_audios`.
- `POST /v1/videos/edits` with `"model":"auto"` is not 400.
- `imaginarium video gen -p x --model auto` parses.

---

## Wave 4 — CI and contract tests

v0.1 is dogfooded. The next consumer is ApexOS hitting this OpenAPI.

| Slice | IDs |
|---|---|
| **4a CI honesty** | OPS-01 slint clippy/build, OPS-02 `npm ci && vite build` + `git diff --exit-code ui-web/dist` |
| **4b Contract tests** | TEST-01 httpmock `ImagineClient` (timeout, download miss, spend-before-POST), TEST-02 axum oneshot (401/403, ConnectInfo fail-closed, 429, import 413, safe id) |
| **4c Hygiene** | OPS-05 PR template lists server tests; OPS-06 `cargo test --workspace --exclude imaginarium-slint` in the README; optional `cargo-deny` |

Do not block Waves 1–3 on this. Do not declare “CI covers the workspace”
until 4a is green.

---

## After Wave 4 — pick **one** product fork

### (a) Remote CLI / edge human (recommended if the mesh is the next user)

ARCHITECTURE promised `IMAGINARIUM_URL` + `imaginarium video gen`.
Only MCP and Slint speak HTTP today. Implement global `--url` / env + token
and reuse `ProxyBackend`. Then `library import` / `library ls` so craft music
beds are not SPA-only (CLI-03, CLI-04, MCP-09).

### (b) Slint 6.2 stills (recommended if the laptop app is used)

Connect already lies (SLINT-01 — fix in Wave 1 or here). Then library browse
stills + job preview. **Not** 6.3 video, **not** linuxkms, **not** ApexOS
embed. Rewrite `ARCHITECTURE.md` §6.3 parity table if you skip this fork.

### (c) Craft `no_wait` + music bed in Vue (recommended if you cut clips weekly)

UI-09. Engine already has `?no_wait=true` and `music`. Wire those two
controls and poll like any job. Do not build still/card/Ken Burns in the
bench until those two earn their keep.

### Parked until someone else moves

| Item | Owner |
|---|---|
| fal.ai + ComfyUI adapters | Imaginarium protocol, **after** ApexRouter `GARDEN.md` §7 |
| SSE / library CRUD / `GET /v1/videos/{id}` | Nobody — not in OpenAPI |
| `X-XAI-Key` / `key_source=client` | Phase 7, and probably never (key stays on the fat node) |
| OAuth SuperGrok | Out of v1 |
| linuxkms / ApexOS in-shell Imagine tab | ApexOS HTTP client; this repo only keeps the contract stable |
| Multi-track / AE-ish craft | Studio+ 5.5+ |

---

## Suggested branch names

```
fix/video-job-fail-closed          # CORE-02
fix/library-walk-stray             # CORE-01
fix/spend-reserve-and-error-type   # CORE-03 + MCP-04
feat/studio-u1-library-refs        # Wave 2
fix/mcp-handshake-and-proxy        # Wave 3
chore/ci-slint-and-dist            # Wave 4
```

One concern per PR. `fmt` / `clippy -D warnings` / `test` on the four
headless crates, same as #1–#19.

---

## What “done” looks like for the next month

- BACKLOG “Still open” is only items we have chosen to park, each with a
  reason.
- A Write token cannot strand a video job or double-book the daily cap.
- The SPA chains with `library:` and shows n>1.
- An agent can re-init, survive a bad UTF-8 byte, and see `error_type`.
- CI compiles Slint and fails if `ui-web/dist` is stale.
- Garden / SSE / kms have not started.

That is a tighter, more honest v0.1 — not a v0.2 product.
