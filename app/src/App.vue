<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import type {
  Action,
  Actuation,
  AppSettings,
  CloseAction,
  Status as DeviceStatus,
  KeyMap,
  KeyView,
  MacroState,
  Rapid,
  WriteResult,
} from "./types";
import KeyboardMap from "./components/KeyboardMap.vue";
import SelectionBar from "./components/SelectionBar.vue";
import ActionBar from "./components/ActionBar.vue";
import ActuationCard from "./components/ActuationCard.vue";
import BindingCard from "./components/BindingCard.vue";
import MacroEditor from "./components/MacroEditor.vue";
import { common, factory, keyName, sameAction, shortLabel } from "./bindings";
import StatusBar from "./components/StatusBar.vue";
import LightingTab from "./components/LightingTab.vue";
import SettingsTab from "./components/SettingsTab.vue";
import CloseDialog from "./components/CloseDialog.vue";
import ModalDialog from "./components/ModalDialog.vue";
import ConfirmWrite from "./components/ConfirmWrite.vue";
import { setLanguage, systemLanguage } from "./i18n";

const { t } = useI18n();
const status = ref<DeviceStatus | null>(null);
const layout = ref<KeyView[]>([]);
const baseline = ref<KeyMap<number>>({}); // mm, as last read from the keyboard
const edits = ref<KeyMap<number>>({}); // mm, not applied yet
const DEFAULT_RAPID: Rapid = { enabled: false, press: 0.4, release: 0.4 };
const rapidBase = ref<KeyMap<Rapid>>({}); // as last applied
const rapidEdits = ref<KeyMap<Rapid>>({}); // not applied yet
const splitOn = ref(false);
const errors = ref<KeyMap<string>>({}); // message from the last apply
const unsaved = ref(new Set<number>()); // applied, lost on replug unless saved
const bindBase = ref<KeyMap<Action | null>>({}); // as last read or applied
const bindEdits = ref<KeyMap<Action>>({}); // not applied yet
const bindErrors = ref<KeyMap<string>>({});
const bindUnsaved = ref(new Set<number>());
const section = ref<"actuation" | "bindings" | "macros">("actuation");
const macroState = ref<MacroState | null>(null);
const selection = ref(new Set<number>());
const progress = ref<[number, number] | null>(null);
const busy = ref(false);
const message = ref("");
const TABS = ["keys", "lighting", "settings"] as const;
const tab = ref<(typeof TABS)[number]>("keys");
const closing = ref(false);
const offering = ref(false);
const asking = ref(false);
const macrosToWrite = ref<number[]>([]); // asked before applying bindings to them
let loadedProfile: number | null = null; // profile the baseline was read from
let unlistenStatus: UnlistenFn | undefined;
let unlistenClose: UnlistenFn | undefined;
let unlistenError: UnlistenFn | undefined;
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const dirty = computed(
  () => new Set([...Object.keys(edits.value), ...Object.keys(rapidEdits.value), ...Object.keys(bindEdits.value)]).size,
);
const writable = computed(() => !!status.value?.device && !status.value?.synapse && !busy.value);
const driver = computed(() => !!status.value?.driver_mode);
const canApply = computed(() => writable.value && dirty.value > 0);
// Rapid Trigger lives on the host: nothing of it goes to the flash.
const canSave = computed(
  () =>
    writable.value &&
    (Object.keys(edits.value).length > 0 ||
      unsaved.value.size > 0 ||
      Object.keys(bindEdits.value).length > 0 ||
      bindUnsaved.value.size > 0),
);
const actions = computed(() => ({
  pending: dirty.value ? t("common.notAppliedN", { n: dirty.value }) : "",
  canRevert: dirty.value > 0 && !busy.value,
  canApply: canApply.value,
  canWrite: canSave.value,
}));
const selectedValue = computed(() => {
  const values = Array.from(selection.value, (k) => edits.value[k] ?? baseline.value[k]);
  return values.length > 0 && values.every((v) => v === values[0]) ? (values[0] ?? null) : null;
});

