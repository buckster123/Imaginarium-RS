<template>
  <div class="craft">
    <aside class="tools card">
      <h2>Image craft</h2>
      <p class="muted tip">PaintShop-light · non-destructive export to library</p>

      <div class="tool-row">
        <button
          v-for="t in toolList"
          :key="t.id"
          class="tab"
          :class="{ active: tool === t.id }"
          @click="tool = t.id"
        >
          {{ t.label }}
        </button>
      </div>

      <div class="field">
        <label>Open</label>
        <div class="row">
          <button class="btn" @click="$refs.file.click()">File…</button>
          <button class="btn" :disabled="!lastJobId" @click="loadJob(lastJobId)">Reload job</button>
        </div>
        <input ref="file" type="file" accept="image/*" hidden @change="onFile" />
      </div>

      <template v-if="tool === 'adjust'">
        <div class="field"><label>Exposure {{ exposure.toFixed(2) }}</label>
          <input type="range" min="-1" max="1" step="0.01" v-model.number="exposure" @input="render" /></div>
        <div class="field"><label>Contrast {{ contrast.toFixed(2) }}</label>
          <input type="range" min="-1" max="1" step="0.01" v-model.number="contrast" @input="render" /></div>
        <div class="field"><label>Saturation {{ saturation.toFixed(2) }}</label>
          <input type="range" min="-1" max="1" step="0.01" v-model.number="saturation" @input="render" /></div>
        <button class="btn" @click="resetAdjust">Reset adjust</button>
      </template>

      <template v-if="tool === 'paint' || tool === 'eraser' || tool === 'mask'">
        <div class="field"><label>Brush size {{ brushSize }}</label>
          <input type="range" min="2" max="80" v-model.number="brushSize" /></div>
        <div class="field" v-if="tool === 'paint'">
          <label>Color</label>
          <input type="color" v-model="brushColor" />
        </div>
        <div class="field"><label>Hardness {{ hardness.toFixed(2) }}</label>
          <input type="range" min="0.1" max="1" step="0.05" v-model.number="hardness" /></div>
      </template>

      <template v-if="tool === 'text'">
        <div class="field"><label>Text</label><input v-model="textValue" /></div>
        <div class="field"><label>Size {{ textSize }}</label>
          <input type="range" min="12" max="120" v-model.number="textSize" /></div>
        <div class="field"><label>Color</label><input type="color" v-model="textColor" /></div>
        <p class="muted">Click canvas to place text</p>
      </template>

      <template v-if="tool === 'crop'">
        <p class="muted">Drag on canvas to set crop rect, then Apply crop</p>
        <div class="row">
          <button class="btn btn-primary" :disabled="!cropRect" @click="applyCrop">Apply crop</button>
          <button class="btn" @click="cropRect = null; render()">Clear</button>
        </div>
        <div class="row">
          <button class="btn" @click="setAspectCrop(1)">1:1</button>
          <button class="btn" @click="setAspectCrop(16/9)">16:9</button>
          <button class="btn" @click="setAspectCrop(9/16)">9:16</button>
        </div>
      </template>

      <div class="row transforms">
        <button class="btn" :disabled="!canUndo" @click="undo" title="Ctrl+Z">Undo</button>
        <button class="btn" :disabled="!canRedo" @click="redo" title="Ctrl+Shift+Z">Redo</button>
        <button class="btn" @click="rotate90">Rotate 90°</button>
        <button class="btn" @click="flipH">Flip H</button>
        <button class="btn" @click="flipV">Flip V</button>
      </div>

      <div class="field">
        <label>Export format</label>
        <select v-model="exportFmt">
          <option value="image/png">PNG</option>
          <option value="image/jpeg">JPEG</option>
          <option value="image/webp">WebP</option>
        </select>
      </div>
      <div class="field" v-if="exportFmt !== 'image/png'">
        <label>Quality {{ exportQ }}</label>
        <input type="range" min="0.5" max="1" step="0.05" v-model.number="exportQ" />
      </div>

      <div class="row">
        <button class="btn btn-primary" :disabled="!hasImage || busy" @click="exportLibrary">
          {{ busy ? 'Saving…' : 'Export → library' }}
        </button>
        <button class="btn" :disabled="!hasImage || busy" @click="exportMask">Export mask</button>
      </div>
      <p v-if="error" class="err">{{ error }}</p>
      <p v-if="lastExportId" class="ok mono">Saved job {{ lastExportId }}</p>
    </aside>

    <section class="stage card">
      <div v-if="!hasImage" class="empty muted">
        Open a file, or chain <strong>→ Craft</strong> from an image result.
      </div>
      <canvas
        ref="canvas"
        class="board"
        :class="{ drawing: isDrawingTool }"
        @pointerdown="onDown"
        @pointermove="onMove"
        @pointerup="onUp"
        @pointerleave="onUp"
      />
    </section>
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { api, getToken } from '../api'
import { toastOk, toastErr } from '../toast'

