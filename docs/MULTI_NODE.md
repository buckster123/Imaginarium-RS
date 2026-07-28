# Multi-node LAN auth (ApexOS-compatible)

Imaginarium Phase 3 mirrors **ApexOS-RS agentd** network auth so mesh nodes can talk the same way.

## Patterns (from ApexOS SECURITY / gateway)

| Pattern | ApexOS | Imaginarium |
|---|---|---|
| Shared node secret | `AGENTD_TOKEN` | `IMAGINARIUM_TOKEN` |
| Bearer header | `Authorization: Bearer …` | same |
| Query token (browsers) | `?token=` | same |
| Extra header | — | `X-Imaginarium-Token` |
| Non-loopback gate | refuse start without token | same |
| Constant-time compare | yes | yes (`ct_eq`) |
| Token list redaction | `has_token` | hash never returned |
| Plaintext LAN | intentional | intentional — use VPN if remote |

## Quick start

```bash
# On fat node
export XAI_API_KEY=...
export IMAGINARIUM_TOKEN=$(openssl rand -hex 24)   # node admin secret

imaginarium token create --label hermes --scope write
# → prints img_… once

imaginarium serve --bind 0.0.0.0:8791
# refuses without IMAGINARIUM_TOKEN or minted tokens
```

```bash
# Edge / ApexOS peer / curl
curl -s http://fat:8791/health
curl -s -H "Authorization: Bearer $IMAGINARIUM_TOKEN" http://fat:8791/v1/models
curl -s "http://fat:8791/v1/models?token=$IMAGINARIUM_TOKEN"

curl -s -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"marble amphitheater","model":"quality"}' \
  http://fat:8791/v1/images/generations
```

## Scopes

| Scope | Can |
|---|---|
| `admin` | mint/revoke tokens + everything |
| `write` | image/video jobs + read |
| `read` | models, jobs, library content |

Node env token is always **admin**.

## CLI

```bash
imaginarium token create --label edge-pi --scope write
imaginarium token ls
imaginarium token revoke <id>
imaginarium serve --bind 127.0.0.1:8791 --allow-localhost-no-auth  # dev only
```

## Storing peer credentials (ApexOS style)

In ApexOS `peers.toml`, each peer carries `token` (the peer's `AGENTD_TOKEN`). For Imaginarium, store the fat node's `IMAGINARIUM_TOKEN` (or a minted write token) the same way on the edge:

```toml
# conceptual — your mesh registry
[[peer]]
node_id = "imaginarium-fat"
base_url = "http://192.168.0.10:8791"
token = "<IMAGINARIUM_TOKEN or img_…>"
```

Then call with `Authorization: Bearer <token>` exactly as `send_to_agent` does for agentd.
