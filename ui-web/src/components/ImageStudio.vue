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
        <textarea
          ref="promptEl"
          v-model="prompt"
          placeholder="Marble amphitheater at golden hour…  (Ctrl+Enter to run)"
          @keydown="onPromptKey"
        />
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
          <div v-if="chainNote" class="ok">{{ chainNote }}</div>
        </div>
        <input ref="file" type="file" accept="image/*" multiple hidden @change="onPick" />
      </div>

      <p v-if="estimate" class="muted">
        Est. ≈ ${{ Number(estimate.estimated_usd).toFixed(4) }} · {{ estimate.note }}
      </p>
      <p v-if="error" class="err">{{ error }}</p>
      <p v-if="busy" class="muted run-line">{{ busyMsg }}</p>

      <div class="row">
        <button class="btn" :disabled="busy" @click="refreshEstimate">Estimate</button>
        <button class="btn btn-primary" :disabled="busy || !canRun" @click="run">
          {{ busy ? 'Working…' : mode === 'gen' ? 'Generate' : 'Edit' }}
        </button>
      </div>
    </section>

    <section class="card preview" v-if="result">
      <h3>
        Result <span class="badge" :class="badgeClass(result.status)">{{ result.status }}</span>
      </h3>
      <p class="mono muted">job {{ result.job_id }}</p>
      <div class="thumbs">
        <template v-for="(a, i) in result.assets || []" :key="i">
          <img v-if="isImage(a)" class="thumb" :src="assetSrc(a)" :alt="'asset ' + i" />
          <a v-else class="mono" :href="assetSrc(a)" target="_blank">open</a>
        </template>
      </div>
      <ChainBar
        :result="result"
        @chain="emitChain"
        @to-jobs="emitJobs"
      />
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

const PREF = 'imaginarium_image_prefs'

const mode = ref('gen')
const prompt = ref('')
const model = ref('quality')
const n = ref(1)
const aspect = ref('16:9')
const resolution = ref('')
const files = ref([])
const fileDataUrls = ref([]) // for chain-loaded remote images
const chainNote = ref('')
const drag = ref(false)
const busy = ref(false)
const busyMsg = ref('')
const error = ref('')
const result = ref(null)
const estimate = ref(null)
const promptEl = ref(null)
const startedAt = ref(0)
let tickTimer = null

const canRun = computed(() => {
  if (!prompt.value.trim()) return false
  if (mode.value === 'edit' && !files.value.length && !fileDataUrls.value.length) return false
  return true
})

watch([model, n, aspect, resolution], () => {
  savePrefs()
  refreshEstimate()
})

watch(
  () => props.chainPayload,
  async (p) => {
    if (!p) return
    if (p.action === 'image-edit') {
      mode.value = 'edit'
      await loadJobAsSource(p.result)
      emit('chain-consumed')
    }
  }
)

function savePrefs() {
  sessionStorage.setItem(
    PREF,
    JSON.stringify({
      model: model.value,
      n: n.value,
      aspect: aspect.value,
      resolution: resolution.value,
    })
  )
}

function loadPrefs() {
  try {
    const j = JSON.parse(sessionStorage.getItem(PREF) || '{}')
    if (j.model) model.value = j.model
    if (j.n) n.value = j.n
    if (j.aspect != null) aspect.value = j.aspect
    if (j.resolution != null) resolution.value = j.resolution
  } catch {
    /* ignore */
  }
}

async function loadJobAsSource(job) {
  chainNote.value = ''
  fileDataUrls.value = []
  files.value = []
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
    fileDataUrls.value = [dataUrl]
    chainNote.value = `Loaded from job ${job.job_id.slice(0, 12)}…`
    toastOk('Source image loaded for AI edit')
  } catch (e) {
    toastErr(e.message || String(e))
  }
}

async function refreshEstimate() {
  try {
    estimate.value = await api.estimate({ kind: 'image', model: model.value, n: n.value })
  } catch {
    estimate.value = null
  }
}

function onPick(e) {
  files.value = [...(e.target.files || [])].slice(0, 3)
  fileDataUrls.value = []
  chainNote.value = ''
}
function onDrop(e) {
  drag.value = false
  files.value = [...(e.dataTransfer.files || [])].filter((f) => f.type.startsWith('image/')).slice(0, 3)
  fileDataUrls.value = []
  chainNote.value = ''
}

function isImage(a) {
  const p = a.local_path || a.url || ''
  return /\.(png|jpe?g|webp|gif)$/i.test(p) || a.kind === 'image'
}
function assetSrc(a) {
  if (result.value?.job_id) {
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

function onPromptKey(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
    e.preventDefault()
    run()
  }
}

function setBusy(v, msg) {
  busy.value = v
  busyMsg.value = msg || ''
  emit('busy', { busy: v, label: msg || 'image' })
  if (v) {
    startedAt.value = Date.now()
    clearInterval(tickTimer)
    tickTimer = setInterval(() => {
      const s = Math.round((Date.now() - startedAt.value) / 1000)
      busyMsg.value = `Working… ${s}s`
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

async function run() {
  error.value = ''
  if (!canRun.value) return
  setBusy(true, 'Working…')
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
      const images = [...fileDataUrls.value]
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
    toastOk(`Image ${result.value.status || 'done'}`)
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
}
.tab {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
  border-radius: 999px;
  padding: 0.35rem 0.85rem;
  cursor: pointer;
}
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
}
.thumbs {
  display: grid;
  gap: 0.5rem;
  margin: 0.75rem 0;
}
.preview {
  align-self: start;
}
.run-line {
  color: var(--warn);
}
</style>
