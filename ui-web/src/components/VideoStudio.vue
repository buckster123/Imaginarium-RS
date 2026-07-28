<template>
  <div class="grid">
    <section class="card">
      <h2>Video studio</h2>
      <div class="mode-row">
        <button
          v-for="m in modes"
          :key="m.id"
          class="tab"
          :class="{ active: mode === m.id }"
          @click="mode = m.id"
        >
          {{ m.label }}
        </button>
      </div>

      <div class="field">
        <label>Prompt</label>
        <textarea
          v-model="prompt"
          placeholder="Slow orbit over hillside amphitheater…  (Ctrl+Enter)"
          @keydown="onPromptKey"
        />
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
        <div
          class="drop"
          :class="{ drag }"
          @dragover.prevent="drag = true"
          @dragleave="drag = false"
          @drop.prevent="onDropOne"
          @click="$refs.one.click()"
        >
          {{ oneLabel }}
          <div v-if="chainNote" class="ok">{{ chainNote }}</div>
        </div>
        <input
          ref="one"
          type="file"
          :accept="mode === 'i2v' ? 'image/*' : 'video/*'"
          hidden
          @change="onPickOne"
        />
      </div>

      <div v-if="mode === 'r2v'" class="field">
        <label>Reference images</label>
        <div
          class="drop"
          :class="{ drag }"
          @dragover.prevent="drag = true"
          @dragleave="drag = false"
          @drop.prevent="onDropRefs"
          @click="$refs.refs.click()"
        >
          {{ refFiles.length ? refFiles.map((f) => f.name).join(', ') : 'Drop refs' }}
        </div>
        <input ref="refs" type="file" accept="image/*" multiple hidden @change="onPickRefs" />
      </div>

      <label class="check">
        <input type="checkbox" v-model="noWait" /> Submit only (no_wait) — poll from Jobs
      </label>

      <p v-if="estimate" class="muted">
        Est. ≈ ${{ Number(estimate.estimated_usd).toFixed(4) }} · {{ estimate.model }} ·
        {{ estimate.note }}
      </p>
      <p v-if="error" class="err">{{ error }}</p>
      <p v-if="busy" class="muted run-line">{{ busyMsg }}</p>

      <div class="row">
        <button class="btn" :disabled="busy" @click="refreshEstimate">Estimate</button>
        <button class="btn btn-primary" :disabled="busy || !canRun" @click="run">
          {{ busy ? 'Working…' : 'Run' }}
        </button>
      </div>
    </section>

    <section class="card" v-if="result">
      <h3>
        Result <span class="badge" :class="badgeClass(result.status)">{{ result.status }}</span>
      </h3>
      <p class="mono muted">job {{ result.job_id }}</p>
      <video v-if="videoSrc" class="thumb" controls :src="videoSrc" />
      <ChainBar :result="result" @chain="emitChain" @to-jobs="emitJobs" />
    </section>
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted } from 'vue'
import { api, fileToDataUrl, getToken } from '../api'
import { toastOk, toastErr } from '../toast'
import ChainBar from './ChainBar.vue'

const props = defineProps({
  chainPayload: { type: Object, default: null },
})
const emit = defineEmits(['done', 'busy', 'chain-consumed'])

const PREF = 'imaginarium_video_prefs'
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
const oneDataUrl = ref('')
const chainNote = ref('')
const refFiles = ref([])
const drag = ref(false)
const busy = ref(false)
const busyMsg = ref('')
const error = ref('')
const result = ref(null)
const estimate = ref(null)
let tickTimer = null
let startedAt = 0

const oneLabel = computed(() => {
  if (oneFile.value) return oneFile.value.name
  if (oneDataUrl.value) return 'Loaded from library job'
  return 'Drop or choose file'
})

const canRun = computed(() => {
  if (mode.value === 'edit' || mode.value === 'extend')
    return (!!oneFile.value || !!oneDataUrl.value) && !!prompt.value.trim()
  if (mode.value === 'i2v') return !!oneFile.value || !!oneDataUrl.value
  if (mode.value === 'r2v') return refFiles.value.length > 0
  return !!prompt.value.trim()
})

const videoSrc = computed(() => {
  if (!result.value?.job_id) return ''
  const t = getToken()
  return api.libraryContentUrl(result.value.job_id) + (t ? `?token=${encodeURIComponent(t)}` : '')
})

watch([mode, duration, model, aspect, resolution], () => {
  savePrefs()
  refreshEstimate()
})

watch(
  () => props.chainPayload,
  async (p) => {
    if (!p) return
    if (p.action === 'i2v') {
      mode.value = 'i2v'
      await loadJobMedia(p.result, 'image')
    } else if (p.action === 'extend') {
      mode.value = 'extend'
      await loadJobMedia(p.result, 'video')
    } else if (p.action === 'video-edit') {
      mode.value = 'edit'
      await loadJobMedia(p.result, 'video')
    }
    emit('chain-consumed')
  }
)

