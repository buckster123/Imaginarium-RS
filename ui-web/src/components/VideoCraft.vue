<template>
  <div class="vcraft">
    <aside class="tools card">
      <h2>Video craft</h2>
      <p class="muted tip">Kdenlive-light · ffmpeg on the fat node</p>

      <div class="field">
        <label>Add clip from job id</label>
        <div class="row">
          <input v-model="jobInput" class="grow" placeholder="paste job ULID" @keydown.enter="addJob" />
          <button class="btn" @click="addJob">Add</button>
        </div>
        <button class="btn" @click="loadRecentVideos">From recent jobs</button>
      </div>

      <div v-if="clips.length" class="clip-list">
        <div v-for="(c, i) in clips" :key="c.uid" class="clip card">
          <div class="clip-head">
            <span class="mono">{{ short(c.job_id) }}</span>
            <button class="btn btn-ghost" @click="removeClip(i)">×</button>
          </div>
          <div class="row">
            <div class="field">
              <label>In (s)</label>
              <input type="number" min="0" step="0.1" v-model.number="c.in_s" />
            </div>
            <div class="field">
              <label>Out (s)</label>
              <input type="number" min="0" step="0.1" v-model.number="c.out_s" placeholder="end" />
            </div>
            <div class="field">
              <label>Gain dB</label>
              <input type="number" step="0.5" v-model.number="c.gain_db" />
            </div>
          </div>
          <div class="row">
            <button class="btn btn-ghost" :disabled="i === 0" @click="move(i, -1)">↑</button>
            <button class="btn btn-ghost" :disabled="i === clips.length - 1" @click="move(i, 1)">↓</button>
          </div>
        </div>
      </div>
      <p v-else class="muted">Add one or more library video jobs, set trims, render.</p>

      <div class="field"><label>Audio fade in {{ audioIn }}s</label>
        <input type="range" min="0" max="3" step="0.05" v-model.number="audioIn" /></div>
      <div class="field"><label>Audio fade out {{ audioOut }}s</label>
        <input type="range" min="0" max="3" step="0.05" v-model.number="audioOut" /></div>
      <div class="field"><label>Video fade in {{ videoIn }}s</label>
        <input type="range" min="0" max="3" step="0.05" v-model.number="videoIn" /></div>
      <div class="field"><label>Video fade out {{ videoOut }}s</label>
        <input type="range" min="0" max="3" step="0.05" v-model.number="videoOut" /></div>

      <div class="field">
        <label>Text overlay (optional)</label>
        <input v-model="overlayText" placeholder="frenship protocol" />
      </div>
      <div class="row">
        <div class="field"><label>Start s</label><input type="number" min="0" step="0.1" v-model.number="ovStart" /></div>
        <div class="field"><label>End s</label><input type="number" min="0" step="0.1" v-model.number="ovEnd" /></div>
      </div>

      <div class="field"><label>Note</label><input v-model="note" placeholder="laundry rave cut" /></div>

      <p v-if="error" class="err">{{ error }}</p>
      <button class="btn btn-primary" :disabled="!clips.length || busy" @click="render()">
        {{ busy ? busyMsg : 'Render → library' }}
      </button>
      <div class="row loop-row">
        <button class="btn btn-primary" :disabled="!clips.length || busy" @click="renderThen('extend')">
          Render → Extend
        </button>
        <button class="btn" :disabled="!clips.length || busy" @click="renderThen('video-edit')">
          Render → AI edit
        </button>
      </div>
      <p v-if="lastId" class="ok mono">Saved {{ lastId }}</p>
      <p class="muted tip" style="margin-top: 0.5rem">
        Loop: trim/fade → extend · AI edit the cut · craft still → I2V
      </p>
    </aside>

    <section class="preview card">
      <h3>Preview source</h3>
      <p class="muted" v-if="!selected">Select a clip or render to preview output.</p>
      <video v-if="previewUrl" class="vid" controls :src="previewUrl" />
      <div class="row" style="margin-top: 0.75rem">
        <button
          v-for="(c, i) in clips"
          :key="c.uid"
          class="btn"
          :class="{ 'btn-primary': selected === i }"
          @click="selectClip(i)"
        >
          Clip {{ i + 1 }}
        </button>
      </div>
      <ChainBar v-if="result" :result="result" @chain="emitChain" @to-jobs="emitJobs" />
    </section>
  </div>
</template>

<script setup>
import { ref, computed, watch } from 'vue'
import { api, getToken } from '../api'
import { toastOk, toastErr } from '../toast'
import ChainBar from './ChainBar.vue'

const props = defineProps({ chainPayload: { type: Object, default: null } })
const emit = defineEmits(['done', 'busy', 'chain-consumed'])

const jobInput = ref('')
const clips = ref([])
const audioIn = ref(0.25)
const audioOut = ref(0.4)
const videoIn = ref(0.15)
const videoOut = ref(0.35)
const overlayText = ref('')
const ovStart = ref(0.3)
const ovEnd = ref(2.5)
const note = ref('')
const busy = ref(false)
const busyMsg = ref('Rendering…')
const error = ref('')
const lastId = ref('')
const selected = ref(null)
const result = ref(null)
let uidSeq = 1