const rapidOf = (k: number) => rapidEdits.value[k] ?? rapidBase.value[k] ?? DEFAULT_RAPID;
const sameRapid = (a: Rapid, b: Rapid) => a.enabled === b.enabled && a.press === b.press && a.release === b.release;

const selectedRapid = computed(() => {
  const r = Array.from(selection.value, rapidOf);
  return { enabled: common(r.map((x) => x.enabled)), press: common(r.map((x) => x.press)), release: common(r.map((x) => x.release)) };
});
const split = computed(() => splitOn.value || Array.from(selection.value, rapidOf).some((r) => r.release !== r.press));
const rapidKeys = computed(() => new Set(layout.value.map((k) => k.key).filter((k) => rapidOf(k).enabled)));
const rapidEdited = computed(() => new Set(Object.keys(rapidEdits.value).map(Number)));

function setRapid(change: (r: Rapid) => Rapid) {
  const next = { ...rapidEdits.value };
  for (const k of selection.value) {
    const r = change(rapidOf(k));
    if (sameRapid(r, rapidBase.value[k] ?? DEFAULT_RAPID)) delete next[k];
    else next[k] = r;
  }
  rapidEdits.value = next;
}

const bindingOf = (k: number) => bindEdits.value[k] ?? bindBase.value[k] ?? null;
// Bindings are edited one key at a time.
const bindKey = computed(() => (selection.value.size === 1 ? [...selection.value][0] : null));
const byKey = computed(() => new Map(layout.value.map((k) => [k.key, k])));

watch(section, (s) => {
  if (s === "bindings" && selection.value.size > 1) selection.value = new Set();
});

function setBinding(a: Action) {
  const k = bindKey.value;
  if (k == null) return;
  const next = { ...bindEdits.value };
  if (sameAction(a, bindBase.value[k])) delete next[k];
  else next[k] = a;
  bindEdits.value = next;
}

// What the key caps show in the open section.
const mapValues = computed(() => {
  const out: KeyMap<string> = {};
  for (const { key: k } of layout.value) {
    if (section.value === "actuation") {
      const mm = edits.value[k] ?? baseline.value[k];
      if (mm != null) out[k] = mm.toFixed(1);
    } else if (k in bindBase.value || k in bindEdits.value) {
      const a = bindingOf(k);
      if (!sameAction(a, factory(k))) out[k] = shortLabel(a, byKey.value, t, macroState.value?.macros);
    }
  }
  return out;
});
const mapEdited = computed(() => new Set(Object.keys(section.value === "actuation" ? edits.value : bindEdits.value).map(Number)));

// Keys each macro is bound to, applied or not.
const macroKeys = computed(() => {
  const out = new Map<number, string[]>();
  for (const k of layout.value) {
    const a = bindingOf(k.key);
    if (a?.type === "macro") out.set(a.id, [...(out.get(a.id) ?? []), keyName(k, t)]);
  }
  return out;
});

function onSplit(on: boolean) {
  splitOn.value = on;
  if (!on) setRapid((r) => ({ ...r, release: r.press }));
}

// Errors not tied to a tab; shown over whatever tab is open.
const appError = ref("");
function showError(e: unknown) {
  appError.value = String(e);
  clearTimeout(errorTimer);
  errorTimer = setTimeout(() => (appError.value = ""), 10_000);
}

async function loadMacros() {
  try {
    macroState.value = await invoke<MacroState>("macros");
  } catch (error) {
    showError(error);
  }
}

