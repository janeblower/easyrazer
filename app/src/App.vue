<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import Keyboard from './components/Keyboard.vue'
import Panel from './components/Panel.vue'
import Status from './components/Status.vue'

const status = ref(null)
const layout = ref([])
const baseline = ref({}) // fwID -> mm, as last read from the keyboard
const edits = ref({}) // fwID -> mm, not applied yet
const errors = ref({}) // fwID -> message from the last apply
const selection = ref(new Set())
const progress = ref(null)
const busy = ref(false)
const message = ref('')
let loadedProfile = null // profile the baseline was read from
let timer

const dirty = computed(() => Object.keys(edits.value).length)
const canApply = computed(() => !!status.value?.device && !status.value?.synapse && !busy.value && dirty.value > 0)
const selectedValue = computed(() => {
  const values = [...selection.value].map(k => edits.value[k] ?? baseline.value[k])
  return values.length && values.every(v => v === values[0]) ? values[0] ?? null : null
})

async function refresh(checkSynapse) {
  // The backend holds the device lock for the whole read or apply; polls would only queue up behind it.
  if (busy.value) return
  try {
    status.value = await invoke('status', { checkSynapse })
  } catch (e) {
    message.value = String(e)
    return
  }
  if (!status.value.device || status.value.synapse) {
    loadedProfile = null
    return
  }
  if (loadedProfile !== status.value.profile && !busy.value) await load()
}

async function load() {
  busy.value = true
  const unlisten = await listen('read-progress', e => (progress.value = e.payload))
  try {
    const profile = status.value.profile
    baseline.value = await invoke('read_all')
    loadedProfile = profile
    message.value = ''
  } catch (e) {
    message.value = String(e)
  } finally {
    unlisten()
    progress.value = null
    busy.value = false
  }
}

function setValue(mm) {
  const next = { ...edits.value }
  for (const k of selection.value) {
    if (baseline.value[k] === mm) delete next[k]
    else next[k] = mm
  }
  edits.value = next
}

function selectAll() {
  selection.value = new Set(layout.value.filter(k => k.editable).map(k => k.key))
}

function revert() {
  edits.value = {}
  errors.value = {}
}

async function apply() {
  busy.value = true
  let failed = false
  try {
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm])
    const results = await invoke('apply', { changes })
    const base = { ...baseline.value }
    const next = { ...edits.value }
    const errs = {}
    for (const r of results) {
      if (r.status === 'error') {
        errs[r.key] = r.message
        continue
      }
      base[r.key] = r.mm
      delete next[r.key]
      if (r.status === 'unconfirmed') errs[r.key] = `Не подтверждено: в клавиатуре ${r.mm.toFixed(1)} мм`
    }
    baseline.value = base
    edits.value = next
    errors.value = errs
    const bad = Object.keys(errs).length
    message.value = bad ? `Не применено или не подтверждено: ${bad} клав. (наведите на красные)` : 'Применено'
  } catch (e) {
    message.value = String(e)
    failed = true
  } finally {
    busy.value = false
  }
  if (failed) await refresh(true)
}

const onFocus = () => refresh(true)

onMounted(async () => {
  layout.value = await invoke('layout')
  await refresh(true)
  timer = setInterval(() => refresh(false), 2000)
  window.addEventListener('focus', onFocus)
})

onUnmounted(() => {
  clearInterval(timer)
  window.removeEventListener('focus', onFocus)
})
</script>

<template>
  <main>
    <Status :status="status" :progress="progress" :message="message" />
    <Keyboard :layout="layout" :baseline="baseline" :edits="edits" :errors="errors" v-model:selection="selection" />
    <Panel
      :count="selection.size"
      :value="selectedValue"
      :dirty="dirty"
      :can-apply="canApply"
      :busy="busy"
      @set="setValue"
      @apply="apply"
      @revert="revert"
      @select-all="selectAll"
      @clear="selection = new Set()"
    />
    <footer>EasyRazer — неофициальный проект, не связан с Razer Inc.</footer>
  </main>
</template>

<style scoped>
main { display: flex; flex-direction: column; gap: 16px; padding: 16px; }
footer { color: var(--muted); font-size: 12px; }
</style>
