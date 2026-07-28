# Agent integration (draft)

## Hermes (when MCP ships — Phase 4)

```yaml
mcp_servers:
  imaginarium:
    command: imaginarium
    args: [mcp]
    env:
      XAI_API_KEY: ${XAI_API_KEY}
      # or remote fat node:
      # IMAGINARIUM_URL: http://192.168.0.10:8791
      # IMAGINARIUM_TOKEN: ${IMAGINARIUM_TOKEN}
    timeout: 600
```

Until Phase 4, agents can shell out:

```bash
imaginarium image gen -p "..." --json
imaginarium models --json
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

## Edge thin client (Phase 3+)

```bash
export IMAGINARIUM_URL=http://fat-node:8791
export IMAGINARIUM_TOKEN=...
imaginarium image gen -p "..." --json
```
