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
        <span class="badge mono" title="Node health">{{ healthLabel }}</span>
        <button class="btn btn-ghost" @click="logout">Lock</button>
      </div>
    </header>

    <main class="main">
      <TokenGate v-if="!authed" @unlocked="onUnlock" />
      <template v-else>
        <ImageStudio v-if="tab === 'image'" @done="onJob" />
        <VideoStudio v-else-if="tab === 'video'" @done="onJob" />
        <JobBoard v-else-if="tab === 'jobs'" :flash="flashJob" @select="selectedJob = $event" />
        <LibraryView v-else-if="tab === 'library'" />
        <SettingsView v-else-if="tab === 'settings'" />
      </template>
    </main>

    <footer class="foot muted">
      Imaginarium-RS · BYOK xAI · ApexOS-compatible LAN tokens · no cloud UI
    </footer>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api, clearToken, getToken } from './api'
import TokenGate from './components/TokenGate.vue'
import ImageStudio from './components/ImageStudio.vue'
import VideoStudio from './components/VideoStudio.vue'
import JobBoard from './components/JobBoard.vue'
import LibraryView from './components/LibraryView.vue'
import SettingsView from './components/SettingsView.vue'

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
    // allow localhost no-auth servers: try models without token
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
    } else {
      // server up but other error — still let in if we have token
      authed.value = true
    }
  }
}

function onUnlock() {
  authed.value = true
  probe()
}

function logout() {
  clearToken()
  authed.value = false
}

function onJob(job) {
  flashJob.value = job
  tab.value = 'jobs'
}

onMounted(async () => {
  await probe()
  await tryAuth()
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
.brand { display: flex; gap: 0.75rem; align-items: center; }
.brand h1 {
  margin: 0;
  font-size: 1.35rem;
  font-weight: 650;
  letter-spacing: 0.02em;
}
.mark { font-size: 1.8rem; line-height: 1; }
.tag { margin: 0.1rem 0 0; font-size: 0.78rem; color: var(--muted); }
.tabs { display: flex; gap: 0.35rem; flex: 1; flex-wrap: wrap; }
.tab {
  background: transparent;
  border: 1px solid transparent;
  color: var(--muted);
  padding: 0.45rem 0.85rem;
  border-radius: 999px;
  cursor: pointer;
}
.tab:hover { color: var(--text); }
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
  background: rgba(212, 168, 75, 0.08);
}
.top-right { display: flex; align-items: center; gap: 0.5rem; margin-left: auto; }
.main { flex: 1; }
.foot {
  margin-top: 2rem;
  font-size: 0.75rem;
  text-align: center;
}
</style>
