<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type { AppSettings } from "../types";

const s = ref<AppSettings | null>(null);
const message = ref("");
let unlisten: UnlistenFn | undefined;
let unmounted = false;

async function load() {
  try {
    s.value = await invoke<AppSettings>("app_settings");
  } catch (error) {
    message.value = String(error);
  }
}

async function set(cmd: string, args: Record<string, unknown>) {
  message.value = "";
  try {
    await invoke(cmd, args);
  } catch (error) {
    message.value = String(error);
  }
  await load();
}

const checked = (e: Event) => (e.target as HTMLInputElement).checked;

onMounted(async () => {
  unlisten = await listen("settings-changed", load);
  // Switched away before listen() resolved: onUnmounted has already run.
  if (unmounted) {
    unlisten();
    return;
  }
  await load();
});
onUnmounted(() => {
  unmounted = true;
  unlisten?.();
});
</script>

<template>
  <section v-if="s" class="settings">
    <label
      ><input type="checkbox" :checked="s.autostart" @change="set('set_autostart', { on: checked($event) })" /> Запускать с Windows</label
    >
    <label>
      При закрытии окна:
      <select :value="s.close_action" @change="set('set_close_action', { action: ($event.target as HTMLSelectElement).value })">
        <option value="ask">Спрашивать</option>
        <option value="tray">Сворачивать в трей</option>
        <option value="exit">Закрывать программу</option>
      </select>
    </label>
    <label
      ><input type="checkbox" :checked="s.watch_synapse" @change="set('set_watch_synapse', { on: checked($event) })" /> Следить за
      Synapse</label
    >
    <label
      ><input type="checkbox" :checked="s.confirm_write" @change="set('set_confirm_write', { on: checked($event) })" /> Спрашивать перед
      записью в память клавиатуры</label
    >
    <p v-if="message" class="err">{{ message }}</p>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  background: var(--panel);
  border-radius: 8px;
}
.err {
  color: var(--error);
  margin: 0;
}
</style>
