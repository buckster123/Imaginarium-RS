<template>
  <section class="card">
    <div class="head">
      <h2>Library</h2>
      <button class="btn" @click="load">Refresh jobs</button>
    </div>
    <p class="muted">
      Assets live on the fat node disk. Preview uses
      <span class="mono">/v1/library/{job_id}/content</span>.
    </p>
    <p v-if="error" class="err">{{ error }}</p>
    <div class="grid">
      <button
        v-for="j in items"
        :key="j.job_id"
        class="tile"
        @click="select(j)"
      >
        <div class="mono">{{ short(j.job_id) }}</div>
        <div class="muted">{{ j.mode }} · {{ j.status }}</div>
      </button>
    </div>
    <div v-if="selected" class="preview">
      <h3 class="mono">{{ selected.job_id }}</h3>
      <video v-if="looksVideo(selected)" class="thumb" controls :src="src(selected)" />
      <img v-else class="thumb" :src="src(selected)" alt="" @error="imgErr = true" />
      <p v-if="imgErr" class="muted">No previewable asset for this job (or still pending).</p>
      <a class="btn" :href="src(selected)" target="_blank" rel="noopener">Open content URL</a>
    </div>
  </section>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api, getToken } from '../api'

const items = ref([])
const selected = ref(null)
const error = ref('')
const imgErr = ref(false)

function short(id) {
  return id?.length > 14 ? id.slice(0, 12) + '…' : id
}
function src(j) {
  const t = getToken()
  return api.libraryContentUrl(j.job_id) + (t ? `?token=${encodeURIComponent(t)}` : '')
}
function looksVideo(j) {
  return String(j.mode || '').includes('video') || String(j.mode || '').includes('t2v')
}
function select(j) {
  imgErr.value = false
  selected.value = j
}

async function load() {
  error.value = ''
  try {
    const data = await api.jobs(50)
    items.value = Array.isArray(data) ? data : data.jobs || []
  } catch (e) {
    error.value = e.message
  }
}

onMounted(load)
</script>

<style scoped>
.head { display: flex; justify-content: space-between; align-items: center; }
h2 { margin: 0; }
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 0.6rem;
  margin: 1rem 0;
}
.tile {
  text-align: left;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 0.75rem;
  cursor: pointer;
  color: inherit;
}
.tile:hover { border-color: var(--gold-dim); }
.preview { margin-top: 1rem; }
</style>
