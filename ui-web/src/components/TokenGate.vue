<template>
  <div class="gate card">
    <h2>Unlock studio</h2>
    <p class="muted">
      Paste a LAN bearer token from the fat node
      (<span class="mono">imaginarium token create</span> or
      <span class="mono">IMAGINARIUM_TOKEN</span>).
      Stored in <span class="mono">sessionStorage</span> only.
    </p>
    <div class="field">
      <label>Token</label>
      <input
        v-model="token"
        type="password"
        autocomplete="off"
        placeholder="img_… or node secret"
        @keydown.enter="submit"
      />
    </div>
    <p v-if="error" class="err">{{ error }}</p>
    <div class="row">
      <button class="btn btn-primary" :disabled="busy" @click="submit">
        {{ busy ? 'Checking…' : 'Enter' }}
      </button>
      <button class="btn" :disabled="busy" @click="tryOpen">Try without token</button>
    </div>
    <p class="muted tip">Health: {{ health }}</p>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api, setToken } from '../api'

const emit = defineEmits(['unlocked'])
const token = ref('')
const error = ref('')
const busy = ref(false)
const health = ref('…')

onMounted(async () => {
  try {
    const h = await api.health()
    health.value = `${h.product} ${h.version} · ok`
  } catch (e) {
    health.value = `unreachable (${e.message})`
  }
})

async function submit() {
  error.value = ''
  if (!token.value.trim()) {
    error.value = 'Token required (or try without on loopback no-auth)'
    return
  }
  busy.value = true
  try {
    setToken(token.value.trim())
    await api.models()
    emit('unlocked')
  } catch (e) {
    setToken('')
    error.value = e.message || String(e)
  } finally {
    busy.value = false
  }
}

async function tryOpen() {
  error.value = ''
  busy.value = true
  try {
    setToken('')
    await api.models()
    emit('unlocked')
  } catch (e) {
    error.value = e.message || 'Auth required on this node'
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.gate { max-width: 480px; margin: 3rem auto; }
.gate h2 { margin: 0 0 0.5rem; color: var(--gold); }
.tip { margin-top: 1.25rem; font-size: 0.85rem; }
</style>
