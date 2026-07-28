<template>
  <section class="card">
    <h2>Settings</h2>
    <p class="muted">Admin-scoped token required to mint/revoke. Never displays the upstream xAI key.</p>

    <div class="field">
      <label>Session token (sessionStorage)</label>
      <input v-model="tokenDraft" type="password" autocomplete="off" />
    </div>
    <div class="row">
      <button class="btn" @click="saveToken">Save token</button>
      <button class="btn btn-ghost" @click="clear">Clear</button>
    </div>

    <hr />

    <h3>Mint LAN token</h3>
    <div class="row">
      <div class="field">
        <label>Label</label>
        <input v-model="label" placeholder="hermes-edge" />
      </div>
      <div class="field">
        <label>Scope</label>
        <select v-model="scope">
          <option value="read">read</option>
          <option value="write">write</option>
          <option value="admin">admin</option>
        </select>
      </div>
    </div>
    <button class="btn btn-primary" :disabled="busy" @click="mint">Create</button>
    <p v-if="minted" class="ok">
      Copy now (shown once): <span class="mono">{{ minted.token }}</span>
    </p>

    <h3>Minted tokens</h3>
    <button class="btn" @click="loadTokens">Refresh list</button>
    <ul>
      <li v-for="t in tokens" :key="t.id" class="tok">
        <span class="mono">{{ t.id }}</span>
        <span class="badge">{{ t.scope }}</span>
        {{ t.label }}
        <button class="btn btn-ghost" @click="revoke(t.id)">Revoke</button>
      </li>
    </ul>
    <p v-if="error" class="err">{{ error }}</p>

    <hr />
    <h3>Models</h3>
    <pre class="mono" v-if="models">{{ pretty(models) }}</pre>
    <button class="btn" @click="loadModels">Load catalog</button>
  </section>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api, getToken, setToken, clearToken } from '../api'

const tokenDraft = ref(getToken())
const label = ref('studio')
const scope = ref('write')
const tokens = ref([])
const minted = ref(null)
const models = ref(null)
const error = ref('')
const busy = ref(false)

function pretty(o) {
  return JSON.stringify(o, null, 2)
}
function saveToken() {
  setToken(tokenDraft.value.trim())
}
function clear() {
  clearToken()
  tokenDraft.value = ''
}

async function loadTokens() {
  error.value = ''
  try {
    const data = await api.tokensList()
    tokens.value = data.tokens || data || []
  } catch (e) {
    error.value = e.message
  }
}

async function mint() {
  busy.value = true
  error.value = ''
  minted.value = null
  try {
    minted.value = await api.tokenCreate({ label: label.value, scope: scope.value })
    await loadTokens()
  } catch (e) {
    error.value = e.message
  } finally {
    busy.value = false
  }
}

async function revoke(id) {
  error.value = ''
  try {
    await api.tokenRevoke(id)
    await loadTokens()
  } catch (e) {
    error.value = e.message
  }
}

async function loadModels() {
  try {
    models.value = await api.models()
  } catch (e) {
    error.value = e.message
  }
}

onMounted(() => {
  loadTokens().catch(() => {})
})
</script>

<style scoped>
h2, h3 { margin: 0 0 0.75rem; }
hr { border: none; border-top: 1px solid var(--border); margin: 1.25rem 0; }
.tok {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  align-items: center;
  padding: 0.4rem 0;
  border-bottom: 1px solid var(--border);
  list-style: none;
}
ul { padding: 0; margin: 0.75rem 0; }
pre {
  background: var(--bg);
  padding: 0.75rem;
  border-radius: 8px;
  max-height: 320px;
  overflow: auto;
  font-size: 0.78rem;
}
</style>
