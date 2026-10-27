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
  bootChanging?: boolean;
}>();
const emit = defineEmits<{ install: []; uninstall: []; boot: [on: boolean] }>();
const removingDriver = ref(false);
const { t } = useI18n();
// Installed, but older than the app needs.
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
const KEYBOARD = [
  { key: "watch_synapse", cmd: "set_watch_synapse", label: "watchSynapse" },
  { key: "confirm_write", cmd: "set_confirm_write", label: "confirmWrite" },
] as const;
const DEV = [
  { key: "dev_mode", cmd: "set_dev_mode", label: "devMode" },
  { key: "unverified_writes", cmd: "set_unverified_writes", label: "unverifiedWrites" },
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
      <h3 class="group">{{ $t("settings.groups.app") }}</h3>
      <label class="row">
        <span class="flex-1">{{ $t("settings.language") }}</span>
        <select :value="s.language ?? $i18n.locale" @change="pickLanguage">
          <option v-for="(m, code) in messages" :key="code" :value="code">{{ m.language.name }}</option>
        </select>
      </label>
      <label class="row">
        <span class="flex-1"
          >{{ $t("settings.onClose") }}<span class="desc">{{ $t("settings.hints.onClose") }}</span></span
        >
        <select :value="s.close_action" @change="set('set_close_action', { action: ($event.target as HTMLSelectElement).value })">
          <option value="ask">{{ $t("settings.ask") }}</option>
          <option value="tray">{{ $t("settings.tray") }}</option>
          <option value="exit">{{ $t("settings.exit") }}</option>
        </select>
      </label>
      <label class="row switch">
        <span class="flex-1"
          >{{ $t("settings.autostart") }}<span class="desc">{{ $t("settings.hints.autostart") }}</span></span
        >
        <input type="checkbox" role="switch" :checked="s.autostart" @change="set('set_autostart', { on: checked($event) })" />
      </label>

      <h3 class="group">{{ $t("settings.groups.keyboard") }}</h3>
      <label v-for="o in KEYBOARD" :key="o.cmd" class="row switch">
        <span class="flex-1"
          >{{ $t(`settings.${o.label}`) }}<span class="desc">{{ $t(`settings.hints.${o.label}`) }}</span></span
        >
        <input type="checkbox" role="switch" :checked="s[o.key]" @change="set(o.cmd, { on: checked($event) })" />
      </label>
    </template>

    <h3 class="group">{{ $t("settings.groups.gamepad") }}</h3>
    <div class="row">
      <span class="flex-1">
        <span class="flex gap-2 items-center">
          hidshim{{ driverStatus?.version && !outdated ? ` ${driverStatus.version}` : "" }}
          <span class="driver-dot" :class="{ on: driverStatus?.installed, busy: !!driverOperation }"></span>
          <span
            class="text-xs"
            role="status"
            :aria-busy="!!driverOperation"
            :class="outdated ? 'text-warn' : driverStatus?.installed ? 'text-accent' : 'text-muted'"
            >{{ driverState }}</span
          >
        </span>
        <span class="desc">{{ $t("settings.hints.driver") }}</span>
        <span v-if="driverStatus?.legacy" class="desc text-warn">{{ $t("gamepad.legacyInstalled") }}</span>
      </span>
      <button v-if="outdated" class="primary inline-flex gap-2 items-center" :disabled="!!driverOperation" @click="emit('install')">
        <AppIcon name="shield" />
        {{ $t("gamepad.update") }}
      </button>
      <button
        class="inline-flex gap-2 items-center"
        :disabled="driverStatus === null || !!driverOperation"
        @click="driverStatus?.installed ? (removingDriver = true) : emit('install')"
      >
        <AppIcon name="shield" />
        {{ $t(driverStatus?.installed ? "gamepad.uninstall" : "gamepad.install") }}
      </button>
    </div>
    <label v-if="driverStatus?.boot != null" class="row switch">
      <span class="flex-1"
        >{{ $t("settings.driverBoot") }}<span class="desc">{{ $t("settings.hints.driverBoot") }}</span></span
      >
      <input
        type="checkbox"
        role="switch"
        :checked="driverStatus.boot"
        :disabled="bootChanging || !!driverOperation"
        @change="emit('boot', checked($event))"
      />
    </label>
    <p v-if="driverError" class="text-error m-0" role="alert">{{ driverError }}</p>

    <div v-if="s" class="danger">
      <h3 class="group">{{ $t("settings.groups.dev") }}</h3>
      <label v-for="o in DEV" :key="o.cmd" class="row switch" :class="{ risky: o.key === 'unverified_writes' }">
        <span class="flex-1"
          >{{ $t(`settings.${o.label}`) }}<span class="desc">{{ $t(`settings.hints.${o.label}`) }}</span></span
        >
        <input type="checkbox" role="switch" :checked="s[o.key]" @change="set(o.cmd, { on: checked($event) })" />
      </label>
      <label v-if="s.dev_mode" class="row">
        <span class="flex-1">{{ $t("settings.devLogSize") }}</span>
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
    </div>
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

<style scoped>
.group {
  margin: 16px 0 2px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--muted);
}
.group:first-child {
  margin-top: 4px;
}
.row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 9px 0;
}
.row + .row {
  border-top: 1px solid var(--line);
}
.desc {
  display: block;
  margin-top: 1px;
  font-size: 12px;
  color: var(--muted);
}
.danger {
  margin-top: 16px;
  padding: 0 12px 2px;
  border: 1px solid var(--write-line);
  border-radius: 6px;
}
.danger .group {
  color: var(--warn);
}
/* Writing to an unchecked model is not a state to look good in. */
.risky input:checked {
  background: var(--edited);
}
.driver-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #5a5a5a;
}
.driver-dot.on,
.driver-dot.busy {
  background: var(--accent);
}
.driver-dot.on {
  box-shadow: 0 0 6px rgb(68 214 44 / 60%);
}
.driver-dot.busy {
  animation: driver-pulse 1s ease-in-out infinite;
}
@keyframes driver-pulse {
  50% {
    opacity: 0.25;
  }
}
@media (prefers-reduced-motion: reduce) {
  .driver-dot.busy {
    animation: none;
  }
}
</style>