function savePrefs() {
  sessionStorage.setItem(
    PREF,
    JSON.stringify({
      model: model.value,
      duration: duration.value,
      extDuration: extDuration.value,
      aspect: aspect.value,
      resolution: resolution.value,
      noWait: noWait.value,
    })
  )
}

function loadPrefs() {
  try {
    const j = JSON.parse(sessionStorage.getItem(PREF) || '{}')
    if (j.model != null) model.value = j.model
    if (j.duration) duration.value = j.duration
    if (j.extDuration) extDuration.value = j.extDuration
    if (j.aspect != null) aspect.value = j.aspect
    if (j.resolution) resolution.value = j.resolution
    if (j.noWait != null) noWait.value = j.noWait
  } catch {
    /* ignore */
  }
}

async function loadJobMedia(job, _kind) {
  oneFile.value = null
  oneDataUrl.value = ''
  chainNote.value = ''
  try {
    const t = getToken()
    const url = api.libraryContentUrl(job.job_id) + (t ? `?token=${encodeURIComponent(t)}` : '')
    const res = await fetch(url)
    if (!res.ok) throw new Error(`load asset HTTP ${res.status}`)
    const blob = await res.blob()
    const dataUrl = await new Promise((resolve, reject) => {
      const r = new FileReader()
      r.onload = () => resolve(r.result)
      r.onerror = reject
      r.readAsDataURL(blob)
    })
    oneDataUrl.value = dataUrl
    chainNote.value = `From job ${job.job_id.slice(0, 12)}…`
    toastOk('Media loaded from library')
  } catch (e) {
    toastErr(e.message || String(e))
  }
}

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
  oneDataUrl.value = ''
  chainNote.value = ''
}
function onDropOne(e) {
  drag.value = false
  oneFile.value = e.dataTransfer.files?.[0] || null
  oneDataUrl.value = ''
  chainNote.value = ''
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

function onPromptKey(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
    e.preventDefault()
    run()
  }
}

function setBusy(v, msg) {
  busy.value = v
  busyMsg.value = msg || ''
  emit('busy', { busy: v, label: msg || 'video' })
  if (v) {
    startedAt = Date.now()
    clearInterval(tickTimer)
    tickTimer = setInterval(() => {
      const s = Math.round((Date.now() - startedAt) / 1000)
      busyMsg.value = `Working… ${s}s (video can take a bit)`
      emit('busy', { busy: true, label: `${s}s` })
    }, 500)
  } else {
    clearInterval(tickTimer)
    tickTimer = null
  }
}

function emitChain(payload) {
  window.dispatchEvent(new CustomEvent('imaginarium-chain', { detail: payload }))
}
function emitJobs(job) {
  window.dispatchEvent(new CustomEvent('imaginarium-to-jobs', { detail: job }))
  emit('done', job)
}

async function oneMedia() {
  if (oneDataUrl.value) return oneDataUrl.value
  if (oneFile.value) return fileToDataUrl(oneFile.value)
  throw new Error('media required')
}

async function run() {
  error.value = ''
  if (!canRun.value) return
  setBusy(true, 'Working…')
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
      const image = await oneMedia()
      result.value = await api.videoGen({ ...bodyBase, image, duration: duration.value })
    } else if (mode.value === 'r2v') {
      const reference_images = []
      for (const f of refFiles.value) reference_images.push(await fileToDataUrl(f))
      result.value = await api.videoGen({
        ...bodyBase,
        reference_images,
        duration: duration.value,
      })
    } else if (mode.value === 'edit') {
      const video = await oneMedia()
      result.value = await api.videoEdit({
        prompt: prompt.value,
        video,
        model: model.value || undefined,
        no_wait: noWait.value,
      })
    } else {
      const video = await oneMedia()
      result.value = await api.videoExtend({
        prompt: prompt.value,
        video,
        duration: extDuration.value,
        model: model.value || undefined,
        no_wait: noWait.value,
      })
    }
    toastOk(`Video ${result.value.status || 'submitted'}`)
    emit('done', result.value)
  } catch (e) {
    error.value = e.message || String(e)
    toastErr(error.value)
  } finally {
    setBusy(false)
  }
}

onMounted(() => {
  loadPrefs()
  refreshEstimate()
})
</script>

<style scoped>
.grid {
  display: grid;
  gap: 1rem;
  grid-template-columns: 1.2fr 1fr;
}
@media (max-width: 900px) {
  .grid {
    grid-template-columns: 1fr;
  }
}
h2,
h3 {
  margin: 0 0 0.75rem;
}
.mode-row {
  display: flex;
  gap: 0.35rem;
  margin-bottom: 1rem;
  flex-wrap: wrap;
}
.tab {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
  border-radius: 999px;
  padding: 0.35rem 0.75rem;
  cursor: pointer;
}
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
}
.check {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  margin: 0.5rem 0 1rem;
  color: var(--muted);
  font-size: 0.9rem;
}
.run-line {
  color: var(--warn);
}
</style>
