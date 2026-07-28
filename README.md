# Imaginarium-RS

Local-first, multi-node **xAI Imagine** gateway — CLI, MCP (soon), LAN-token HTTP API (soon), Vue browser UI (soon), optional Slint native app (later).

Plan: `~/Projects/plan_drafts/imaginarium-rs.md`

## Status

| Phase | Scope | State |
|---|---|---|
| 0 | Workspace, config, models catalog | **done** |
| 1 | Image gen/edit + local library + job DB | **done** |
| 2 | Video full surface | **done** |
| 3 | LAN HTTP API + tokens | **done** |
| 4 | MCP stdio + proxy | **done** |
| 5 | Vue 3 embedded UI | pending |
| 6 | Slint winit/kms app (GPL) | pending |

## Quick start

```bash
cd ~/Projects/Imaginarium-RS
cargo build -p imaginarium-cli
export XAI_API_KEY=...

./target/debug/imaginarium models
./target/debug/imaginarium image gen -p "marble amphitheater golden hour" --model quality --ar 16:9

# Video (blocks until done by default; use --no-wait + status/wait)
./target/debug/imaginarium video gen -p "camera orbit over a hillside amphitheater" --duration 8 --res 720p
./target/debug/imaginarium video i2v --image ./still.png -p "slow pan out" --res 1080p
./target/debug/imaginarium video ref -p "model walks the runway" --ref a.png --ref b.png
./target/debug/imaginarium video edit --video ./clip.mp4 -p "add golden hour light"
./target/debug/imaginarium video extend --video ./clip.mp4 --duration 6 -p "continue the pan"
./target/debug/imaginarium video status <job_id>
./target/debug/imaginarium video wait <job_id>
```

Assets land in `~/.local/share/imaginarium/library/YYYY/MM/DD/<job_id>/`.

## Layout

```
crates/
  imaginarium-core/     # client, models, config, jobs, library (MIT/Apache)
  imaginarium-cli/      # `imaginarium` binary
  imaginarium-mcp/      # Phase 4
  imaginarium-server/   # Phase 3/5
  imaginarium-slint/    # Phase 6 GPL app (not in default workspace)
ui-web/                 # Vue 3 (Phase 5)
docs/
openapi/
```

## Config

- Config: `$IMAGINARIUM_CONFIG` or `~/.config/imaginarium/config.toml`
- Data: `$IMAGINARIUM_HOME` or `~/.local/share/imaginarium/`
- Upstream key: `XAI_API_KEY` (or `upstream.api_key` in config — discouraged)

Default bind for future serve: `127.0.0.1:8791`

## License

- Headless stack (core, cli, mcp, server): **MIT OR Apache-2.0**
- `imaginarium-slint` / `imaginarium-app`: **GPL-3.0-only** (see `docs/LICENSING.md`)