async function load() {
  busy.value = true;
  const unlisten = await listen<[number, number]>("read-progress", (e) => (progress.value = e.payload));
  try {
    const profile = status.value?.profile ?? null;
    const { values: base, unsaved: keys, rapid, bindings, unsaved_bindings } = await invoke<Actuation>("read_all");
    baseline.value = base;
    rapidBase.value = rapid;
    rapidEdits.value = Object.fromEntries(
      Object.entries(rapidEdits.value).filter(([k, r]) => !sameRapid(r, rapid[Number(k)] ?? DEFAULT_RAPID)),
    );
    unsaved.value = new Set(keys);
    edits.value = Object.fromEntries(Object.entries(edits.value).filter(([k, mm]) => base[Number(k)] !== mm));
    errors.value = {};
    bindBase.value = bindings;
    bindEdits.value = Object.fromEntries(Object.entries(bindEdits.value).filter(([k, a]) => !sameAction(a, bindings[Number(k)])));
    bindUnsaved.value = new Set(unsaved_bindings);
    bindErrors.value = {};
    loadedProfile = profile;
    message.value = "";
    await loadMacros();
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

async function setMode(on: boolean) {
  if (on === driver.value) return;
  busy.value = true;
  try {
    await invoke("set_driver_mode", { on });
  } catch (error) {
    showError(error);
  } finally {
    busy.value = false;
  }
  await refresh();
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
  rapidEdits.value = {};
  errors.value = {};
  bindEdits.value = {};
  bindErrors.value = {};
}

async function write(command: "apply" | "save", done: string) {
  busy.value = true;
  let failed = false;
  try {
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm]);
    const rapid = command === "apply" ? rapidEdits.value : null;
    const bindings = Object.entries(bindEdits.value).map(([k, a]) => [Number(k), a]);
    const {
      results,
      unsaved: keys,
      bindings: bound,
      unsaved_bindings,
    } = await invoke<WriteResult>(command, rapid ? { changes, rapid, bindings } : { changes, bindings });
    if (rapid) {
      rapidBase.value = { ...rapidBase.value, ...rapid };
      rapidEdits.value = {};
    }
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
    bindUnsaved.value = new Set(unsaved_bindings);
    const bBase = { ...bindBase.value };
    const bNext = { ...bindEdits.value };
    const bErrs: KeyMap<string> = {};
    for (const r of bound) {
      if (r.status === "error") {
        bErrs[r.key] = r.message;
        continue;
      }
      bBase[r.key] = r.action;
      delete bNext[r.key];
      if (r.status === "unconfirmed") bErrs[r.key] = t("bindings.unconfirmed");
    }
    bindBase.value = bBase;
    bindEdits.value = bNext;
    bindErrors.value = bErrs;
    const bad = Object.keys(errs).length + Object.keys(bErrs).length;
    message.value = bad ? t("actuation.failed", { n: bad }) : t(done);
  } catch (error) {
    message.value = String(error);
    failed = true;
  } finally {
    busy.value = false;
  }
  if (failed) await refresh();
}

// In HW mode the firmware plays only macros in its flash.
async function apply() {
  const ids = driver.value
    ? []
    : Object.values(bindEdits.value).flatMap((a) => (a.type === "macro" && !macroState.value?.macros[a.id]?.written ? [a.id] : []));
  macrosToWrite.value = [...new Set(ids)];
  if (macrosToWrite.value.length === 0) await write("apply", "actuation.applied");
}

