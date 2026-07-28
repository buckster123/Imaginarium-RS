# Imaginarium browser studio (Vue 3 + Vite)

Zero-install client: assets are built here and **embedded** into `imaginarium serve`
via `rust-embed`. Operators only need a browser + LAN token.

## Dev (hot reload against local API)

```bash
# terminal A
export XAI_API_KEY=...
export IMAGINARIUM_TOKEN=...
imaginarium serve --bind 127.0.0.1:8791 --allow-localhost-no-auth

# terminal B
cd ui-web && npm install && npm run dev
# → http://127.0.0.1:5179  (proxies /v1 → :8791)
```

## Production build (required before release cargo build)

```bash
cd ui-web && npm ci && npm run build
# writes ui-web/dist/**  → rust-embed path
cargo build -p imaginarium-cli --release
```

Open `http://127.0.0.1:8791/` after `imaginarium serve`.
