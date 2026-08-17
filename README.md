<div align="center">

<img src="assets/banner.jpg" alt="Imaginarium-RS" width="100%">

<h1>Imaginarium-RS</h1>

<p><strong>A local-first, multi-node gateway to xAI's Imagine image &amp; video API.</strong><br>
One Rust workspace — CLI · MCP · LAN-token HTTP API · zero-install browser studio · optional native app.</p>

<p>
<img alt="license" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue">
<img alt="app license" src="https://img.shields.io/badge/native%20app-GPL--3.0-8A2BE2">
<img alt="rust" src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white">
<img alt="status" src="https://img.shields.io/badge/status-v0.1%20%C2%B7%20local--first-brightgreen">
</p>

</div>

---

> [!NOTE]
> **Bring your own key.** Your `XAI_API_KEY` lives on **one** fat node; every other surface — edge Pi, laptop, agent — reaches it over a LAN bearer token and never sees the key. Generated media is downloaded to your **local library** by default: the ephemeral xAI URL may expire, your files won't.

## Why

Some model vendors don't offer image or video generation. xAI's Imagine API does, and its surface is unusually flexible. **Imaginarium-RS** wraps that surface once, in Rust, and exposes it to everything that might want it:

- **agents** — over MCP (stdio) or the HTTP API,
- **humans** — a zero-install browser studio, or a native desktop/kiosk app,
- **a home lab** — one node holds the key + disk cache; the rest of the mesh calls it with a token.

Local-first by design, honest about failure, and small enough to run on a spare board.

## Surfaces

| Surface | For | How |
|---|---|---|
| **CLI** | scripts &amp; you | `imaginarium image gen …` / `video i2v …` |
| **MCP** | agents (Claude Code, Hermes, …) | stdio server, or a thin proxy to a fat node |
| **HTTP API** | anything on the LAN | `POST /v1/images/generations` + a bearer token |
| **Browser studio** | humans, no install | open `http://<node>:8791/`, paste a token once |
| **Native app** | desktop &amp; kiosk | Slint (`winit` + `linuxkms`) — separate GPL binary |

## Quick start

```bash
cd ~/Projects/Imaginarium-RS
cargo build -p imaginarium-cli
export XAI_API_KEY=...            # or set upstream.api_key in the config

# Images
imaginarium image gen  -p "marble amphitheater, golden hour" --model 2.0 --ar 16:9
imaginarium image edit --image ./a.png -p "noir, rain-slick streets"

# Video (blocks until done by default; add --no-wait + status/wait for long jobs)
imaginarium video gen    -p "camera orbit over a hillside amphitheater" --duration 8 --res 1080p
imaginarium video gen    -p "she speaks to camera with the voice from <AUDIO_0>" --voice eve
imaginarium video i2v    --image ./still.png -p "slow pan out" --res 1080p
imaginarium video ref    -p "the person from <IMAGE_0> talks as <AUDIO_0>" --ref ./face.png --voice eve --res 720p
imaginarium video extend --video ./clip.mp4 --duration 6 -p "continue the pan"

# Serve the studio + API (loopback by default; LAN needs an explicit bind + token)
export IMAGINARIUM_TOKEN=$(openssl rand -hex 24)
imaginarium serve --bind 127.0.0.1:8791          # → http://127.0.0.1:8791

# MCP for agents (holds the key locally)…
imaginarium mcp
# …or a thin edge proxy that forwards to a fat node (no key on the edge)
imaginarium mcp --proxy http://192.168.0.10:8791
```

Assets land in `~/.local/share/imaginarium/library/YYYY/MM/DD/<job_id>/`.

## Models

Capability matrix lives once in `imaginarium-core::models` and is consumed by CLI, API, MCP, and UI.

| Model | Modes | Max res | ~Cost |
|---|---|---|---|
| `grok-imagine-image` | text→image, edit | 2k | ~$0.02 / image |
| `grok-imagine-image-quality` | text→image, edit | 2k | ~$0.05 / image |
| `grok-imagine-image-2.0` | text→image, edit | 2k | ~$0.04 / image |
| `grok-imagine-video` | T2V · I2V · R2V · edit · extend | 720p | ~$0.05 / sec |
| `grok-imagine-video-1.5` | T2V · I2V · R2V (+ preset voices) | **1080p** (T2V/I2V; R2V 720p) | ~$0.08 / $0.14 / $0.25 per sec (480/720/1080p) |

Aliases: `image`, `quality`, `2.0` (also `image-2.0`). Image 2.0 accepts an optional `quality` of `low` or `medium` (upstream default `medium`) — CLI `--quality`, HTTP/MCP `quality`. That field is rejected on the older image models.

Pass `--model auto` (or omit it) and generate modes default to `video-1.5`. Edit/extend stay on legacy `video`. Video 1.5 accepts `--voice` / `reference_audios` (preset `voice_id`s: eve, ara, leo, rex, …; max 3; tag `<AUDIO_0>`).

