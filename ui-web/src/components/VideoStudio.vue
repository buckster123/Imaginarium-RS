<template>
  <div class="grid">
    <section class="card">
      <h2>Video studio</h2>
      <div class="mode-row">
        <button v-for="m in modes" :key="m.id" class="tab" :class="{ active: mode === m.id }" @click="mode = m.id">
          {{ m.label }}
        </button>
      </div>

      <div class="field">
        <label>Prompt</label>
        <textarea v-model="prompt" placeholder="Slow orbit over hillside amphitheater…" />
      </div>

      <div class="row">
        <div class="field">
          <label>Model</label>
          <select v-model="model">
            <option value="">auto</option>
            <option value="video">grok-imagine-video</option>
            <option value="1.5">grok-imagine-video-1.5</option>
          </select>
        </div>
        <div class="field" v-if="mode === 't2v' || mode === 'i2v' || mode === 'r2v'">
          <label>Duration (s)</label>
          <input v-model.number="duration" type="number" min="1" max="15" />
        </div>
        <div class="field" v-if="mode === 'extend'">
          <label>Extend (s)</label>
          <input v-model.number="extDuration" type="number" min="2" max="10" />
        </div>
        <div class="field">
          <label>Aspect</label>
          <select v-model="aspect">
            <option value="">default</option>
            <option>16:9</option>
            <option>9:16</option>
            <option>1:1</option>
          </select>
        </div>
        <div class="field">
          <label>Resolution</label>
          <select v-model="resolution">
            <option value="720p">720p</option>
            <option value="480p">480p</option>
            <option value="1080p">1080p (1.5 I2V)</option>
          </select>
        </div>
      </div>

      <div v-if="mode === 'i2v' || mode === 'edit' || mode === 'extend'" class="field">
        <label>{{ mode === 'i2v' ? 'Start image' : 'Source video' }}</label>
        <div class="drop" :class="{ drag }" @dragover.prevent="drag = true" @dragleave="drag = false" @drop.prevent="onDropOne" @click="$refs.one.click()">
          {{ oneFile ? oneFile.name : 'Drop or choose file' }}
        </div>
        <input ref="one" type="file" :accept="mode === 'i2v' ? 'image/*' : 'video/*'" hidden @change="onPickOne" />
      </div>

      <div v-if="mode === 'r2v'" class="field">
        <label>Reference images</label>
        <div class="drop" :class="{ drag }" @dragover.prevent="drag = true" @dragleave="drag = false" @drop.prevent="onDropRefs" @click="$refs.refs.click()">
          {{ refFiles.length ? refFiles.map((f) => f.name).join(', ') : 'Drop refs' }}
        </div>
        <input ref="refs" type="file" accept="image/*" multiple hidden @change="onPickRefs" />
      </div>

      <label class="check">
        <input type="checkbox" v-model="noWait" /> Submit only (no_wait) — poll from Jobs
      </label>

      <p v-if="estimate" class="muted">
        Est. ≈ ${{ Number(estimate.estimated_usd).toFixed(4) }} · {{ estimate.model }} · {{ estimate.note }}
      </p>
      <p v-if="error" class="err">{{ error }}</p>

      <div class="row">
        <button class="btn" :disabled="busy" @click="refreshEstimate">Estimate</button>
        <button class="btn btn-primary" :disabled="busy || !canRun" @click="run">
          {{ busy ? 'Working…' : 'Run' }}
        </button>
      </div>
    </section>

    <section class="card" v-if="result">
      <h3>Result <span class="badge" :class="badgeClass(result.status)">{{ result.status }}</span></h3>
      <p class="mono muted">job {{ result.job_id }}</p>
      <video
        v-if="videoSrc"
        class="thumb"
        controls
        :src="videoSrc"
      />
      <button class="btn" @click="$emit('done', result)">Open in Jobs</button>
    </section>
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { api, fileToDataUrl, getToken } from '../api'

