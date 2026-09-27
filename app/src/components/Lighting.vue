<script setup>
import { ref, computed, watch, nextTick, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import ColorPicker from './ColorPicker.vue'
import ConfirmWrite from './ConfirmWrite.vue'

const props = defineProps({ status: Object })

const GROUPS = {
  off: 'Выключено',
  static: 'Статичный',
  breathing: 'Дыхание',
  spectrum: 'Спектр',
  wave: 'Волна',
  reactive: 'Отклик',
  starlight: 'Звёздное небо',
}
const TYPES = { key: 'Клавиша', ripple: 'Рябь' }
const COLOR_VARIANTS = ['one', 'two', 'random']
const DIRS = { left: '← влево', right: 'вправо →' }
const DEFAULT_COLORS = [[0x44, 0xd6, 0x2c], [0x00, 0x40, 0xff]]

const effects = ref([])
const applied = ref(null)
const saved = ref(null)
// What the controls show; the effect sent to the keyboard is derived from it.
const ui = ref(null)
const dynamicLighting = ref(false)
const confirmWrite = ref(true)
const asking = ref(false)
const busy = ref(false)
const message = ref('')
let loading = false

// Effect names are `group[_type][_one|_two|_random]`.
function parse(name) {
  const parts = name.split('_')
  const variant = COLOR_VARIANTS.includes(parts.at(-1)) ? parts.pop() : null
  return { group: parts[0], type: parts[1] ?? null, variant }
}

const blocked = computed(() => !props.status || props.status.synapse)
const connected = computed(() => !!props.status?.device && !blocked.value)
const groups = computed(() => {
  const out = []
  for (const e of effects.value) {
    const p = parse(e.name)
    let g = out.find(g => g.group === p.group)
    if (!g) out.push((g = { group: p.group, label: GROUPS[p.group] ?? p.group, types: [], effects: [] }))
    if (p.type && !g.types.includes(p.type)) g.types.push(p.type)
    g.effects.push({ ...e, ...p })
  }
  return out
})

function candidates(s) {
  return groups.value.find(g => g.group === s.group)?.effects.filter(e => e.type === s.type) ?? []
}

const currentGroup = computed(() => ui.value && groups.value.find(g => g.group === ui.value.group))
const cands = computed(() => (ui.value ? candidates(ui.value) : []))
const slots = computed(() => Math.max(0, ...cands.value.map(e => e.colors)))
const hasRandom = computed(() => cands.value.some(e => e.variant === 'random'))
const randomOn = computed(() => ui.value.random && hasRandom.value)

// One set color picks the one-color effect, two pick the two-color one, none turn the lighting off.
function resolve(s) {
  const cs = candidates(s)
  if (!cs.length) return null
  const set = s.colors.slice(0, Math.max(0, ...cs.map(e => e.colors))).filter(Boolean)
  let e = cs[0]
  if (s.random && cs.some(c => c.variant === 'random')) e = cs.find(c => c.variant === 'random')
  else if (cs.some(c => c.colors)) {
    if (!set.length) return { effect: { name: 'off' }, brightness: s.brightness }
    e = cs.find(c => c.colors === set.length) ?? cs.find(c => c.colors === 1)
  }
  const effect = { name: e.name }
  if (e.colors >= 1) effect.rgb1 = set[0]
  if (e.colors >= 2) effect.rgb2 = set[1]
  if (e.dirs.length) effect.dir = e.dirs.includes(s.dir) ? s.dir : e.dirs[0]
  if (e.speed) {
    const [lo, hi] = e.speed
    effect.speed = s.speed >= lo && s.speed <= hi ? s.speed : Math.round((lo + hi) / 2)
  }
  return { effect, brightness: s.brightness }
}

function stateFrom(look) {
  if (look && !effects.value.some(e => e.name === look.effect.name)) look = null
  const s = { group: null, type: null, colors: [...DEFAULT_COLORS], random: false, dir: null, speed: null, brightness: 255 }
  const name = look?.effect.name ?? effects.value[0]?.name
  if (!name) return null
  const p = parse(name)
  Object.assign(s, { group: p.group, type: p.type, random: p.variant === 'random' })
  if (look) {
    const e = look.effect
    if (e.rgb1) s.colors = [e.rgb1, e.rgb2 ?? (p.variant === 'one' ? null : DEFAULT_COLORS[1])]
    Object.assign(s, { dir: e.dir ?? null, speed: e.speed ?? null, brightness: look.brightness })
  }
  return s
}

const draft = computed(() => ui.value && resolve(ui.value))
const dirty = computed(() => !!draft.value && !same(draft.value, applied.value ?? saved.value))
const canSave = computed(() => connected.value && !busy.value && !!draft.value && !same(draft.value, saved.value))

// The backend omits unset effect fields; compare looks by the fields that are set.
function norm(look) {
  if (!look) return null
  const effect = { name: look.effect.name }
  for (const k of ['rgb1', 'rgb2', 'dir', 'speed']) if (look.effect[k] != null) effect[k] = look.effect[k]
  return { effect, brightness: look.brightness }
}

function same(a, b) {
  return JSON.stringify(norm(a)) === JSON.stringify(norm(b))
}

function setUi(k, v) {
  ui.value = { ...ui.value, [k]: v }
}

function pickGroup(g) {
  setUi('group', g.group)
  setUi('type', g.types.includes(ui.value.type) ? ui.value.type : (g.types[0] ?? null))
}

function setColor(i, rgb) {
  const colors = [...ui.value.colors]
  colors[i] = rgb
  setUi('colors', colors)
}

const percent = computed({
  get: () => Math.round((ui.value.brightness / 255) * 100),
  set: p => setUi('brightness', Math.round((p * 255) / 100)),
})

// Taken from the group, not the resolved effect: with no colors the effect is "off" but the controls stay.
const dirInfo = computed(() => cands.value.find(e => e.dirs.length))
const dirOf = computed(() => (dirInfo.value.dirs.includes(ui.value.dir) ? ui.value.dir : dirInfo.value.dirs[0]))
const speedInfo = computed(() => cands.value.find(e => e.speed))

// Some firmware speeds are "lower is faster"; the slider always reads slow → fast.
const speedSlider = computed({
  get: () => {
    const { speed: [lo, hi], fast_low } = speedInfo.value
    const s = ui.value.speed >= lo && ui.value.speed <= hi ? ui.value.speed : Math.round((lo + hi) / 2)
    return fast_low ? lo + hi - s : s
  },
  set: v => {
    const { speed: [lo, hi], fast_low } = speedInfo.value
    setUi('speed', fast_low ? lo + hi - v : v)
  },
})

let pending = null
let sending = false

// One preview in flight, at most one per 50 ms; the newest draft wins. Parallel calls
// would race for the device lock and could land out of order.
async function preview() {
  pending = norm(draft.value)
  if (sending) return
  sending = true
  while (pending) {
    await new Promise(done => setTimeout(done, 50))
    const look = pending
    pending = null
    try {
      await invoke('lighting_preview', { look })
    } catch (e) {
      message.value = String(e)
    }
  }
  sending = false
}

watch(draft, (now, before) => {
  if (!loading && connected.value && now && !same(now, before)) preview()
})

async function load() {
  try {
    const s = await invoke('lighting_state')
    effects.value = s.effects
    applied.value = s.applied
    saved.value = s.saved
    dynamicLighting.value = s.dynamic_lighting
    confirmWrite.value = s.confirm_write
    loading = true
    ui.value = stateFrom(s.applied ?? s.saved)
    await nextTick()
    loading = false
    message.value = ''
  } catch (e) {
    message.value = String(e)
  }
}

function revert() {
  ui.value = stateFrom(applied.value ?? saved.value)
}

async function apply() {
  busy.value = true
  try {
    const look = norm(draft.value)
    await invoke('lighting_apply', { look })
    applied.value = look
    message.value = 'Применено'
  } catch (e) {
    message.value = String(e)
  } finally {
    busy.value = false
  }
}

async function write() {
  busy.value = true
  try {
    const look = norm(draft.value)
    await invoke('lighting_write', { look })
    applied.value = look
    saved.value = look
    message.value = 'Записано в память клавиатуры'
  } catch (e) {
    message.value = String(e)
  } finally {
    busy.value = false
  }
}

async function save() {
  if (confirmWrite.value) asking.value = true
  else await write()
}

async function onConfirm(dontAsk) {
  asking.value = false
  if (dontAsk) {
    try {
      await invoke('set_confirm_write', { on: false })
      confirmWrite.value = false
    } catch (e) {
      message.value = String(e)
    }
  }
  await write()
}

async function toggleDynamic() {
  const on = !dynamicLighting.value
  try {
    await invoke('set_dynamic_lighting', { on })
    dynamicLighting.value = on
  } catch (e) {
    message.value = String(e)
  }
}

watch(connected, ok => ok && load())
onMounted(load)
</script>

<template>
  <section class="lighting">
    <div v-if="dynamicLighting" class="warn">
      ⚠ Подсветкой управляет динамическое освещение Windows — эффекты клавиатуры не видны.
      <button :disabled="blocked" @click="toggleDynamic">Отключить</button>
    </div>
    <p v-if="message" class="msg">{{ message }}</p>
    <p v-if="!draft" class="msg">Подключите клавиатуру, чтобы увидеть её эффекты.</p>
    <div v-else class="body" :class="{ blocked }">
      <ul class="fx">
        <li v-for="g in groups" :key="g.group" :class="{ on: currentGroup === g }" @click="!blocked && pickGroup(g)">
          {{ g.label }}
        </li>
      </ul>
      <div class="params">
        <div v-if="currentGroup?.types.length" class="row">
          <span class="lbl">Тип</span>
          <span class="seg">
            <button v-for="t in currentGroup.types" :key="t" :class="{ on: ui.type === t }" :disabled="blocked" @click="setUi('type', t)">
              {{ TYPES[t] ?? t }}
            </button>
          </span>
        </div>
        <div v-if="slots" class="row">
          <span class="lbl">{{ slots > 1 ? 'Цвета' : 'Цвет' }}</span>
          <ColorPicker
            v-for="i in slots"
            :key="i"
            :model-value="ui.colors[i - 1]"
            :disabled="blocked || randomOn"
            @update:model-value="setColor(i - 1, $event)"
          />
          <button
            v-if="hasRandom"
            class="icon shuffle"
            :class="{ on: ui.random }"
            :aria-pressed="ui.random"
            title="Случайные цвета"
            aria-label="Случайные цвета"
            :disabled="blocked"
            @click="setUi('random', !ui.random)"
          >
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M2 18h1.4c1.3 0 2.5-.6 3.3-1.7l6.1-8.6c.7-1.1 2-1.7 3.3-1.7H22" />
              <path d="m18 2 4 4-4 4" />
              <path d="M2 6h1.9c1.5 0 2.9.9 3.6 2.2" />
              <path d="M22 18h-5.9c-1.3 0-2.6-.7-3.3-1.8l-.5-.8" />
              <path d="m18 14 4 4-4 4" />
            </svg>
          </button>
        </div>
        <div v-if="dirInfo" class="row">
          <span class="lbl">Направление</span>
          <span class="seg">
            <button v-for="d in dirInfo.dirs" :key="d" :class="{ on: dirOf === d }" :disabled="blocked" @click="setUi('dir', d)">
              {{ DIRS[d] ?? d }}
            </button>
          </span>
        </div>
        <div v-if="speedInfo" class="row">
          <span class="lbl">Скорость</span>
          <span class="hint">медленно</span>
          <input v-model.number="speedSlider" type="range" :min="speedInfo.speed[0]" :max="speedInfo.speed[1]" step="1" :disabled="blocked" />
          <span class="hint">быстро</span>
        </div>
        <p v-if="ui.group === 'off'" class="msg">Подсветка выключена.</p>
        <div v-else class="row">
          <span class="lbl">Яркость</span>
          <input v-model.number="percent" type="range" min="0" max="100" step="1" :disabled="blocked" />
          <span>{{ percent }}%</span>
        </div>
      </div>
    </div>
    <div class="foot">
      <label class="switch">
        <input type="checkbox" role="switch" :checked="dynamicLighting" :disabled="blocked" @change="toggleDynamic" />
        Динамическое освещение Windows
      </label>
      <span class="spacer"></span>
      <span v-if="dirty" class="dirty">● не применено</span>
      <button class="icon" title="Отменить изменения" aria-label="Отменить изменения" :disabled="!dirty || busy || blocked" @click="revert">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M9 14 4 9l5-5" />
          <path d="M4 9h11a5 5 0 0 1 0 10h-3" />
        </svg>
      </button>
      <button class="primary" :disabled="!dirty || busy || blocked" @click="apply">Применить</button>
      <button class="icon save" title="Записать в память клавиатуры" aria-label="Записать в память клавиатуры" :disabled="!canSave" @click="save">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
          <path d="M7 3v5h8V3" />
          <path d="M7 21v-7h10v7" />
        </svg>
      </button>
    </div>
    <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
  </section>
</template>

<style scoped>
.lighting { display: flex; flex-direction: column; gap: 12px; }
.warn { background: #3a2a10; color: #ffcf7a; padding: 8px 12px; border-radius: 6px; display: flex; align-items: center; gap: 12px; }
.msg { margin: 0; color: var(--muted); }
.body { display: flex; gap: 12px; }
.body.blocked { opacity: 0.4; pointer-events: none; }
.fx { list-style: none; margin: 0; padding: 10px; width: 210px; background: var(--panel); border-radius: 6px; }
.fx li { padding: 8px 10px; margin-bottom: 4px; border-radius: 6px; background: var(--key); border: 2px solid transparent; cursor: pointer; }
.fx li.on { border-color: var(--accent); }
.params { flex: 1; padding: 10px 14px; background: var(--panel); border-radius: 6px; }
.row { display: flex; align-items: center; gap: 10px; margin: 10px 0; }
.lbl { color: var(--muted); min-width: 100px; }
.hint { color: var(--muted); font-size: 12px; }
.seg { display: inline-flex; }
.seg button { border-radius: 0; }
.seg button:first-child { border-radius: 6px 0 0 6px; }
.seg button:last-child { border-radius: 0 6px 6px 0; }
.seg button.on { background: var(--accent); color: #0b0b0b; border-color: var(--accent); }
input[type='range'] { width: 240px; accent-color: var(--accent); }
.foot { display: flex; align-items: center; gap: 8px; padding: 12px 16px; background: var(--panel); border-radius: 6px; }
.switch { display: flex; align-items: center; gap: 8px; }
.switch input { appearance: none; width: 34px; height: 18px; border-radius: 9px; background: #3a3a3a; position: relative; cursor: pointer; }
.switch input::after { content: ''; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; transition: left 0.15s; }
.switch input:checked { background: var(--accent); }
.switch input:checked::after { left: 18px; }
.spacer { flex: 1; }
.dirty { color: var(--edited); font-size: 12px; }
.icon { width: 34px; height: 32px; padding: 0; display: inline-flex; align-items: center; justify-content: center; }
.shuffle { margin-left: 6px; }
.shuffle.on { color: var(--accent); border-color: var(--accent); }
.icon.save { background: transparent; color: #ffb070; border-color: #8a5a20; }
</style>
