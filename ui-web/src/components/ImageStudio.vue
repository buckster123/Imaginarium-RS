<template>
  <div class="grid">
    <section class="card">
      <h2>Image studio</h2>
      <div class="mode-row">
        <button class="tab" :class="{ active: mode === 'gen' }" @click="mode = 'gen'">Generate</button>
        <button class="tab" :class="{ active: mode === 'edit' }" @click="mode = 'edit'">Edit</button>
      </div>

      <div class="field">
        <label>Prompt</label>
        <textarea v-model="prompt" placeholder="Marble amphitheater at golden hour…" />
      </div>

      <div class="row">
        <div class="field">
          <label>Model</label>
          <select v-model="model">
            <option value="image">grok-imagine-image</option>
            <option value="quality">grok-imagine-image-quality</option>
          </select>
        </div>
        <div class="field">
          <label>n</label>
          <input v-model.number="n" type="number" min="1" max="4" />
        </div>
        <div class="field">
          <label>Aspect</label>
          <select v-model="aspect">
            <option value="">default</option>
            <option>1:1</option>
            <option>16:9</option>
            <option>9:16</option>
            <option>4:3</option>
            <option>3:4</option>
            <option>3:2</option>
            <option>2:3</option>
          </select>
        </div>
        <div class="field">
          <label>Resolution</label>
          <select v-model="resolution">
            <option value="">default</option>
            <option value="1k">1k</option>
            <option value="2k">2k</option>
          </select>
        </div>
      </div>

      <div v-if="mode === 'edit'" class="field">
        <label>Source image(s) — up to 3</label>
        <div
          class="drop"
          :class="{ drag }"
          @dragover.prevent="drag = true"
          @dragleave="drag = false"
          @drop.prevent="onDrop"
          @click="$refs.file.click()"
        >
          Drop images or click to choose
          <div v-if="files.length" class="mono">{{ files.map((f) => f.name).join(', ') }}</div>
        </div>
        <input ref="file" type="file" accept="image/*" multiple hidden @change="onPick" />
      </div>

      <p v-if="estimate" class="muted">Est. ≈ ${{ estimate.estimated_usd?.toFixed?.(4) ?? estimate.estimated_usd }} · {{ estimate.note }}</p>
      <p v-if="error" class="err">{{ error }}</p>

      <div class="row">
        <button class="btn" :disabled="busy" @click="refreshEstimate">Estimate</button>
        <button class="btn btn-primary" :disabled="busy || !canRun" @click="run">
          {{ busy ? 'Working…' : mode === 'gen' ? 'Generate' : 'Edit' }}
        </button>
      </div>
    </section>

    <section class="card preview" v-if="result">
      <h3>Result <span class="badge" :class="badgeClass(result.status)">{{ result.status }}</span></h3>
      <p class="mono muted">job {{ result.job_id }}</p>
      <div class="thumbs">
        <template v-for="(a, i) in result.assets || []" :key="i">
          <img v-if="isImage(a)" class="thumb" :src="assetSrc(a)" :alt="'asset ' + i" />
          <a v-else class="mono" :href="assetSrc(a)" target="_blank">open</a>
        </template>
      </div>
      <button class="btn" @click="$emit('done', result)">Open in Jobs</button>
    </section>
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { api, fileToDataUrl, getToken } from '../api'

const emit = defineEmits(['done'])
const mode = ref('gen')
const prompt = ref('')
const model = ref('quality')
const n = ref(1)
const aspect = ref('16:9')
const resolution = ref('')
const files = ref([])
const drag = ref(false)
const busy = ref(false)
const error = ref('')
const result = ref(null)
const estimate = ref(null)

const canRun = computed(() => {
  if (!prompt.value.trim()) return false
  if (mode.value === 'edit' && !files.value.length) return false
  return true
})

watch([model, n], () => refreshEstimate())

async function refreshEstimate() {
  try {
    estimate.value = await api.estimate({ kind: 'image', model: model.value, n: n.value })
  } catch {
    estimate.value = null
  }
}

function onPick(e) {
  files.value = [...(e.target.files || [])].slice(0, 3)
}
function onDrop(e) {
  drag.value = false
  files.value = [...(e.dataTransfer.files || [])].filter((f) => f.type.startsWith('image/')).slice(0, 3)
}

function isImage(a) {
  const p = a.local_path || a.url || ''
  return /\.(png|jpe?g|webp|gif)$/i.test(p) || a.kind === 'image'
}
function assetSrc(a) {
  if (a.local_path && result.value?.job_id) {
    return api.libraryContentUrl(result.value.job_id) + authQ()
  }
  return a.url || a.public_url || ''
}
function authQ() {
  const t = getToken()
  return t ? `?token=${encodeURIComponent(t)}` : ''
}
function badgeClass(s) {
  if (s === 'completed' || s === 'done') return 'done'
  if (s === 'failed' || s === 'error') return 'fail'
  return 'run'
}

async function run() {
  error.value = ''
  busy.value = true
  result.value = null
  try {
    if (mode.value === 'gen') {
      result.value = await api.imageGen({
        prompt: prompt.value,
        model: model.value,
        n: n.value,
        aspect_ratio: aspect.value || undefined,
        resolution: resolution.value || undefined,
      })
    } else {
      const images = []
      for (const f of files.value) images.push(await fileToDataUrl(f))
      result.value = await api.imageEdit({
        prompt: prompt.value,
        images,
        model: model.value,
        n: n.value,
        aspect_ratio: aspect.value || undefined,
        resolution: resolution.value || undefined,
      })
    }
  } catch (e) {
    error.value = e.message || String(e)
  } finally {
    busy.value = false
  }
}

refreshEstimate()
</script>

<style scoped>
.grid { display: grid; gap: 1rem; grid-template-columns: 1.2fr 1fr; }
@media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }
h2, h3 { margin: 0 0 0.75rem; }
.mode-row { display: flex; gap: 0.35rem; margin-bottom: 1rem; }
.tab {
  background: transparent; border: 1px solid var(--border); color: var(--muted);
  border-radius: 999px; padding: 0.35rem 0.85rem; cursor: pointer;
}
.tab.active { color: var(--gold); border-color: var(--gold-dim); }
.thumbs { display: grid; gap: 0.5rem; margin: 0.75rem 0; }
.preview { align-self: start; }
</style>
