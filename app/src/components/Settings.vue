<script setup>
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const s = ref(null)
const message = ref('')
let unlisten

async function load() {
  try {
    s.value = await invoke('app_settings')
  } catch (e) {
    message.value = String(e)
  }
}

async function set(cmd, args) {
  message.value = ''
  try {
    await invoke(cmd, args)
  } catch (e) {
    message.value = String(e)
  }
  await load()
}

onMounted(async () => {
  unlisten = await listen('settings-changed', load)
  await load()
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <section v-if="s" class="settings">
    <label><input type="checkbox" :checked="s.autostart" @change="set('set_autostart', { on: $event.target.checked })" /> Запускать с Windows</label>
    <label>
      При закрытии окна:
      <select :value="s.close_action" @change="set('set_close_action', { action: $event.target.value })">
        <option value="ask">Спрашивать</option>
        <option value="tray">Сворачивать в трей</option>
        <option value="exit">Закрывать программу</option>
      </select>
    </label>
    <label><input type="checkbox" :checked="s.watch_synapse" @change="set('set_watch_synapse', { on: $event.target.checked })" /> Следить за Synapse</label>
    <label><input type="checkbox" :checked="s.confirm_write" @change="set('set_confirm_write', { on: $event.target.checked })" /> Спрашивать перед записью в память клавиатуры</label>
    <p v-if="message" class="err">{{ message }}</p>
  </section>
</template>

<style scoped>
.settings { display: flex; flex-direction: column; gap: 12px; padding: 16px; background: var(--panel); border-radius: 8px; }
.err { color: var(--error); margin: 0; }
</style>
