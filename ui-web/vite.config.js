import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// Built assets land in dist/ and are rust-embed'd by imaginarium-server.
export default defineConfig({
  plugins: [vue()],
  base: '/',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsDir: 'assets',
  },
  server: {
    port: 5179,
    proxy: {
      '/v1': 'http://127.0.0.1:8791',
      '/health': 'http://127.0.0.1:8791',
    },
  },
})
