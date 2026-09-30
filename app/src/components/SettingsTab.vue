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
const TOGGLES = [
  { key: "autostart", cmd: "set_autostart", label: "settings.autostart" },
  { key: "watch_synapse", cmd: "set_watch_synapse", label: "settings.watchSynapse" },
  { key: "confirm_write", cmd: "set_confirm_write", label: "settings.confirmWrite" },
] as const;

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
  <section v-if="s" class="px-4 py-3 card">
    <label class="field">
      <span class="field-label min-w-[160px]">{{ $t("settings.language") }}</span>
      <select :value="s.language ?? $i18n.locale" @change="pickLanguage">
        <option v-for="(m, code) in messages" :key="code" :value="code">{{ m.language.name }}</option>
      </select>
    </label>
    <label class="field">
      <span class="field-label min-w-[160px]">{{ $t("settings.onClose") }}</span>
      <select :value="s.close_action" @change="set('set_close_action', { action: ($event.target as HTMLSelectElement).value })">
        <option value="ask">{{ $t("settings.ask") }}</option>
        <option value="tray">{{ $t("settings.tray") }}</option>
        <option value="exit">{{ $t("settings.exit") }}</option>
      </select>
    </label>
    <label v-for="o in TOGGLES" :key="o.cmd" class="switch my-2.5">
      <input type="checkbox" role="switch" :checked="s[o.key]" @change="set(o.cmd, { on: checked($event) })" />
      {{ $t(o.label) }}
    </label>
    <p v-if="message" class="text-error m-0">{{ message }}</p>
  </section>
</template>
