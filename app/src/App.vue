<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type { AppSettings, ApplyResult, CloseAction, Status as DeviceStatus, KeyMap, KeyView } from "./types";
import KeyboardMap from "./components/KeyboardMap.vue";
import ActuationPanel from "./components/ActuationPanel.vue";
import StatusBar from "./components/StatusBar.vue";
import LightingTab from "./components/LightingTab.vue";
import SettingsTab from "./components/SettingsTab.vue";
import CloseDialog from "./components/CloseDialog.vue";
import AutostartOffer from "./components/AutostartOffer.vue";

const status = ref<DeviceStatus | null>(null);
const layout = ref<KeyView[]>([]);
const baseline = ref<KeyMap<number>>({}); // mm, as last read from the keyboard
const edits = ref<KeyMap<number>>({}); // mm, not applied yet
const errors = ref<KeyMap<string>>({}); // message from the last apply
const selection = ref(new Set<number>());
const progress = ref<[number, number] | null>(null);
const busy = ref(false);
const message = ref("");
const tab = ref("actuation");
const closing = ref(false);
const offering = ref(false);
let loadedProfile: number | null = null; // profile the baseline was read from
let unlistenStatus: UnlistenFn | undefined;
let unlistenClose: UnlistenFn | undefined;
let unlistenError: UnlistenFn | undefined;
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const dirty = computed(() => Object.keys(edits.value).length);
const canApply = computed(() => !!status.value?.device && !status.value?.synapse && !busy.value && dirty.value > 0);
const selectedValue = computed(() => {
  const values = Array.from(selection.value, (k) => edits.value[k] ?? baseline.value[k]);
  return values.length > 0 && values.every((v) => v === values[0]) ? (values[0] ?? null) : null;
});

async function load() {
  busy.value = true;
  const unlisten = await listen<[number, number]>("read-progress", (e) => (progress.value = e.payload));
  try {
    const profile = status.value?.profile ?? null;
    const base = await invoke<KeyMap<number>>("read_all");
    baseline.value = base;
    edits.value = Object.fromEntries(Object.entries(edits.value).filter(([k, mm]) => base[Number(k)] !== mm));
    errors.value = {};
    loadedProfile = profile;
    message.value = "";
  } catch (error) {
    message.value = String(error);
  } finally {
    unlisten();
    progress.value = null;
    busy.value = false;
  }
}

async function onStatus(s: DeviceStatus) {
  // Mid read or apply the table is being rewritten; the next event brings a fresh status anyway.
  if (busy.value) return;
  status.value = s;
  if (!s.device || s.synapse) {
    loadedProfile = null;
    return;
  }
  if (loadedProfile !== s.profile && !busy.value) await load();
}

async function refresh() {
  try {
    await onStatus(await invoke<DeviceStatus>("status"));
  } catch (error) {
    message.value = String(error);
  }
}

function setValue(mm: number) {
  const next = { ...edits.value };
  for (const k of selection.value) {
    if (baseline.value[k] === mm) delete next[k];
    else next[k] = mm;
  }
  edits.value = next;
}

function selectAll() {
  selection.value = new Set(layout.value.filter((k) => k.editable).map((k) => k.key));
}

function revert() {
  edits.value = {};
  errors.value = {};
}

async function apply() {
  busy.value = true;
  let failed = false;
  try {
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm]);
    const results = await invoke<ApplyResult[]>("apply", { changes });
    const base = { ...baseline.value };
    const next = { ...edits.value };
    const errs: KeyMap<string> = {};
    for (const r of results) {
      if (r.status === "error") {
        errs[r.key] = r.message;
        continue;
      }
      base[r.key] = r.mm;
      delete next[r.key];
      if (r.status === "unconfirmed") errs[r.key] = `Не подтверждено: в клавиатуре ${r.mm.toFixed(1)} мм`;
    }
    baseline.value = base;
    edits.value = next;
    errors.value = errs;
    const bad = Object.keys(errs).length;
    message.value = bad ? `Не применено или не подтверждено: ${bad} клав. (наведите на красные)` : "Применено";
  } catch (error) {
    message.value = String(error);
    failed = true;
  } finally {
    busy.value = false;
  }
  if (failed) await refresh();
}

// Errors not tied to a tab; shown over whatever tab is open.
const appError = ref("");
function showError(e: unknown) {
  appError.value = String(e);
  clearTimeout(errorTimer);
  errorTimer = setTimeout(() => (appError.value = ""), 10_000);
}

async function onClose(action: CloseAction, remember: boolean) {
  closing.value = false;
  // The choice still applies this session even if it could not be saved.
  if (remember) await invoke("set_close_action", { action }).catch(showError);
  await invoke(action === "tray" ? "hide_window" : "quit").catch(showError);
}

async function onOffer(on: boolean) {
  offering.value = false;
  try {
    await invoke("autostart_answered", { on });
  } catch (error) {
    showError(error);
  }
}

onMounted(async () => {
  unlistenClose = await listen("close-requested", () => (closing.value = true));
  unlistenError = await listen<string>("app-error", (e) => {
    showError(e.payload);
  });
  try {
    const settings = await invoke<AppSettings>("app_settings");
    offering.value = !settings.autostart_offered;
  } catch (error) {
    message.value = String(error);
  }
  layout.value = await invoke<KeyView[]>("layout");
  unlistenStatus = await listen<DeviceStatus>("status", (e) => {
    void onStatus(e.payload);
  });
  await refresh();
});

onUnmounted(() => {
  unlistenStatus?.();
  unlistenClose?.();
  unlistenError?.();
  clearTimeout(errorTimer);
});
</script>

<template>
  <main>
    <nav class="tabs">
      <button :class="{ on: tab === 'actuation' }" @click="tab = 'actuation'">Актуация</button>
      <button :class="{ on: tab === 'lighting' }" @click="tab = 'lighting'">Подсветка</button>
      <button :class="{ on: tab === 'settings' }" @click="tab = 'settings'">Настройки</button>
    </nav>
    <StatusBar :status="status" :progress="progress" :message="tab !== 'lighting' ? message : ''" :error="appError" />
    <template v-if="tab === 'actuation'">
      <KeyboardMap v-model:selection="selection" :layout="layout" :baseline="baseline" :edits="edits" :errors="errors" />
      <ActuationPanel
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
    <LightingTab v-else-if="tab === 'lighting'" :status="status" />
    <SettingsTab v-else />
    <CloseDialog v-if="closing" @choose="onClose" @cancel="closing = false" />
    <AutostartOffer v-if="offering && !closing" @answer="onOffer" @later="offering = false" />
    <footer>EasyRazer — неофициальный проект, не связан с Razer Inc.</footer>
  </main>
</template>

<style scoped>
main {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
}
footer {
  color: var(--muted);
  font-size: 12px;
}
.tabs {
  display: flex;
  gap: 4px;
}
.tabs button {
  border-radius: 6px 6px 0 0;
  background: var(--panel);
  color: var(--muted);
  border-color: transparent;
}
.tabs button.on {
  background: var(--key);
  color: var(--text);
  border-bottom: 2px solid var(--accent);
}
</style>