## Multi-node

```
        ┌─────────────────────────────┐
        │  Imaginarium NODE (fat)     │
        │  · XAI_API_KEY              │
        │  · disk library + job DB    │
        │  · axum :8791 (API + UI)    │
        │  · MCP stdio                │
        └────────────▲────────────────┘
                     │  LAN + bearer token
        ┌────────────┼───────────┬───────────────┐
     Claude Code   Edge Pi     Browser        Slint app
      (MCP/HTTP)  (HTTP/MCP    (zero install)  (HTTP client)
                   proxy)
```

The key stays on the fat node; edge nodes and clients only ever hold a token. See [`docs/MULTI_NODE.md`](docs/MULTI_NODE.md).

## Repository layout

```
crates/
  imaginarium-core/    # client, models, config, jobs, library     (MIT/Apache)
  imaginarium-cli/     # the `imaginarium` binary                   (MIT/Apache)
  imaginarium-mcp/     # MCP stdio server (+ fat-node proxy)        (MIT/Apache)
  imaginarium-server/  # axum LAN API + embedded Vue studio         (MIT/Apache)
  imaginarium-slint/   # native winit/kms app                       (GPL-3.0)
ui-web/                # Vue 3 studio (source + committed dist/)
openapi/               # imaginarium-v1.yaml — the HTTP contract
docs/                  # architecture, multi-node, licensing, ApexOS embed
```

<details>
<summary><strong>Full HTTP surface</strong> (see <code>openapi/imaginarium-v1.yaml</code>)</summary>

`GET /health` · `GET /v1/models` · `POST /v1/estimate` ·
`POST /v1/images/{generations,edits}` ·
`POST /v1/videos/{generations,edits,extensions}` ·
`GET /v1/jobs` · `GET /v1/jobs/{id}` (polls pending video) · `POST /v1/jobs/{id}/wait` ·
`GET /v1/library/{id}/content` · `GET /v1/library/{id}/thumb` · `POST /v1/library/import` ·
`POST /v1/craft/video/render` · `GET|POST /v1/tokens` · `DELETE /v1/tokens/{id}`

Auth on every `/v1/*` route (any one): `Authorization: Bearer <token>`,
`X-Imaginarium-Token: <token>`, or `?token=<token>`. Scopes: `read` · `write` · `admin`.
</details>

## Security posture

- Default bind is loopback `127.0.0.1:8791`. A non-loopback bind **refuses to start without a token**.
- Tokens are stored as hashes; the plaintext is shown once at mint. The upstream xAI key is never returned by the API and never logged.
- Media inputs from the network accept only `data:` / `http(s):` / `file_…` / `library:{job_id}` refs — bare local paths are the CLI's privilege alone.
- Optional spend caps: `[limits] max_usd_per_job` / `max_usd_per_day` (omit or `0` = off).
- Paid-request token bucket (default **30/min**, burst **10**) per minted token, node env token, and local CLI/MCP. Set `[limits] paid_rpm = 0` to disable. HTTP 429 + `Retry-After`. Polls and craft are not counted.

See [`SECURITY.md`](SECURITY.md) for the trust model, [`BACKLOG.md`](BACKLOG.md) for the live ledger, and [`docs/audit/`](docs/audit/) for the 2026-08-17 rematch.

## Configuration

| What | Where |
|---|---|
| Config | `$IMAGINARIUM_CONFIG` or `~/.config/imaginarium/config.toml` |
| Data / library | `$IMAGINARIUM_HOME` or `~/.local/share/imaginarium/` |
| Upstream key | `XAI_API_KEY` (or `upstream.api_key` in config — discouraged) |
| Node / mesh token | `IMAGINARIUM_TOKEN`, or `imaginarium token create` |
| Spend caps | `[limits] max_usd_per_job` / `max_usd_per_day` (optional) |
| Paid rate limit | `[limits] paid_rpm` (default 30) / `paid_burst` (default 10); `0` = off |

## Landing changes

Feature branch off `master` → PR → merge. That is how #1–#8 landed; keep doing it. Don't push commits straight to `master`.

```bash
git checkout -b feat/…
# local commits at slice / milestone
git push -u origin HEAD
gh pr create
```

CI (`fmt` / `clippy` / `test` / `build`) runs on every PR. Merge when green.

## License

- Headless stack (`core`, `cli`, `mcp`, `server`, embedded web assets): **MIT OR Apache-2.0**.
- Native app (`imaginarium-slint` / `imaginarium-app`): **GPL-3.0-only**.

Shipping the headless node stays permissive; shipping the native GUI carries GPL obligations for that binary. Details in [`docs/LICENSING.md`](docs/LICENSING.md).

---

<div align="center">
<sub>xAI Imagine usage is subject to xAI's terms. Imaginarium-RS is an independent client — not affiliated with xAI.<br>
🖼️ <em>The banner above was generated by Imaginarium-RS itself, via <code>grok-imagine-image-quality</code>.</em></sub>
</div>