const props = defineProps({
  chainPayload: { type: Object, default: null },
})
const emit = defineEmits(['done', 'chain-consumed', 'busy'])

const toolList = [
  { id: 'pan', label: 'View' },
  { id: 'crop', label: 'Crop' },
  { id: 'adjust', label: 'Adjust' },
  { id: 'paint', label: 'Paint' },
  { id: 'eraser', label: 'Eraser' },
  { id: 'mask', label: 'Mask' },
  { id: 'text', label: 'Text' },
]

const tool = ref('paint')
const canvas = ref(null)
const exposure = ref(0)
const contrast = ref(0)
const saturation = ref(0)
const brushSize = ref(18)
const brushColor = ref('#d4a84b')
const hardness = ref(0.85)
const textValue = ref('frenship')
const textSize = ref(36)
const textColor = ref('#ffffff')
const exportFmt = ref('image/png')
const exportQ = ref(0.92)
const busy = ref(false)
const error = ref('')
const lastExportId = ref('')
const lastJobId = ref('')
const sourceJobId = ref(null)
const hasImage = ref(false)
const cropRect = ref(null)
const canUndo = ref(false)
const canRedo = ref(false)

// layers
let baseCanvas = null // offscreen original+adjust
let paintCanvas = null
let maskCanvas = null
let imgNatural = { w: 0, h: 0 }
let drawing = false
let lastPt = null
let cropStart = null
let strokeDirty = false

/** @type {Array<object>} */
let history = []
let historyIndex = -1
const HISTORY_MAX = 40
let suppressHistory = false

const isDrawingTool = computed(() => ['paint', 'eraser', 'mask', 'crop'].includes(tool.value))

function syncHistoryButtons() {
  canUndo.value = historyIndex > 0
  canRedo.value = historyIndex >= 0 && historyIndex < history.length - 1
}

function snapshotLayers() {
  if (!baseCanvas || !paintCanvas || !maskCanvas) return null
  return {
    w: imgNatural.w,
    h: imgNatural.h,
    base: baseCanvas.toDataURL('image/png'),
    paint: paintCanvas.toDataURL('image/png'),
    mask: maskCanvas.toDataURL('image/png'),
    exposure: exposure.value,
    contrast: contrast.value,
    saturation: saturation.value,
  }
}

async function loadDataUrlOnto(canvasEl, dataUrl) {
  const img = await loadImageBitmap(dataUrl)
  const ctx = canvasEl.getContext('2d')
  ctx.clearRect(0, 0, canvasEl.width, canvasEl.height)
  ctx.drawImage(img, 0, 0)
}