async function onWriteMacros() {
  const ids = macrosToWrite.value;
  macrosToWrite.value = [];
  busy.value = true;
  try {
    for (const id of ids) macroState.value = await invoke<MacroState>("write_macro", { id });
  } catch (error) {
    message.value = String(error);
    return;
  } finally {
    busy.value = false;
  }
  await write("apply", "actuation.applied");
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
  await loadMacros();
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
  <main class="mx-auto p-4 flex flex-col gap-4 max-w-[1153px]">
    <nav class="flex gap-1">
      <button
        v-for="name in TABS"
        :key="name"
        class="rounded-b-none"
        :class="
          tab === name ? 'border-x-transparent border-t-transparent border-b-2 border-b-accent' : 'bg-panel text-muted border-transparent'
        "
        :aria-current="tab === name ? 'page' : undefined"
        @click="tab = name"
      >
        {{ $t(`tabs.${name}`) }}
      </button>
      <span class="ml-auto inline-flex self-center" :title="$t('mode.hint')">
        <button
          v-for="on in [false, true]"
          :key="String(on)"
          class="px-3 py-1 seg-btn"
          :class="driver === on && (on ? 'border-driver bg-driver text-white' : 'seg-on')"
          :disabled="!writable"
          @click="setMode(on)"
        >
          {{ $t(on ? "mode.driver" : "mode.hw") }}
        </button>
      </span>
    </nav>
    <StatusBar :status="status" :progress="progress" :message="tab !== 'lighting' ? message : ''" :error="appError" />
    <!-- With the map shown, the tab takes its width, not the window's. -->
    <div v-if="tab === 'keys'" class="flex flex-col gap-4" :class="{ 'self-start': section !== 'macros' }">
      <div class="flex gap-3 items-center">
        <span class="inline-flex">
          <button
            v-for="s in ['actuation', 'bindings', 'macros'] as const"
            :key="s"
            class="seg-btn"
            :class="{ 'seg-on': section === s }"
            @click="section = s"
          >
            {{ $t(`keys.${s}`) }}
          </button>
        </span>
        <SelectionBar
          v-if="section !== 'macros'"
          class="ml-auto"
          :count="selection.size"
          :single="section === 'bindings'"
          @select-all="selectAll"
          @clear="selection = new Set()"
        />
      </div>
      <MacroEditor
        v-if="section === 'macros'"
        :state="macroState"
        :layout="layout"
        :bound="macroKeys"
        :writable="writable"
        @update="(s) => (macroState = s)"
      />
      <template v-else>
        <KeyboardMap
          v-model:selection="selection"
          :layout="layout"
          :values="mapValues"
          :edited="mapEdited"
          :errors="section === 'actuation' ? errors : bindErrors"
          :unsaved="section === 'actuation' ? unsaved : bindUnsaved"
          :rapid="driver && section === 'actuation' ? rapidKeys : new Set()"
          :rapid-edited="rapidEdited"
          :single="section === 'bindings'"
        />
        <div class="gap-4 grid" :class="{ 'grid-cols-2': section === 'actuation' }">
          <div class="px-4 py-3 card flex flex-col gap-3">
            <ActuationCard
              v-if="section === 'actuation'"
              :count="selection.size"
              :value="selectedValue"
              :rapid="selectedRapid"
              :split="split"
              :driver="driver"
              @set="setValue"
              @rapid="(on) => setRapid((r) => ({ ...r, enabled: on }))"
              @press="(v) => setRapid((r) => ({ ...r, press: v, release: split ? r.release : v }))"
              @release="(v) => setRapid((r) => ({ ...r, release: v }))"
              @split="onSplit"
            />
            <BindingCard
              v-else
              :key-id="bindKey"
              :action="bindKey == null ? null : bindingOf(bindKey)"
              :layout="layout"
              :macros="macroState?.macros ?? {}"
              @set="setBinding"
            />
          </div>
          <div v-if="section === 'actuation'" class="card"></div>
        </div>
        <ActionBar v-bind="actions" @revert="revert" @apply="apply" @write="save" />
      </template>
    </div>
    <LightingTab v-else-if="tab === 'lighting'" :status="status" />
    <SettingsTab v-else />
    <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
    <ConfirmWrite
      v-if="macrosToWrite.length > 0"
      :text="$t('bindings.writeMacros', { names: macrosToWrite.map((id) => macroState?.macros[id]?.name ?? `#${id}`).join(', ') })"
      @yes="onWriteMacros"
      @no="macrosToWrite = []"
    />
    <CloseDialog v-if="closing" @choose="onClose" @cancel="closing = false" />
    <ModalDialog v-if="offering && !closing" :title="$t('dialogs.autostart.title')" @cancel="offering = false">
      <p>{{ $t("dialogs.autostart.text") }}</p>
      <template #actions>
        <button @click="onOffer(false)">{{ $t("dialogs.autostart.no") }}</button>
        <button class="primary" autofocus @click="onOffer(true)">{{ $t("dialogs.autostart.yes") }}</button>
      </template>
    </ModalDialog>
    <footer class="text-xs text-muted">{{ $t("footer") }}</footer>
  </main>
</template>