watch(
  () => props.chainPayload,
  (p) => {
    if (!p?.result?.job_id) return
    if (p.action === 'video-craft' || p.action === 'craft-video') {
      addClip(p.result.job_id)
      emit('chain-consumed')
    }
  }
)

function short(id) {
  return id?.length > 14 ? id.slice(0, 12) + '…' : id
}

function contentUrl(jobId) {
  const t = getToken()
  return api.libraryContentUrl(jobId) + (t ? `?token=${encodeURIComponent(t)}` : '')
}

const previewUrl = computed(() => {
  if (result.value?.job_id) return contentUrl(result.value.job_id)
  if (selected.value != null && clips.value[selected.value]) {
    return contentUrl(clips.value[selected.value].job_id)
  }
  return ''
})

function addClip(jobId) {
  const id = (jobId || '').trim()
  if (!id) return
  clips.value.push({
    uid: uidSeq++,
    job_id: id,
    in_s: 0,
    out_s: 0,
    gain_db: 0,
  })
  selected.value = clips.value.length - 1
  toastOk(`Clip added ${short(id)}`)
}

function addJob() {
  addClip(jobInput.value)
  jobInput.value = ''
}

function removeClip(i) {
  clips.value.splice(i, 1)
  if (selected.value === i) selected.value = null
}

function move(i, dir) {
  const j = i + dir
  if (j < 0 || j >= clips.value.length) return
  const t = clips.value[i]
  clips.value[i] = clips.value[j]
  clips.value[j] = t
  selected.value = j
}

function selectClip(i) {
  selected.value = i
}

async function loadRecentVideos() {
  try {
    const data = await api.jobs(40)
    const list = Array.isArray(data) ? data : data.jobs || []
    const vids = list.filter((j) => {
      const m = String(j.mode || '')
      return m.includes('video') || m.includes('craft')
    })
    if (!vids.length) {
      toastErr('No recent video-ish jobs')
      return
    }
    // add first pending selection - show as quick picks via adding top 3
    for (const j of vids.slice(0, 5)) {
      if (!clips.value.some((c) => c.job_id === j.job_id)) addClip(j.job_id)
    }
  } catch (e) {
    toastErr(e.message)
  }
}

async function render() {
  if (!clips.value.length) return null
  error.value = ''
  busy.value = true
  busyMsg.value = 'ffmpeg rendering…'
  emit('busy', { busy: true, label: 'ffmpeg' })
  const started = Date.now()
  const tick = setInterval(() => {
    const s = Math.round((Date.now() - started) / 1000)
    busyMsg.value = `ffmpeg rendering… ${s}s`
    emit('busy', { busy: true, label: `${s}s` })
  }, 500)
  try {
    const body = {
      clips: clips.value.map((c) => ({
        job_id: c.job_id,
        in_s: Number(c.in_s) || 0,
        out_s: Number(c.out_s) || 0,
        gain_db: Number(c.gain_db) || 0,
      })),
      audio_fade_in_s: audioIn.value,
      audio_fade_out_s: audioOut.value,
      video_fade_in_s: videoIn.value,
      video_fade_out_s: videoOut.value,
      overlays: overlayText.value.trim()
        ? [
            {
              text: overlayText.value.trim(),
              start_s: ovStart.value,
              end_s: ovEnd.value,
              x: 40,
              y: 40,
              fontsize: 32,
            },
          ]
        : [],
      note: note.value || 'video craft render',
    }
    result.value = await api.craftVideoRender(body)
    lastId.value = result.value.job_id
    toastOk(`Rendered ${short(result.value.job_id)}`)
    emit('done', result.value)
    return result.value
  } catch (e) {
    error.value = e.message
    toastErr(e.message)
    return null
  } finally {
    clearInterval(tick)
    busy.value = false
    emit('busy', { busy: false })
  }
}

async function renderThen(action) {
  const job = await render()
  if (!job) return
  window.dispatchEvent(
    new CustomEvent('imaginarium-chain', {
      detail: { action, result: job },
    })
  )
  toastOk(action === 'extend' ? 'Cut → Extend' : 'Cut → AI video edit')
}

function emitChain(p) {
  window.dispatchEvent(new CustomEvent('imaginarium-chain', { detail: p }))
}
function emitJobs(job) {
  window.dispatchEvent(new CustomEvent('imaginarium-to-jobs', { detail: job }))
  emit('done', job)
}
</script>

<style scoped>
.vcraft {
  display: grid;
  grid-template-columns: 340px 1fr;
  gap: 1rem;
  align-items: start;
}
@media (max-width: 900px) {
  .vcraft {
    grid-template-columns: 1fr;
  }
}
h2, h3 { margin: 0 0 0.5rem; }
.tip { margin: 0 0 0.75rem; font-size: 0.82rem; }
.grow { flex: 1; min-width: 0; }
.clip-list { display: flex; flex-direction: column; gap: 0.5rem; margin: 0.75rem 0; max-height: 280px; overflow: auto; }
.clip { padding: 0.65rem; }
.clip-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.35rem; }
.vid { width: 100%; max-height: 420px; border-radius: 8px; background: #000; }
.loop-row { margin-top: 0.45rem; }
input[type='number'], input:not([type]) {
  width: 100%;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 0.45rem 0.55rem;
  color: inherit;
}
</style>