async function restoreSnapshot(snap) {
  if (!snap) return
  suppressHistory = true
  ensureLayers(snap.w, snap.h)
  imgNatural = { w: snap.w, h: snap.h }
  exposure.value = snap.exposure
  contrast.value = snap.contrast
  saturation.value = snap.saturation
  await Promise.all([
    loadDataUrlOnto(baseCanvas, snap.base),
    loadDataUrlOnto(paintCanvas, snap.paint),
    loadDataUrlOnto(maskCanvas, snap.mask),
  ])
  hasImage.value = true
  cropRect.value = null
  await nextTick()
  fitCanvas()
  render()
  suppressHistory = false
  syncHistoryButtons()
}

function pushHistory(label = '') {
  if (suppressHistory || !hasImage.value || !baseCanvas) return
  const snap = snapshotLayers()
  if (!snap) return
  // drop redo branch
  if (historyIndex < history.length - 1) {
    history = history.slice(0, historyIndex + 1)
  }
  history.push({ ...snap, label })
  if (history.length > HISTORY_MAX) {
    history.shift()
  }
  historyIndex = history.length - 1
  syncHistoryButtons()
}

async function undo() {
  if (historyIndex <= 0) return
  historyIndex -= 1
  await restoreSnapshot(history[historyIndex])
  toastOk('Undo')
}

async function redo() {
  if (historyIndex >= history.length - 1) return
  historyIndex += 1
  await restoreSnapshot(history[historyIndex])
  toastOk('Redo')
}

function onKeyHistory(e) {
  const mod = e.ctrlKey || e.metaKey
  if (!mod || !hasImage.value) return
  const key = e.key.toLowerCase()
  if (key === 'z' && !e.shiftKey) {
    e.preventDefault()
    undo()
  } else if ((key === 'z' && e.shiftKey) || key === 'y') {
    e.preventDefault()
    redo()
  }
}

watch(
  () => props.chainPayload,
  async (p) => {
    if (!p) return
    if (p.action === 'craft' || p.action === 'image-craft') {
      await loadJob(p.result?.job_id)
      emit('chain-consumed')
    }
  }
)

function ensureLayers(w, h) {
  if (!baseCanvas) baseCanvas = document.createElement('canvas')
  if (!paintCanvas) paintCanvas = document.createElement('canvas')
  if (!maskCanvas) maskCanvas = document.createElement('canvas')
  for (const c of [baseCanvas, paintCanvas, maskCanvas]) {
    c.width = w
    c.height = h
  }
  // mask default white (keep all)
  const mctx = maskCanvas.getContext('2d')
  mctx.fillStyle = '#ffffff'
  mctx.fillRect(0, 0, w, h)
}

async function loadImageBitmap(src) {
  const img = new Image()
  img.crossOrigin = 'anonymous'
  await new Promise((res, rej) => {
    img.onload = res
    img.onerror = rej
    img.src = src
  })
  return img
}

async function loadFromDataUrl(dataUrl, jobId) {
  error.value = ''
  const img = await loadImageBitmap(dataUrl)
  imgNatural = { w: img.naturalWidth || img.width, h: img.naturalHeight || img.height }
  ensureLayers(imgNatural.w, imgNatural.h)
  const bctx = baseCanvas.getContext('2d')
  bctx.clearRect(0, 0, imgNatural.w, imgNatural.h)
  bctx.drawImage(img, 0, 0)
  paintCanvas.getContext('2d').clearRect(0, 0, imgNatural.w, imgNatural.h)
  const mctx = maskCanvas.getContext('2d')
  mctx.fillStyle = '#ffffff'
  mctx.fillRect(0, 0, imgNatural.w, imgNatural.h)
  hasImage.value = true
  sourceJobId.value = jobId || null
  lastJobId.value = jobId || lastJobId.value
  exposure.value = 0
  contrast.value = 0
  saturation.value = 0
  await nextTick()
  fitCanvas()
  render()
  history = []
  historyIndex = -1
  pushHistory('open')
}

