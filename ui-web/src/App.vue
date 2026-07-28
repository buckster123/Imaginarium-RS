<template>
  <div class="shell">
    <header class="top">
      <div class="brand">
        <span class="mark">🏛️</span>
        <div>
          <h1>Imaginarium</h1>
          <p class="tag">local-first · Grok Imagine studio</p>
        </div>
      </div>
      <nav v-if="authed" class="tabs">
        <button
          v-for="t in tabs"
          :key="t.id"
          class="tab"
          :class="{ active: tab === t.id }"
          @click="tab = t.id"
        >
          {{ t.label }}
        </button>
      </nav>
      <div class="top-right" v-if="authed">
        <span v-if="globalBusy" class="badge run">working… {{ busyLabel }}</span>
        <span class="badge mono" title="Node health">{{ healthLabel }}</span>
        <button class="btn btn-ghost" @click="logout">Lock</button>
      </div>
    </header>

    <main class="main">
      <TokenGate v-if="!authed" @unlocked="onUnlock" />
      <template v-else>
        <ImageStudio
          v-show="tab === 'image'"
          ref="imageRef"
          :chain-payload="imageChain"
          @done="onJob"
          @busy="onBusy"
          @chain-consumed="imageChain = null"
        />
        <VideoStudio
          v-show="tab === 'video'"
          ref="videoRef"
          :chain-payload="videoChain"
          @done="onJob"
          @busy="onBusy"
          @chain-consumed="videoChain = null"
        />
        <JobBoard v-if="tab === 'jobs'" :flash="flashJob" @select="selectedJob = $event" />
        <LibraryView v-else-if="tab === 'library'" />
        <SettingsView v-else-if="tab === 'settings'" />
      </template>
    </main>

    <footer class="foot muted">
      Imaginarium-RS · BYOK xAI · ApexOS-compatible LAN tokens · Studio+ craft coming · no cloud UI
    </footer>
    <ToastHost />
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue'
import { api, clearToken, getToken } from './api'
import { toastOk, toastErr } from './toast'
import TokenGate from './components/TokenGate.vue'
import ImageStudio from './components/ImageStudio.vue'
import VideoStudio from './components/VideoStudio.vue'
import JobBoard from './components/JobBoard.vue'
import LibraryView from './components/LibraryView.vue'
import SettingsView from './components/SettingsView.vue'
import ToastHost from './components/ToastHost.vue'

const tabs = [
  { id: 'image', label: 'Image' },
  { id: 'video', label: 'Video' },
  { id: 'jobs', label: 'Jobs' },
  { id: 'library', label: 'Library' },
  { id: 'settings', label: 'Settings' },
]

const authed = ref(false)
const tab = ref('image')
const healthLabel = ref('…')
const flashJob = ref(null)
const selectedJob = ref(null)
const globalBusy = ref(false)
const busyLabel = ref('')
const imageChain = ref(null)
const videoChain = ref(null)
const imageRef = ref(null)
const videoRef = ref(null)

async function probe() {
  try {
    const h = await api.health()
    healthLabel.value = `${h.product || 'ok'} v${h.version || '?'}`
  } catch {
    healthLabel.value = 'offline'
  }
}

async function tryAuth() {
  if (!getToken()) {
    try {
      await api.models()
      authed.value = true
      return
    } catch {
      authed.value = false
      return
    }
  }
  try {
    await api.models()
    authed.value = true
  } catch (e) {
    if (e.status === 401) {
      clearToken()
      authed.value = false
      toastErr('Token rejected — unlock again')
    } else {
      authed.value = true
    }
  }
}

function onUnlock() {
  authed.value = true
  toastOk('Studio unlocked')
  probe()
}

function logout() {
  clearToken()
  authed.value = false
  toastOk('Locked')
}

function onJob(job) {
  flashJob.value = job
  toastOk(`Job ${job.status || 'done'}: ${String(job.job_id || '').slice(0, 12)}…`)
}

function onBusy({ busy, label }) {
  globalBusy.value = !!busy
  busyLabel.value = label || ''
}

function handleChain(payload) {
  const { action, result } = payload
  if (action === 'i2v' || action === 'extend' || action === 'video-edit') {
    videoChain.value = { action, result }
    tab.value = 'video'
    toastOk(
      action === 'i2v' ? 'Loaded still → Video I2V' : action === 'extend' ? 'Loaded clip → Extend' : 'Loaded clip → AI edit'
    )
  } else if (action === 'image-edit') {
    imageChain.value = { action, result }
    tab.value = 'image'
    toastOk('Loaded still → Image AI edit')
  }
}

function onKey(e) {
  if (e.key === 'Escape') {
    // toasts self-dismiss; Esc focuses nothing special
  }
}

// expose chain handler to children via provide would be cleaner — children emit chain to parent
// Image/Video emit 'chain' to App — wire below by listening... actually ChainBar is inside studios
// Studios will emit 'chain' up

onMounted(async () => {
  window.addEventListener('keydown', onKey)
  // listen for chain from studios via custom event bus-lite
  window.addEventListener('imaginarium-chain', (ev) => handleChain(ev.detail))
  window.addEventListener('imaginarium-to-jobs', (ev) => {
    flashJob.value = ev.detail
    tab.value = 'jobs'
  })
  await probe()
  await tryAuth()
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
})
</script>

<style scoped>
.shell {
  min-height: 100%;
  display: flex;
  flex-direction: column;
  max-width: 1200px;
  margin: 0 auto;
  padding: 1rem 1.25rem 2rem;
}
.top {
  display: flex;
  align-items: center;
  gap: 1.25rem;
  flex-wrap: wrap;
  margin-bottom: 1.25rem;
  padding-bottom: 1rem;
  border-bottom: 1px solid var(--border);
}
.brand {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}
.brand h1 {
  margin: 0;
  font-size: 1.35rem;
  font-weight: 650;
  letter-spacing: 0.02em;
}
.mark {
  font-size: 1.8rem;
  line-height: 1;
}
.tag {
  margin: 0.1rem 0 0;
  font-size: 0.78rem;
  color: var(--muted);
}
.tabs {
  display: flex;
  gap: 0.35rem;
  flex: 1;
  flex-wrap: wrap;
}
.tab {
  background: transparent;
  border: 1px solid transparent;
  color: var(--muted);
  padding: 0.45rem 0.85rem;
  border-radius: 999px;
  cursor: pointer;
}
.tab:hover {
  color: var(--text);
}
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
  background: rgba(212, 168, 75, 0.08);
}
.top-right {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-left: auto;
}
.main {
  flex: 1;
}
.foot {
  margin-top: 2rem;
  font-size: 0.75rem;
  text-align: center;
}
</style>
