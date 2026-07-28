# Imaginarium-RS

Local-first, multi-node **xAI Imagine** gateway — CLI, MCP (soon), LAN-token HTTP API (soon), Vue browser UI (soon), optional Slint native app (later).

Plan: `~/Projects/plan_drafts/imaginarium-rs.md`

## Status

| Phase | Scope | State |
|---|---|---|
| 0 | Workspace, config, models catalog | **done** |
| 1 | Image gen/edit + local library + job DB | **in progress** (client wired) |
| 2 | Video full surface | pending |
| 3 | LAN HTTP API + tokens | pending |
| 4 | MCP stdio + proxy | pending |
| 5 | Vue 3 embedded UI | pending |
| 6 | Slint winit/kms app (GPL) | pending |

## Quick start

```bash
cd ~/Projects/Imaginarium-RS
cargo build -p imaginarium-cli
./target/debug/imaginarium version
./target/debug/imaginarium models
./target/debug/imaginarium config init
./target/debug/imaginarium config show

export XAI_API_KEY=...
./target/debug/imaginarium estimate image --model quality --n 2
./target/debug/imaginarium image gen -p "a marble amphitheater at golden hour" --model quality --ar 16:9 --json
```

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
