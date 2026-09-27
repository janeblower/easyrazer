<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import Keyboard from './components/Keyboard.vue'
import Panel from './components/Panel.vue'
import Status from './components/Status.vue'
import Lighting from './components/Lighting.vue'
import Settings from './components/Settings.vue'
import CloseDialog from './components/CloseDialog.vue'
import AutostartOffer from './components/AutostartOffer.vue'

const status = ref(null)
const layout = ref([])
const baseline = ref({}) // fwID -> mm, as last read from the keyboard
const edits = ref({}) // fwID -> mm, not applied yet
const errors = ref({}) // fwID -> message from the last apply
const selection = ref(new Set())
const progress = ref(null)
const busy = ref(false)
const message = ref('')
const tab = ref('actuation')
const closing = ref(false)
const offering = ref(false)
let loadedProfile = null // profile the baseline was read from
let unlistenStatus
let unlistenClose

const dirty = computed(() => Object.keys(edits.value).length)
const canApply = computed(() => !!status.value?.device && !status.value?.synapse && !busy.value && dirty.value > 0)
const selectedValue = computed(() => {
  const values = [...selection.value].map(k => edits.value[k] ?? baseline.value[k])
  return values.length && values.every(v => v === values[0]) ? values[0] ?? null : null
})

async function refresh() {
  try {
    await onStatus(await invoke('status'))
  } catch (e) {
    message.value = String(e)
  }
}

async function onStatus(s) {
  // Mid read or apply the table is being rewritten; the next event brings a fresh status anyway.
  if (busy.value) return
  status.value = s
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
    const base = await invoke('read_all')
    baseline.value = base
    edits.value = Object.fromEntries(Object.entries(edits.value).filter(([k, mm]) => base[k] !== mm))
    errors.value = {}
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
  if (failed) await refresh()
}

async function onClose(action, remember) {
  closing.value = false
  try {
    if (remember) await invoke('set_close_action', { action })
    await invoke(action === 'tray' ? 'hide_window' : 'quit')
  } catch (e) {
    message.value = String(e)
  }
}

async function onOffer(on) {
  offering.value = false
  try {
    await invoke('autostart_answered', { on })
  } catch (e) {
    message.value = String(e)
  }
}

onMounted(async () => {
  unlistenClose = await listen('close-requested', () => (closing.value = true))
  try {
    offering.value = !(await invoke('app_settings')).autostart_offered
  } catch (e) {
    message.value = String(e)
  }
  layout.value = await invoke('layout')
  unlistenStatus = await listen('status', e => onStatus(e.payload))
  await refresh()
})

onUnmounted(() => {
  unlistenStatus?.()
  unlistenClose?.()
})
</script>

<template>
  <main>
    <nav class="tabs">
      <button :class="{ on: tab === 'actuation' }" @click="tab = 'actuation'">Актуация</button>
      <button :class="{ on: tab === 'lighting' }" @click="tab = 'lighting'">Подсветка</button>
      <button :class="{ on: tab === 'settings' }" @click="tab = 'settings'">Настройки</button>
    </nav>
    <Status :status="status" :progress="progress" :message="tab !== 'lighting' ? message : ''" />
    <template v-if="tab === 'actuation'">
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
    </template>
    <Lighting v-else-if="tab === 'lighting'" :status="status" />
    <Settings v-else />
    <CloseDialog v-if="closing" @choose="onClose" @cancel="closing = false" />
    <AutostartOffer v-if="offering && !closing" @answer="onOffer" @later="offering = false" />
    <footer>EasyRazer — неофициальный проект, не связан с Razer Inc.</footer>
  </main>
</template>

<style scoped>
main { display: flex; flex-direction: column; gap: 16px; padding: 16px; }
footer { color: var(--muted); font-size: 12px; }
.tabs { display: flex; gap: 4px; }
.tabs button { border-radius: 6px 6px 0 0; background: var(--panel); color: var(--muted); border-color: transparent; }
.tabs button.on { background: var(--key); color: var(--text); border-bottom: 2px solid var(--accent); }
</style>
