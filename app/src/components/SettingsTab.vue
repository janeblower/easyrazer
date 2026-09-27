<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type { AppSettings } from "../types";
import { messages, setLanguage } from "../i18n";

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

async function pickLanguage(e: Event) {
  const language = (e.target as HTMLSelectElement).value;
  setLanguage(language);
  await set("set_language", { language });
}

const checked = (e: Event) => (e.target as HTMLInputElement).checked;

onMounted(async () => {
  unlisten = await listen("settings-changed", () => {
    void load();
  });
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
  <section v-if="s" class="p-4 rounded-lg bg-panel flex flex-col gap-3">
    <label>
      {{ $t("settings.language") }}
      <select :value="s.language ?? $i18n.locale" @change="pickLanguage">
        <option v-for="(m, code) in messages" :key="code" :value="code">{{ m.language.name }}</option>
      </select>
    </label>
    <label
      ><input type="checkbox" :checked="s.autostart" @change="set('set_autostart', { on: checked($event) })" />
      {{ $t("settings.autostart") }}</label
    >
    <label>
      {{ $t("settings.onClose") }}
      <select :value="s.close_action" @change="set('set_close_action', { action: ($event.target as HTMLSelectElement).value })">
        <option value="ask">{{ $t("settings.ask") }}</option>
        <option value="tray">{{ $t("settings.tray") }}</option>
        <option value="exit">{{ $t("settings.exit") }}</option>
      </select>
    </label>
    <label
      ><input type="checkbox" :checked="s.watch_synapse" @change="set('set_watch_synapse', { on: checked($event) })" />
      {{ $t("settings.watchSynapse") }}</label
    >
    <label
      ><input type="checkbox" :checked="s.confirm_write" @change="set('set_confirm_write', { on: checked($event) })" />
      {{ $t("settings.confirmWrite") }}</label
    >
    <p v-if="message" class="text-error m-0">{{ message }}</p>
  </section>
</template>
