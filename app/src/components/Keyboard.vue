<script setup>
import { ref, computed } from 'vue'

const props = defineProps({
  layout: Array,
  baseline: Object,
  edits: Object,
  errors: Object,
  selection: Set,
})
const emit = defineEmits(['update:selection'])

const U = 50
const root = ref(null)
const band = ref(null)
let start = null
let additive = false

const size = computed(() => ({
  width: Math.max(0, ...props.layout.map(k => k.x + k.w)) * U + 'px',
  height: Math.max(0, ...props.layout.map(k => k.y + k.h)) * U + 'px',
}))

const bandStyle = computed(() => {
  const b = norm(band.value)
  return { left: b.x0 + 'px', top: b.y0 + 'px', width: b.x1 - b.x0 + 'px', height: b.y1 - b.y0 + 'px' }
})

function value(key) {
  return props.edits[key] ?? props.baseline[key]
}

function point(e) {
  const r = root.value.getBoundingClientRect()
  return [e.clientX - r.left, e.clientY - r.top]
}

function norm(b) {
  return { x0: Math.min(b.x0, b.x1), y0: Math.min(b.y0, b.y1), x1: Math.max(b.x0, b.x1), y1: Math.max(b.y0, b.y1) }
}

function down(e) {
  start = point(e)
  additive = e.ctrlKey
  root.value.setPointerCapture(e.pointerId)
}

function move(e) {
  if (!start) return
  const [x, y] = point(e)
  if (band.value || Math.hypot(x - start[0], y - start[1]) > 4) {
    band.value = { x0: start[0], y0: start[1], x1: x, y1: y }
  }
}

function up(e) {
  if (!start) return
  const next = additive ? new Set(props.selection) : new Set()
  if (band.value) {
    const b = norm(band.value)
    for (const k of props.layout) {
      const hit = k.x * U < b.x1 && (k.x + k.w) * U > b.x0 && k.y * U < b.y1 && (k.y + k.h) * U > b.y0
      if (k.editable && hit) next.add(k.key)
    }
  } else {
    // Pointer capture retargets events to the root, so find the key under the cursor explicitly.
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-key]')
    const k = el && props.layout.find(k => k.key === Number(el.dataset.key))
    if (k?.editable) {
      if (additive && next.has(k.key)) next.delete(k.key)
      else next.add(k.key)
    }
  }
  start = null
  band.value = null
  emit('update:selection', next)
}
</script>

<template>
  <div ref="root" class="kb" :style="size" @pointerdown="down" @pointermove="move" @pointerup="up">
    <div
      v-for="k in layout"
      :key="k.key"
      :data-key="k.key"
      class="key"
      :class="{ sel: selection.has(k.key), edited: k.key in edits, err: k.key in errors, off: !k.editable }"
      :style="{ left: k.x * U + 'px', top: k.y * U + 'px', width: k.w * U - 4 + 'px', height: k.h * U - 4 + 'px' }"
      :title="errors[k.key]"
    >
      <span class="label">{{ k.label }}</span>
      <span v-if="k.editable && value(k.key) != null" class="mm">{{ value(k.key).toFixed(1) }}</span>
    </div>
    <div v-if="band" class="band" :style="bandStyle"></div>
  </div>
</template>

<style scoped>
.kb { position: relative; }
.key {
  position: absolute;
  background: var(--key);
  border: 2px solid transparent;
  border-radius: 6px;
  padding: 3px 5px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  cursor: pointer;
}
.key.off { background: var(--key-off); color: var(--muted); cursor: default; }
.key.sel { border-color: var(--accent); }
.key.edited .mm { color: var(--edited); font-weight: 600; }
.key.err { border-color: var(--error); }
.label { font-size: 12px; }
.mm { font-size: 11px; color: var(--muted); align-self: flex-end; }
.band { position: absolute; border: 1px dashed var(--accent); background: rgba(68, 214, 44, 0.08); pointer-events: none; }
</style>
