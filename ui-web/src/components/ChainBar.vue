<template>
  <div v-if="result" class="chain card">
    <div class="chain-head">
      <strong>Chain</strong>
      <span class="muted mono">{{ shortId }}</span>
    </div>
    <div class="chain-acts">
      <button class="btn" type="button" @click="copyId">Copy job id</button>
      <a class="btn" :href="contentUrl" target="_blank" rel="noopener">Open file</a>
      <a class="btn" :href="contentUrl" :download="downloadName">Download</a>
      <button class="btn" type="button" @click="$emit('to-jobs', result)">Jobs</button>

      <!-- Image paths -->
      <button v-if="isImage" class="btn" type="button" @click="go('craft')">→ Craft</button>
      <button v-if="isImage" class="btn btn-primary" type="button" @click="go('i2v')">→ I2V</button>
      <button v-if="isImage" class="btn" type="button" @click="go('image-edit')">→ AI edit</button>

      <!-- Video paths -->
      <button v-if="isVideo" class="btn" type="button" @click="go('video-craft')">→ Video craft</button>
      <button v-if="isVideo" class="btn btn-primary" type="button" @click="go('extend')">→ Extend</button>
      <button v-if="isVideo" class="btn" type="button" @click="go('video-edit')">→ AI edit</button>
    </div>
    <p class="muted loop-hint" v-if="isImage">
      Loop: craft → I2V → video craft → extend
    </p>
    <p class="muted loop-hint" v-else-if="isVideo">
      Loop: trim/fade → extend · or AI edit the cut
    </p>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { api, getToken } from '../api'
import { toastOk, toastErr } from '../toast'

const props = defineProps({
  result: { type: Object, required: true },
})
const emit = defineEmits(['chain', 'to-jobs'])

const shortId = computed(() => {
  const id = props.result?.job_id || ''
  return id.length > 16 ? id.slice(0, 14) + '…' : id
})

const asset = computed(() => (props.result?.assets && props.result.assets[0]) || null)
const mode = computed(() => String(props.result?.mode || '').toLowerCase())

const isImage = computed(() => {
  const a = asset.value
  const p = (a?.local_path || a?.url || a?.mime_type || '').toLowerCase()
  if (a?.kind === 'image') return true
  if (a?.kind === 'video') return false
  if (/\.(png|jpe?g|webp|gif)$/i.test(p) || p.includes('image/')) return true
  if (/\.(mp4|webm|mov)$/i.test(p) || p.includes('video/')) return false
  if (mode.value.includes('video') || mode.value.includes('t2v') || mode.value.includes('i2v'))
    return false
  if (mode.value.includes('image') || mode.value.includes('craft_export')) return true
  // craft_export default image unless mime says video
  if (mode.value.includes('craft')) return !p.includes('video')
  return true
})

const isVideo = computed(() => {
  if (isImage.value) return false
  const a = asset.value
  const p = (a?.local_path || a?.url || a?.mime_type || '').toLowerCase()
  if (a?.kind === 'video') return true
  if (/\.(mp4|webm|mov)$/i.test(p) || p.includes('video/')) return true
  return (
    mode.value.includes('video') ||
    mode.value.includes('t2v') ||
    mode.value.includes('extend') ||
    mode.value.includes('craft')
  )
})

const contentUrl = computed(() => {
  const id = props.result?.job_id
  if (!id) return asset.value?.url || asset.value?.public_url || '#'
  const t = getToken()
  return api.libraryContentUrl(id) + (t ? `?token=${encodeURIComponent(t)}` : '')
})

const downloadName = computed(() => {
  if (isVideo.value) return `${props.result.job_id || 'clip'}.mp4`
  return `${props.result.job_id || 'image'}.png`
})

function go(action) {
  emit('chain', { action, result: props.result })
}

async function copyId() {
  try {
    await navigator.clipboard.writeText(props.result.job_id || '')
    toastOk('Job id copied')
  } catch {
    toastErr('Clipboard failed')
  }
}
</script>

<style scoped>
.chain {
  margin-top: 0.75rem;
  padding: 0.75rem 0.9rem;
}
.chain-head {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  margin-bottom: 0.55rem;
  font-size: 0.9rem;
}
.chain-acts {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}
.loop-hint {
  margin: 0.55rem 0 0;
  font-size: 0.78rem;
}
</style>