const emit = defineEmits(['done'])
const modes = [
  { id: 't2v', label: 'T2V' },
  { id: 'i2v', label: 'I2V' },
  { id: 'r2v', label: 'R2V' },
  { id: 'edit', label: 'Edit' },
  { id: 'extend', label: 'Extend' },
]
const mode = ref('t2v')
const prompt = ref('')
const model = ref('')
const duration = ref(6)
const extDuration = ref(6)
const aspect = ref('16:9')
const resolution = ref('720p')
const noWait = ref(false)
const oneFile = ref(null)
const refFiles = ref([])
const drag = ref(false)
const busy = ref(false)
const error = ref('')
const result = ref(null)
const estimate = ref(null)

const canRun = computed(() => {
  if (mode.value === 'edit' || mode.value === 'extend') return !!oneFile.value && !!prompt.value.trim()
  if (mode.value === 'i2v') return !!oneFile.value
  if (mode.value === 'r2v') return refFiles.value.length > 0
  return !!prompt.value.trim()
})

const videoSrc = computed(() => {
  if (!result.value) return ''
  const a = (result.value.assets || [])[0]
  if (!a) return ''
  if (result.value.job_id) {
    const t = getToken()
    return api.libraryContentUrl(result.value.job_id) + (t ? `?token=${encodeURIComponent(t)}` : '')
  }
  return a.url || a.public_url || ''
})

watch([mode, duration, model], () => refreshEstimate())

async function refreshEstimate() {
  try {
    const m = model.value || (mode.value === 'i2v' ? '1.5' : 'video')
    const d = mode.value === 'extend' ? extDuration.value : duration.value
    estimate.value = await api.estimate({ kind: 'video', model: m, duration: d })
  } catch {
    estimate.value = null
  }
}

function onPickOne(e) {
  oneFile.value = e.target.files?.[0] || null
}
function onDropOne(e) {
  drag.value = false
  oneFile.value = e.dataTransfer.files?.[0] || null
}
function onPickRefs(e) {
  refFiles.value = [...(e.target.files || [])]
}
function onDropRefs(e) {
  drag.value = false
  refFiles.value = [...(e.dataTransfer.files || [])].filter((f) => f.type.startsWith('image/'))
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
    const bodyBase = {
      prompt: prompt.value || undefined,
      model: model.value || undefined,
      no_wait: noWait.value,
      aspect_ratio: aspect.value || undefined,
      resolution: resolution.value || undefined,
    }
    if (mode.value === 't2v') {
      result.value = await api.videoGen({ ...bodyBase, duration: duration.value })
    } else if (mode.value === 'i2v') {
      const image = await fileToDataUrl(oneFile.value)
      result.value = await api.videoGen({ ...bodyBase, image, duration: duration.value })
    } else if (mode.value === 'r2v') {
      const reference_images = []
      for (const f of refFiles.value) reference_images.push(await fileToDataUrl(f))
      result.value = await api.videoGen({ ...bodyBase, reference_images, duration: duration.value })
    } else if (mode.value === 'edit') {
      const video = await fileToDataUrl(oneFile.value)
      result.value = await api.videoEdit({
        prompt: prompt.value,
        video,
        model: model.value || undefined,
        no_wait: noWait.value,
      })
    } else {
      const video = await fileToDataUrl(oneFile.value)
      result.value = await api.videoExtend({
        prompt: prompt.value,
        video,
        duration: extDuration.value,
        model: model.value || undefined,
        no_wait: noWait.value,
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
.mode-row { display: flex; gap: 0.35rem; margin-bottom: 1rem; flex-wrap: wrap; }
.tab {
  background: transparent; border: 1px solid var(--border); color: var(--muted);
  border-radius: 999px; padding: 0.35rem 0.75rem; cursor: pointer;
}
.tab.active { color: var(--gold); border-color: var(--gold-dim); }
.check { display: flex; gap: 0.5rem; align-items: center; margin: 0.5rem 0 1rem; color: var(--muted); font-size: 0.9rem; }
</style>