async function onFile(e) {
  const f = e.target.files?.[0]
  if (!f) return
  const url = URL.createObjectURL(f)
  try {
    await loadFromDataUrl(url, null)
    toastOk('Image opened')
  } catch (err) {
    toastErr(err.message || String(err))
  } finally {
    URL.revokeObjectURL(url)
  }
}

async function loadJob(jobId) {
  if (!jobId) return
  try {
    const t = getToken()
    const url = api.libraryContentUrl(jobId) + (t ? `?token=${encodeURIComponent(t)}` : '')
    const res = await fetch(url)
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const blob = await res.blob()
    const dataUrl = await new Promise((resolve, reject) => {
      const r = new FileReader()
      r.onload = () => resolve(r.result)
      r.onerror = reject
      r.readAsDataURL(blob)
    })
    await loadFromDataUrl(dataUrl, jobId)
    toastOk(`Loaded ${jobId.slice(0, 10)}…`)
  } catch (e) {
    error.value = e.message
    toastErr(e.message)
  }
}

function fitCanvas() {
  const c = canvas.value
  if (!c || !imgNatural.w) return
  // display size capped
  const maxW = Math.min(900, c.parentElement?.clientWidth - 24 || 800)
  const scale = Math.min(1, maxW / imgNatural.w)
  c.width = imgNatural.w
  c.height = imgNatural.h
  c.style.width = `${Math.round(imgNatural.w * scale)}px`
  c.style.height = `${Math.round(imgNatural.h * scale)}px`
}

function applyAdjustToImageData(id) {
  const d = id.data
  const exp = Math.pow(2, exposure.value)
  const con = contrast.value
  const sat = saturation.value
  const cf = (1 + con) * (con < 0 ? 1 : 1)
  for (let i = 0; i < d.length; i += 4) {
    let r = d[i] / 255
    let g = d[i + 1] / 255
    let b = d[i + 2] / 255
    // exposure
    r *= exp
    g *= exp
    b *= exp
    // contrast around 0.5
    r = (r - 0.5) * (1 + con * 1.5) + 0.5
    g = (g - 0.5) * (1 + con * 1.5) + 0.5
    b = (b - 0.5) * (1 + con * 1.5) + 0.5
    // saturation
    const gray = 0.2126 * r + 0.7152 * g + 0.0722 * b
    r = gray + (r - gray) * (1 + sat)
    g = gray + (g - gray) * (1 + sat)
    b = gray + (b - gray) * (1 + sat)
    d[i] = Math.max(0, Math.min(255, r * 255))
    d[i + 1] = Math.max(0, Math.min(255, g * 255))
    d[i + 2] = Math.max(0, Math.min(255, b * 255))
  }
  void cf
}

function render() {
  const c = canvas.value
  if (!c || !baseCanvas || !hasImage.value) return
  const ctx = c.getContext('2d')
  const w = imgNatural.w
  const h = imgNatural.h
  // adjusted base
  const tmp = document.createElement('canvas')
  tmp.width = w
  tmp.height = h
  const tctx = tmp.getContext('2d')
  tctx.drawImage(baseCanvas, 0, 0)
  if (exposure.value || contrast.value || saturation.value) {
    const id = tctx.getImageData(0, 0, w, h)
    applyAdjustToImageData(id)
    tctx.putImageData(id, 0, 0)
  }
  ctx.clearRect(0, 0, w, h)
  ctx.drawImage(tmp, 0, 0)
  ctx.drawImage(paintCanvas, 0, 0)
  // mask overlay (semi-transparent red where mask is dark)
  if (tool.value === 'mask') {
    ctx.save()
    ctx.globalAlpha = 0.35
    // show inverse: paint red where mask < 200
    const mid = maskCanvas.getContext('2d').getImageData(0, 0, w, h)
    const ov = ctx.createImageData(w, h)
    for (let i = 0; i < mid.data.length; i += 4) {
      const v = mid.data[i]
      if (v < 200) {
        ov.data[i] = 220
        ov.data[i + 1] = 40
        ov.data[i + 2] = 40
        ov.data[i + 3] = 180
      }
    }
    const ovc = document.createElement('canvas')
    ovc.width = w
    ovc.height = h
    ovc.getContext('2d').putImageData(ov, 0, 0)
    ctx.drawImage(ovc, 0, 0)
    ctx.restore()
  }
  if (cropRect.value) {
    const r = cropRect.value
    ctx.save()
    ctx.fillStyle = 'rgba(0,0,0,0.45)'
    ctx.fillRect(0, 0, w, h)
    ctx.clearRect(r.x, r.y, r.w, r.h)
    ctx.strokeStyle = '#d4a84b'
    ctx.lineWidth = 2
    ctx.strokeRect(r.x, r.y, r.w, r.h)
    ctx.restore()
  }
}

