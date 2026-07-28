<template>
  <div class="toasts" aria-live="polite">
    <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind" @click="dismissToast(t.id)">
      {{ t.text }}
    </div>
  </div>
</template>

<script setup>
import { toasts, dismissToast } from '../toast'
</script>

<style scoped>
.toasts {
  position: fixed;
  right: 1rem;
  bottom: 1rem;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  max-width: min(420px, 92vw);
  pointer-events: none;
}
.toast {
  pointer-events: auto;
  padding: 0.7rem 0.95rem;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--bg-elev);
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);
  font-size: 0.9rem;
  cursor: pointer;
  animation: in 0.18s ease-out;
}
.toast.ok {
  border-color: #2d5a40;
  color: var(--ok);
}
.toast.err {
  border-color: #5a2d32;
  color: var(--err);
}
.toast.info {
  color: var(--text);
}
@keyframes in {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
</style>
