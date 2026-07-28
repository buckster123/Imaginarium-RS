# Agent integration

## Hermes

Local (node holds `XAI_API_KEY`):

```yaml
mcp_servers:
  imaginarium:
    command: imaginarium
    args: [mcp]
    # or: command: imaginarium-mcp
    env:
      XAI_API_KEY: ${XAI_API_KEY}
      # optional paths
      # IMAGINARIUM_HOME: /home/andre/.local/share/imaginarium
    # Video poll can take minutes — raise tool timeout
    # timeout / tool timeout: 600+
```

Edge thin client → fat node (Phase 3 API):

```yaml
mcp_servers:
  imaginarium:
    command: imaginarium
    args: [mcp, --proxy, "http://192.168.0.10:8791"]
    env:
      IMAGINARIUM_TOKEN: ${IMAGINARIUM_TOKEN}
```

Equivalent env-only proxy:

```bash
export IMAGINARIUM_URL=http://fat:8791
export IMAGINARIUM_TOKEN=...
imaginarium mcp
```

## Claude Code / Cursor-style MCP JSON

```json
{
  "mcpServers": {
    "imaginarium": {
      "command": "imaginarium",
      "args": ["mcp"],
      "env": {
        "XAI_API_KEY": "..."
      }
    }
  }
}
```

## ApexOS plugins.toml

```toml
[[plugin]]
id      = "imaginarium"
cmd     = "imaginarium"
args    = ["mcp"]
restart = "always"
# env via agentd env file: XAI_API_KEY=...
```

Or fat-node proxy from edge:

```toml
[[plugin]]
id   = "imaginarium"
cmd  = "imaginarium"
args = ["mcp", "--proxy", "http://fat:8791"]
# IMAGINARIUM_TOKEN in env
```

## Tools (10)

| Tool | Purpose |
|---|---|
| `imaginarium_models` | capability matrix |
| `imaginarium_estimate` | rough USD before spend |
| `imaginarium_image_generate` | T2I |
| `imaginarium_image_edit` | edit 1–3 images |
| `imaginarium_video_generate` | T2V / I2V / R2V |
| `imaginarium_video_edit` | video edit |
| `imaginarium_video_extend` | extend clip |
| `imaginarium_job_status` | one-shot poll |
| `imaginarium_job_wait` | block until done |
| `imaginarium_jobs_list` | recent jobs |

**Agent tip:** for long video, prefer `no_wait=true` then `imaginarium_job_status` / `imaginarium_job_wait` so host timeouts don't kill the call mid-gen. Never dump giant base64 into chat — paths/URLs only.

## Wire format

Newline-delimited JSON-RPC 2.0 over stdio, protocol `2024-11-05` (same as Cerebro-RS / agentd). Logs on **stderr** only.
