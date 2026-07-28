<template>
  <div>
    <div class="mode-bar">
      <button class="tab" :class="{ active: mode === 'image' }" @click="mode = 'image'">Image craft</button>
      <button class="tab" :class="{ active: mode === 'video' }" @click="mode = 'video'">Video craft</button>
    </div>
    <ImageCraft
      v-show="mode === 'image'"
      :chain-payload="imageChain"
      @done="$emit('done', $event)"
      @busy="$emit('busy', $event)"
      @chain-consumed="$emit('chain-consumed')"
    />
    <VideoCraft
      v-show="mode === 'video'"
      :chain-payload="videoChain"
      @done="$emit('done', $event)"
      @busy="$emit('busy', $event)"
      @chain-consumed="$emit('chain-consumed')"
    />
  </div>
</template>

<script setup>
import { ref, watch } from 'vue'
import ImageCraft from './ImageCraft.vue'
import VideoCraft from './VideoCraft.vue'

const props = defineProps({
  chainPayload: { type: Object, default: null },
})
defineEmits(['done', 'busy', 'chain-consumed'])

const mode = ref('image')
const imageChain = ref(null)
const videoChain = ref(null)

watch(
  () => props.chainPayload,
  (p) => {
    if (!p) return
    if (p.action === 'craft' || p.action === 'image-craft') {
      mode.value = 'image'
      imageChain.value = p
    } else if (p.action === 'video-craft' || p.action === 'craft-video') {
      mode.value = 'video'
      videoChain.value = p
    }
  }
)
</script>

<style scoped>
.mode-bar {
  display: flex;
  gap: 0.4rem;
  margin-bottom: 0.85rem;
}
.tab {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
  border-radius: 999px;
  padding: 0.4rem 0.9rem;
  cursor: pointer;
}
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
  background: rgba(212, 168, 75, 0.08);
}
</style>
