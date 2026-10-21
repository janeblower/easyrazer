<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type { AppSettings, GamepadDriverStatus } from "../types";
import { messages, setLanguage } from "../i18n";
import AppIcon from "./AppIcon.vue";
import ModalDialog from "./ModalDialog.vue";

const props = defineProps<{
  driverStatus: GamepadDriverStatus | null;
  driverOperation: "install" | "uninstall" | null;
  driverError: string | null;
}>();
const emit = defineEmits<{ install: []; uninstall: [] }>();
const removingDriver = ref(false);
const { t } = useI18n();
// Both packages are installed, but older than the client needs.
const outdated = computed(() => (props.driverStatus?.ready === false ? props.driverStatus.version : null));
const driverState = computed(() => {
  if (props.driverOperation) return t(props.driverOperation === "install" ? "gamepad.installing" : "gamepad.uninstalling");
  if (props.driverStatus === null) return t("gamepad.checkingDriver");
  if (outdated.value) return t("gamepad.driverOutdatedShort", { version: outdated.value });
  return t(props.driverStatus.installed ? "gamepad.driverInstalled" : "gamepad.driverNotInstalled");
});

function uninstallDriver() {
  removingDriver.value = false;
  emit("uninstall");
}

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
  { key: "dev_mode", cmd: "set_dev_mode", label: "settings.devMode" },
  { key: "unverified_writes", cmd: "set_unverified_writes", label: "settings.unverifiedWrites" },
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
  <section class="px-4 py-3 card">
    <template v-if="s">
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
      <label v-if="s.dev_mode" class="field">
        <span class="field-label min-w-[160px]">{{ $t("settings.devLogSize") }}</span>
        <input
          type="number"
          min="1000"
          max="50000"
          step="1000"
          class="w-24"
          :value="s.dev_log_size"
          @change="set('set_dev_log_size', { size: Number(($event.target as HTMLInputElement).value) })"
        />
      </label>
    </template>
    <div class="field mt-4 flex-wrap">
      <span class="field-label min-w-[160px]">HIDMaestro</span>
      <span class="text-muted" role="status" :aria-busy="!!driverOperation">{{ driverState }}</span>
      <button v-if="outdated" class="primary" :disabled="!!driverOperation" @click="emit('install')">
        <AppIcon name="shield" />
        {{ $t("gamepad.update") }}
      </button>
      <button
        :disabled="driverStatus === null || !!driverOperation"
        @click="driverStatus?.installed ? (removingDriver = true) : emit('install')"
      >
        <AppIcon name="shield" />
        {{ $t(driverStatus?.installed ? "gamepad.uninstall" : "gamepad.install") }}
      </button>
    </div>
    <p v-if="driverError" class="text-error m-0" role="alert">{{ driverError }}</p>
    <p v-if="message" class="text-error m-0">{{ message }}</p>
    <ModalDialog v-if="removingDriver" :title="$t('gamepad.uninstallTitle')" @cancel="removingDriver = false">
      <p>{{ $t("gamepad.uninstallText") }}</p>
      <template #actions>
        <button autofocus @click="removingDriver = false">{{ $t("profiles.cancel") }}</button>
        <button class="primary" :disabled="!!driverOperation" @click="uninstallDriver">{{ $t("gamepad.uninstall") }}</button>
      </template>
    </ModalDialog>
  </section>
</template>
