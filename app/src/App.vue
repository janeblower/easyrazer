<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type { Actuation, AppSettings, CloseAction, Status as DeviceStatus, KeyMap, KeyView, WriteResult } from "./types";
import KeyboardMap from "./components/KeyboardMap.vue";
import ActuationPanel from "./components/ActuationPanel.vue";
import StatusBar from "./components/StatusBar.vue";
import LightingTab from "./components/LightingTab.vue";
import SettingsTab from "./components/SettingsTab.vue";
import CloseDialog from "./components/CloseDialog.vue";
import AutostartOffer from "./components/AutostartOffer.vue";
import ConfirmWrite from "./components/ConfirmWrite.vue";
import { setLanguage, systemLanguage } from "./i18n";

const { t } = useI18n();
const status = ref<DeviceStatus | null>(null);
const layout = ref<KeyView[]>([]);
const baseline = ref<KeyMap<number>>({}); // mm, as last read from the keyboard
const edits = ref<KeyMap<number>>({}); // mm, not applied yet
const errors = ref<KeyMap<string>>({}); // message from the last apply
const unsaved = ref(new Set<number>()); // applied, lost on replug unless saved
const selection = ref(new Set<number>());
const progress = ref<[number, number] | null>(null);
const busy = ref(false);
const message = ref("");
const tab = ref("actuation");
const closing = ref(false);
const offering = ref(false);
const asking = ref(false);
let loadedProfile: number | null = null; // profile the baseline was read from
let unlistenStatus: UnlistenFn | undefined;
let unlistenClose: UnlistenFn | undefined;
let unlistenError: UnlistenFn | undefined;
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const tabClass = (t: string) => [
  "rounded-b-none",
  tab.value === t
    ? "border-x-transparent border-t-transparent border-b-2 border-b-accent bg-key text-text"
    : "bg-panel text-muted border-transparent",
];

const dirty = computed(() => Object.keys(edits.value).length);
const writable = computed(() => !!status.value?.device && !status.value?.synapse && !busy.value);
const canApply = computed(() => writable.value && dirty.value > 0);
const canSave = computed(() => writable.value && (dirty.value > 0 || unsaved.value.size > 0));
const selectedValue = computed(() => {
  const values = Array.from(selection.value, (k) => edits.value[k] ?? baseline.value[k]);
  return values.length > 0 && values.every((v) => v === values[0]) ? (values[0] ?? null) : null;
});

// Errors not tied to a tab; shown over whatever tab is open.
const appError = ref("");
function showError(e: unknown) {
  appError.value = String(e);
  clearTimeout(errorTimer);
  errorTimer = setTimeout(() => (appError.value = ""), 10_000);
}

async function load() {
  busy.value = true;
  const unlisten = await listen<[number, number]>("read-progress", (e) => (progress.value = e.payload));
  try {
    const profile = status.value?.profile ?? null;
    const { values: base, unsaved: keys } = await invoke<Actuation>("read_all");
    baseline.value = base;
    unsaved.value = new Set(keys);
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
  if (s.error) showError(s.error);
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

async function write(command: "apply" | "save", done: string) {
  busy.value = true;
  let failed = false;
  try {
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm]);
    const { results, unsaved: keys } = await invoke<WriteResult>(command, { changes });
    unsaved.value = new Set(keys);
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
      if (r.status === "unconfirmed") errs[r.key] = t("actuation.unconfirmed", { v: r.mm.toFixed(1) });
    }
    baseline.value = base;
    edits.value = next;
    errors.value = errs;
    const bad = Object.keys(errs).length;
    message.value = bad ? t("actuation.failed", { n: bad }) : t(done);
  } catch (error) {
    message.value = String(error);
    failed = true;
  } finally {
    busy.value = false;
  }
  if (failed) await refresh();
}

async function save() {
  try {
    const settings = await invoke<AppSettings>("app_settings");
    if (settings.confirm_write) {
      asking.value = true;
      return;
    }
  } catch (error) {
    message.value = String(error);
    return;
  }
  await write("save", "actuation.saved");
}

async function onConfirm(dontAsk: boolean) {
  asking.value = false;
  if (dontAsk) await invoke("set_confirm_write", { on: false }).catch(showError);
  await write("save", "actuation.saved");
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
    const lang = settings.language ?? systemLanguage();
    setLanguage(lang);
    // The tray and backend messages follow the stored language.
    if (!settings.language) await invoke("set_language", { language: lang });
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
  <main class="p-4 flex flex-col gap-4">
    <nav class="flex gap-1">
      <button :class="tabClass('actuation')" @click="tab = 'actuation'">{{ $t("tabs.actuation") }}</button>
      <button :class="tabClass('lighting')" @click="tab = 'lighting'">{{ $t("tabs.lighting") }}</button>
      <button :class="tabClass('settings')" @click="tab = 'settings'">{{ $t("tabs.settings") }}</button>
    </nav>
    <StatusBar :status="status" :progress="progress" :message="tab !== 'lighting' ? message : ''" :error="appError" />
    <template v-if="tab === 'actuation'">
      <KeyboardMap v-model:selection="selection" :layout="layout" :baseline="baseline" :edits="edits" :errors="errors" :unsaved="unsaved" />
      <ActuationPanel
        :count="selection.size"
        :value="selectedValue"
        :dirty="dirty"
        :can-apply="canApply"
        :can-save="canSave"
        :busy="busy"
        @set="setValue"
        @apply="write('apply', 'actuation.applied')"
        @save="save"
        @revert="revert"
        @select-all="selectAll"
        @clear="selection = new Set()"
      />
    </template>
    <LightingTab v-else-if="tab === 'lighting'" :status="status" />
    <SettingsTab v-else />
    <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
    <CloseDialog v-if="closing" @choose="onClose" @cancel="closing = false" />
    <AutostartOffer v-if="offering && !closing" @answer="onOffer" @later="offering = false" />
    <footer class="text-xs text-muted">{{ $t("footer") }}</footer>
  </main>
</template>