function canvasPt(e) {
  const c = canvas.value
  const rect = c.getBoundingClientRect()
  const x = ((e.clientX - rect.left) / rect.width) * c.width
  const y = ((e.clientY - rect.top) / rect.height) * c.height
  return { x, y }
}

function brushStroke(ctx, x0, y0, x1, y1, color, erase = false) {
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  ctx.lineWidth = brushSize.value
  ctx.strokeStyle = color
  ctx.globalAlpha = hardness.value
  if (erase) {
    ctx.globalCompositeOperation = 'destination-out'
    ctx.strokeStyle = 'rgba(0,0,0,1)'
  } else {
    ctx.globalCompositeOperation = 'source-over'
  }
  ctx.beginPath()
  ctx.moveTo(x0, y0)
  ctx.lineTo(x1, y1)
  ctx.stroke()
  ctx.globalCompositeOperation = 'source-over'
  ctx.globalAlpha = 1
}

function onDown(e) {
  if (!hasImage.value) return
  const p = canvasPt(e)
  drawing = true
  lastPt = p
  canvas.value.setPointerCapture?.(e.pointerId)
  if (tool.value === 'text') {
    const ctx = paintCanvas.getContext('2d')
    ctx.font = `bold ${textSize.value}px "IBM Plex Sans", sans-serif`
    ctx.fillStyle = textColor.value
    ctx.textBaseline = 'top'
    ctx.fillText(textValue.value || ' ', p.x, p.y)
    drawing = false
    render()
    pushHistory('text')
    return
  }
  if (tool.value === 'crop') {
    cropStart = p
    cropRect.value = { x: p.x, y: p.y, w: 0, h: 0 }
    return
  }
  if (tool.value === 'paint' || tool.value === 'eraser' || tool.value === 'mask') {
    strokeDirty = true
    const ctx =
      tool.value === 'mask' ? maskCanvas.getContext('2d') : paintCanvas.getContext('2d')
    const col = tool.value === 'mask' ? '#000000' : brushColor.value
    const erase = tool.value === 'eraser' || (tool.value === 'mask' && e.shiftKey)
    brushStroke(ctx, p.x, p.y, p.x + 0.01, p.y + 0.01, col, erase || tool.value === 'eraser')
    render()
  }
}

function onMove(e) {
  if (!drawing || !lastPt) return
  const p = canvasPt(e)
  if (tool.value === 'crop' && cropStart) {
    cropRect.value = {
      x: Math.min(cropStart.x, p.x),
      y: Math.min(cropStart.y, p.y),
      w: Math.abs(p.x - cropStart.x),
      h: Math.abs(p.y - cropStart.y),
    }
    render()
    lastPt = p
    return
  }
  if (tool.value === 'paint' || tool.value === 'eraser' || tool.value === 'mask') {
    const ctx =
      tool.value === 'mask' ? maskCanvas.getContext('2d') : paintCanvas.getContext('2d')
    const col = tool.value === 'mask' ? (e.shiftKey ? '#ffffff' : '#000000') : brushColor.value
    brushStroke(ctx, lastPt.x, lastPt.y, p.x, p.y, col, tool.value === 'eraser')
    render()
  }
  lastPt = p
}

