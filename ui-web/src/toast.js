import { ref } from 'vue'

/** @type {import('vue').Ref<Array<{id:number,kind:string,text:string}>>} */
export const toasts = ref([])
let seq = 1

export function toast(text, kind = 'info', ms = 4200) {
  const id = seq++
  toasts.value = [...toasts.value, { id, kind, text }]
  if (ms > 0) {
    setTimeout(() => dismissToast(id), ms)
  }
  return id
}

export function dismissToast(id) {
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

export function toastOk(text) {
  return toast(text, 'ok')
}
export function toastErr(text) {
  return toast(text, 'err', 7000)
}
