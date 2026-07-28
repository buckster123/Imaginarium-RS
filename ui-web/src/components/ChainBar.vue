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
      <button
        v-if="isImage"
        class="btn btn-primary"
        type="button"
        @click="$emit('chain', { action: 'i2v', result })"
      >
        → I2V
      </button>
      <button
        v-if="isImage"
        class="btn"
        type="button"
        @click="$emit('chain', { action: 'image-edit', result })"
      >
        → AI edit
      </button>
      <button
        v-if="isVideo"
        class="btn btn-primary"
        type="button"
        @click="$emit('chain', { action: 'extend', result })"
      >
        → Extend
      </button>
      <button
        v-if="isVideo"
        class="btn"
        type="button"
        @click="$emit('chain', { action: 'video-edit', result })"
      >
        → AI edit
      </button>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { api, getToken } from '../api'
import { toastOk, toastErr } from '../toast'

const props = defineProps({
  result: { type: Object, required: true },
})
defineEmits(['chain', 'to-jobs'])

const shortId = computed(() => {
  const id = props.result?.job_id || ''
  return id.length > 16 ? id.slice(0, 14) + '…' : id
})

const asset = computed(() => (props.result?.assets && props.result.assets[0]) || null)

const isImage = computed(() => {
  const a = asset.value
  if (!a) return String(props.result?.mode || '').includes('image')
  const p = a.local_path || a.url || ''
  return a.kind === 'image' || /\.(png|jpe?g|webp|gif)$/i.test(p)
})

const isVideo = computed(() => {
  const a = asset.value
  if (!a) return String(props.result?.mode || '').includes('video') || String(props.result?.mode || '').includes('t2v')
  const p = a.local_path || a.url || ''
  return a.kind === 'video' || /\.(mp4|webm)$/i.test(p)
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
</style>
