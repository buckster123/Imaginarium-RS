<template>
  <section class="card">
    <div class="head">
      <h2>Jobs</h2>
      <button class="btn" :disabled="loading" @click="load">Refresh</button>
    </div>
    <p v-if="flash" class="ok muted">Latest: <span class="mono">{{ flash.job_id }}</span></p>
    <p v-if="error" class="err">{{ error }}</p>
    <table v-if="items.length">
      <thead>
        <tr>
          <th>Job</th>
          <th>Status</th>
          <th>Mode</th>
          <th>Model</th>
          <th>Created</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="j in items" :key="j.job_id">
          <td class="mono">{{ short(j.job_id) }}</td>
          <td><span class="badge" :class="badgeClass(j.status)">{{ j.status }}</span></td>
          <td>{{ j.mode }}</td>
          <td class="mono">{{ j.model }}</td>
          <td class="muted">{{ j.created_at }}</td>
          <td class="acts">
            <button class="btn btn-ghost" @click="open(j)">View</button>
            <button
              v-if="isPending(j.status)"
              class="btn btn-ghost"
              :disabled="waiting === j.job_id"
              @click="wait(j)"
            >
              Wait
            </button>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="muted">No jobs yet — generate something.</p>

    <div v-if="detail" class="detail card">
      <h3>Detail <span class="mono">{{ detail.job_id }}</span></h3>
      <pre class="mono">{{ pretty(detail) }}</pre>
      <div v-if="mediaUrl" class="media">
        <video v-if="isVideo" class="thumb" controls :src="mediaUrl" />
        <img v-else class="thumb" :src="mediaUrl" alt="asset" />
      </div>
    </div>
  </section>
</template>

<script setup>
import { ref, watch, onMounted, computed } from 'vue'
import { api, getToken } from '../api'

const props = defineProps({ flash: Object })
const emit = defineEmits(['select'])

const items = ref([])
const loading = ref(false)
const error = ref('')
const detail = ref(null)
const waiting = ref('')

const mediaUrl = computed(() => {
  if (!detail.value?.job_id) return ''
  const t = getToken()
  return api.libraryContentUrl(detail.value.job_id) + (t ? `?token=${encodeURIComponent(t)}` : '')
})
const isVideo = computed(() => {
  const a = detail.value?.assets?.[0]
  const p = a?.local_path || a?.url || ''
  return /\.mp4|\.webm/i.test(p) || a?.kind === 'video'
})

watch(
  () => props.flash,
  (j) => {
    if (j) {
      load()
      detail.value = j
    }
  }
)

function short(id) {
  return id?.length > 12 ? id.slice(0, 10) + '…' : id
}
function pretty(o) {
  return JSON.stringify(o, null, 2)
}
function badgeClass(s) {
  if (s === 'completed' || s === 'done') return 'done'
  if (s === 'failed' || s === 'error') return 'fail'
  return 'run'
}
function isPending(s) {
  return ['pending', 'submitted', 'processing', 'running', 'in_progress'].includes(
    String(s || '').toLowerCase()
  )
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const data = await api.jobs(40)
    items.value = Array.isArray(data) ? data : data.jobs || []
  } catch (e) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

async function open(j) {
  try {
    detail.value = await api.job(j.job_id)
    emit('select', detail.value)
  } catch (e) {
    error.value = e.message
  }
}

async function wait(j) {
  waiting.value = j.job_id
  error.value = ''
  try {
    detail.value = await api.jobWait(j.job_id)
    await load()
  } catch (e) {
    error.value = e.message
  } finally {
    waiting.value = ''
  }
}

onMounted(load)
</script>

<style scoped>
.head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.75rem; }
h2, h3 { margin: 0; }
table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
th, td { text-align: left; padding: 0.45rem 0.35rem; border-bottom: 1px solid var(--border); }
th { color: var(--muted); font-weight: 500; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; }
.acts { display: flex; gap: 0.25rem; }
.detail { margin-top: 1rem; }
pre {
  max-height: 240px;
  overflow: auto;
  background: var(--bg);
  padding: 0.75rem;
  border-radius: 8px;
  font-size: 0.78rem;
}
.media { margin-top: 0.75rem; }
</style>