function onUp() {
  if (strokeDirty) {
    pushHistory('stroke')
    strokeDirty = false
  }
  drawing = false
  lastPt = null
  cropStart = null
}

function resetAdjust() {
  if (exposure.value || contrast.value || saturation.value) pushHistory('adjust-before-reset')
  exposure.value = 0
  contrast.value = 0
  saturation.value = 0
  render()
}

function rotate90() {
  if (!hasImage.value) return
  const w = imgNatural.w
  const h = imgNatural.h
  const rot = (src) => {
    const out = document.createElement('canvas')
    out.width = h
    out.height = w
    const ctx = out.getContext('2d')
    ctx.translate(h, 0)
    ctx.rotate(Math.PI / 2)
    ctx.drawImage(src, 0, 0)
    return out
  }
  baseCanvas = rot(baseCanvas)
  paintCanvas = rot(paintCanvas)
  maskCanvas = rot(maskCanvas)
  imgNatural = { w: h, h: w }
  fitCanvas()
  render()
  pushHistory('rotate')
}

function flipH() {
  flip(-1, 1)
}
function flipV() {
  flip(1, -1)
}
function flip(sx, sy) {
  if (!hasImage.value) return
  const w = imgNatural.w
  const h = imgNatural.h
  const doFlip = (src) => {
    const out = document.createElement('canvas')
    out.width = w
    out.height = h
    const ctx = out.getContext('2d')
    ctx.translate(sx < 0 ? w : 0, sy < 0 ? h : 0)
    ctx.scale(sx, sy)
    ctx.drawImage(src, 0, 0)
    return out
  }
  baseCanvas = doFlip(baseCanvas)
  paintCanvas = doFlip(paintCanvas)
  maskCanvas = doFlip(maskCanvas)
  render()
  pushHistory('flip')
}

function setAspectCrop(ratio) {
  if (!hasImage.value) return
  const w = imgNatural.w
  const h = imgNatural.h
  let cw = w
  let ch = w / ratio
  if (ch > h) {
    ch = h
    cw = h * ratio
  }
  cropRect.value = {
    x: (w - cw) / 2,
    y: (h - ch) / 2,
    w: cw,
    h: ch,
  }
  render()
}

function applyCrop() {
  const r = cropRect.value
  if (!r || r.w < 2 || r.h < 2) return
  const x = Math.max(0, Math.floor(r.x))
  const y = Math.max(0, Math.floor(r.y))
  const w = Math.min(imgNatural.w - x, Math.floor(r.w))
  const h = Math.min(imgNatural.h - y, Math.floor(r.h))
  const cropLayer = (src) => {
    const out = document.createElement('canvas')
    out.width = w
    out.height = h
    out.getContext('2d').drawImage(src, x, y, w, h, 0, 0, w, h)
    return out
  }
  baseCanvas = cropLayer(baseCanvas)
  paintCanvas = cropLayer(paintCanvas)
  maskCanvas = cropLayer(maskCanvas)
  imgNatural = { w, h }
  cropRect.value = null
  fitCanvas()
  render()
  pushHistory('crop')
  toastOk('Crop applied')
}

function compositeExport(includeMaskFade = false) {
  const w = imgNatural.w
  const h = imgNatural.h
  const out = document.createElement('canvas')
  out.width = w
  out.height = h
  const ctx = out.getContext('2d')
  const tmp = document.createElement('canvas')
  tmp.width = w
  tmp.height = h
  const tctx = tmp.getContext('2d')
  tctx.drawImage(baseCanvas, 0, 0)
  if (exposure.value || contrast.value || saturation.value) {
    const id = tctx.getImageData(0, 0, w, h)
    applyAdjustToImageData(id)
    tctx.putImageData(id, 0, 0)
  }
  ctx.drawImage(tmp, 0, 0)
  ctx.drawImage(paintCanvas, 0, 0)
  if (includeMaskFade) {
    // multiply by mask luminance
    const img = ctx.getImageData(0, 0, w, h)
    const mask = maskCanvas.getContext('2d').getImageData(0, 0, w, h)
    for (let i = 0; i < img.data.length; i += 4) {
      const m = mask.data[i] / 255
      img.data[i + 3] = Math.round(img.data[i + 3] * m)
    }
    ctx.putImageData(img, 0, 0)
  }
  return out
}

async function exportLibrary() {
  if (!hasImage.value) return
  busy.value = true
  error.value = ''
  emit('busy', { busy: true, label: 'export' })
  try {
    const out = compositeExport(false)
    const dataUrl = out.toDataURL(exportFmt.value, exportQ.value)
    const ext = exportFmt.value === 'image/png' ? 'png' : exportFmt.value === 'image/webp' ? 'webp' : 'jpg'
    const result = await api.libraryImport({
      data: dataUrl,
      filename: `craft.${ext}`,
      note: 'image craft export',
      source_job_id: sourceJobId.value || undefined,
    })
    lastExportId.value = result.job_id
    lastJobId.value = result.job_id
    toastOk(`Exported ${result.job_id.slice(0, 12)}…`)
    emit('done', result)
    window.dispatchEvent(new CustomEvent('imaginarium-to-jobs', { detail: result }))
  } catch (e) {
    error.value = e.message
    toastErr(e.message)
  } finally {
    busy.value = false
    emit('busy', { busy: false })
  }
}

async function exportMask() {
  if (!hasImage.value) return
  busy.value = true
  try {
    const dataUrl = maskCanvas.toDataURL('image/png')
    const result = await api.libraryImport({
      data: dataUrl,
      filename: 'mask.png',
      note: 'image craft mask',
      source_job_id: sourceJobId.value || undefined,
    })
    toastOk(`Mask saved ${result.job_id.slice(0, 12)}…`)
    emit('done', result)
  } catch (e) {
    toastErr(e.message)
  } finally {
    busy.value = false
  }
}

onMounted(() => {
  window.addEventListener('resize', () => {
    fitCanvas()
    render()
  })
  window.addEventListener('keydown', onKeyHistory)
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyHistory)
})

// Debounced history for adjust sliders (one step after you stop dragging)
let adjustTimer = null
watch([exposure, contrast, saturation], () => {
  if (suppressHistory || !hasImage.value) return
  clearTimeout(adjustTimer)
  adjustTimer = setTimeout(() => pushHistory('adjust'), 350)
})

defineExpose({ loadJob })
</script>

<style scoped>
.craft {
  display: grid;
  grid-template-columns: 300px 1fr;
  gap: 1rem;
  align-items: start;
}
@media (max-width: 900px) {
  .craft {
    grid-template-columns: 1fr;
  }
}
.tools h2 {
  margin: 0 0 0.25rem;
}
.tip {
  margin: 0 0 0.75rem;
  font-size: 0.82rem;
}
.tool-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-bottom: 0.85rem;
}
.tab {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
  border-radius: 999px;
  padding: 0.3rem 0.65rem;
  cursor: pointer;
  font-size: 0.85rem;
}
.tab.active {
  color: var(--gold);
  border-color: var(--gold-dim);
}
.transforms {
  margin: 0.75rem 0;
}
.stage {
  min-height: 360px;
  overflow: auto;
  display: flex;
  align-items: center;
  justify-content: center;
}
.board {
  max-width: 100%;
  background: #0a0c10;
  border-radius: 8px;
  touch-action: none;
}
.board.drawing {
  cursor: crosshair;
}
.empty {
  padding: 2rem;
  text-align: center;
}
input[type='range'] {
  width: 100%;
}
input[type='color'] {
  width: 100%;
  height: 2rem;
  border: none;
  background: transparent;
}
</style>
