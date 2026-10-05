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
  Hypershift,
  KeyMap,
  KeyView,
  MacroState,
  ProfilesView,
  Rapid,
  SnapTap,
  System,
  WriteResult,
} from "./types";
import KeyboardMap from "./components/KeyboardMap.vue";
import SelectionBar from "./components/SelectionBar.vue";
import ActionBar from "./components/ActionBar.vue";
import ActuationCard from "./components/ActuationCard.vue";
import SnapTapCard from "./components/SnapTapCard.vue";
import { sameSnap, snapColors } from "./snap";
import BindingCard from "./components/BindingCard.vue";
import MacroEditor from "./components/MacroEditor.vue";
import { UNIQUE, common, factory, hsFactory, keyName, sameAction, shortLabel } from "./bindings";
import StatusBar from "./components/StatusBar.vue";
import LightingTab from "./components/LightingTab.vue";
import SettingsTab from "./components/SettingsTab.vue";
import CloseDialog from "./components/CloseDialog.vue";
import ProfileMenu from "./components/ProfileMenu.vue";
import ModalDialog from "./components/ModalDialog.vue";
import ConfirmWrite from "./components/ConfirmWrite.vue";
import AppIcon from "./components/AppIcon.vue";
import { useConfirmWrite } from "./confirmWrite";
import { hasUnapplied, trackUnapplied } from "./unapplied";
import { setLanguage, systemLanguage } from "./i18n";

const { t } = useI18n();
const version = __APP_VERSION__;
const status = ref<DeviceStatus | null>(null);
const layout = ref<KeyView[]>([]);
const baseline = ref<KeyMap<number>>({}); // mm, as last read from the keyboard
const edits = ref<KeyMap<number>>({}); // mm, not applied yet
const DEFAULT_RAPID: Rapid = { enabled: false, press: 0.4, release: 0.4 };
const rapidBase = ref<KeyMap<Rapid>>({}); // as last applied
const rapidEdits = ref<KeyMap<Rapid>>({}); // not applied yet
const splitOn = ref(false);
const SNAP_OFF: SnapTap = { enabled: false, groups: [] };
const snapBase = ref<SnapTap>(SNAP_OFF); // as last applied
const snapEdit = ref<SnapTap | null>(null); // not applied yet
const errors = ref<KeyMap<string>>({}); // message from the last apply
const unsaved = ref(new Set<number>()); // applied, lost on replug unless saved
const bindBase = ref<KeyMap<Action | null>>({}); // as last read or applied
const bindEdits = ref<KeyMap<Action>>({}); // not applied yet
const bindErrors = ref<KeyMap<string>>({});
const bindUnsaved = ref(new Set<number>());
const hsBase = ref<KeyMap<number>>({}); // Hypershift mm, as last read or applied
const hsEdits = ref<KeyMap<number>>({});
const hsSplitOn = ref(false);
const hsBindBase = ref<KeyMap<Action | null>>({});
const hsBindEdits = ref<KeyMap<Action>>({});
const hsUnsaved = ref(new Set<number>());
const section = ref<"actuation" | "bindings" | "macros">("actuation");
const macroState = ref<MacroState | null>(null);
const selection = ref(new Set<number>());
const progress = ref<[number, number] | null>(null); // reads done, reads in all
const editableCount = computed(() => layout.value.filter((k) => k.editable).length);
const busy = ref(false);
const profiles = ref<ProfilesView | null>(null);
const removing = ref<number | null>(null); // asked before deleting
const switching = ref<(() => void) | null>(null); // asked before dropping edits
const message = ref("");
const TABS = ["keys", "lighting", "settings"] as const;
const tab = ref<(typeof TABS)[number]>("keys");
const closing = ref(false);
const hiding = ref(false); // asked before dropping edits with the window
const offering = ref(false);
const macrosToWrite = ref<number[]>([]); // asked before applying bindings to them
let loadedProfile: number | null = null; // profile the baseline was read from
let unlistenStatus: UnlistenFn | undefined;
let unlistenClose: UnlistenFn | undefined;
let unlistenHide: UnlistenFn | undefined;
let unlistenError: UnlistenFn | undefined;
let unlistenProgress: UnlistenFn | undefined;
let unlistenSnap: UnlistenFn | undefined;
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const dirty = computed(
  () =>
    new Set([
      ...Object.keys(edits.value),
      ...Object.keys(rapidEdits.value),
      ...Object.keys(bindEdits.value),
      ...Object.keys(hsEdits.value),
      ...Object.keys(hsBindEdits.value),
    ]).size + (snapEdit.value ? 1 : 0),
);
trackUnapplied(() => dirty.value > 0);
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
      bindUnsaved.value.size > 0 ||
      Object.keys(hsEdits.value).length > 0 ||
      Object.keys(hsBindEdits.value).length > 0 ||
      hsUnsaved.value.size > 0),
);
const actions = computed(() => ({
  pending: dirty.value ? t("common.notAppliedN", { n: dirty.value }) : "",
  canRevert: dirty.value > 0 && !busy.value,
  canApply: canApply.value,
  canWrite: canSave.value,
}));
const selectedValue = computed(() => common(Array.from(selection.value, (k) => edits.value[k] ?? baseline.value[k])) ?? null);
const valueOf = (k: number) => edits.value[k] ?? baseline.value[k];
const hsOf = (k: number) => hsEdits.value[k] ?? hsBase.value[k];
const selectedHs = computed(() => common(Array.from(selection.value, hsOf)) ?? null);
const hsSplit = computed(() => hsSplitOn.value || [...selection.value].some((k) => hsOf(k) !== valueOf(k)));

function setHs(mm: (k: number) => number) {
  const next = { ...hsEdits.value };
  for (const k of selection.value) {
    const v = mm(k);
    if (hsBase.value[k] === v) delete next[k];
    else next[k] = v;
  }
  hsEdits.value = next;
}

function onHsSplit(on: boolean) {
  hsSplitOn.value = on;
  if (!on) setHs(valueOf);
}

// On keys where it differs from the normal one.
const mapHs = computed(() => {
  const out: KeyMap<string> = {};
  if (section.value !== "actuation") return out;
  for (const { key: k } of layout.value) {
    const v = hsOf(k);
    if (v != null && v !== valueOf(k)) out[k] = v.toFixed(1);
  }
  return out;
});
const hsEdited = computed(() => new Set(Object.keys(hsEdits.value).map(Number)));

function takeHypershift(h: Hypershift) {
  hsBase.value = h.values;
  hsBindBase.value = h.bindings;
  hsUnsaved.value = new Set(h.unsaved);
}

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

const snap = computed(() => snapEdit.value ?? snapBase.value);

function setSnap(s: SnapTap) {
  snapEdit.value = sameSnap(s, snapBase.value) ? null : s;
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

const hsBindingOf = (k: number) => hsBindEdits.value[k] ?? hsBindBase.value[k] ?? null;
const combo = (k: number | null) => (k == null ? null : `Fn+${keyName(byKey.value.get(k)!, t)}`);

// The key holding a unique system action, unapplied edits included.
function holder(s: System): number | null {
  for (const { key: k } of layout.value) {
    const a = hsBindingOf(k);
    if (a?.type === "system" && a.action === s) return k;
  }
  return null;
}

const taken = computed(() =>
  Object.fromEntries(
    UNIQUE.flatMap((s) => {
      const k = holder(s);
      return k != null && k !== bindKey.value ? [[s, combo(k)!]] : [];
    }),
  ),
);
const snapCombo = computed(() => combo(holder("snap_tap")));
const nextCombo = computed(() => combo(holder("next_profile")));
const moving = ref<{ from: number; action: System } | null>(null); // asked before moving a unique action

function putHs(next: KeyMap<Action>, k: number, a: Action) {
  if (sameAction(a, hsBindBase.value[k])) delete next[k];
  else next[k] = a;
}

function setHsBinding(a: Action) {
  const k = bindKey.value;
  if (k == null) return;
  const from = a.type === "system" && UNIQUE.includes(a.action) ? holder(a.action) : null;
  if (from != null && from !== k && a.type === "system") {
    moving.value = { from, action: a.action };
    return;
  }
  const next = { ...hsBindEdits.value };
  putHs(next, k, a);
  hsBindEdits.value = next;
}

// The old combination goes back to its own key, not to its factory system action.
function confirmMove() {
  const m = moving.value;
  const k = bindKey.value;
  moving.value = null;
  if (!m || k == null) return;
  const next = { ...hsBindEdits.value };
  putHs(next, m.from, factory(m.from));
  putHs(next, k, { type: "system", action: m.action });
  hsBindEdits.value = next;
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

async function loadProfiles() {
  try {
    profiles.value = await invoke<ProfilesView>("profiles");
  } catch (error) {
    showError(error);
  }
}

async function load() {
  busy.value = true;
  progress.value = [0, 1];
  try {
    const {
      values: base,
      unsaved: keys,
      rapid,
      snap: snapRead,
      bindings,
      unsaved_bindings,
      hypershift,
      profile,
    } = await invoke<Actuation>("read_all");
    baseline.value = base;
    rapidBase.value = rapid;
    rapidEdits.value = Object.fromEntries(
      Object.entries(rapidEdits.value).filter(([k, r]) => !sameRapid(r, rapid[Number(k)] ?? DEFAULT_RAPID)),
    );
    snapBase.value = snapRead;
    if (snapEdit.value && sameSnap(snapEdit.value, snapRead)) snapEdit.value = null;
    unsaved.value = new Set(keys);
    edits.value = Object.fromEntries(Object.entries(edits.value).filter(([k, mm]) => base[Number(k)] !== mm));
    errors.value = {};
    bindBase.value = bindings;
    bindEdits.value = Object.fromEntries(Object.entries(bindEdits.value).filter(([k, a]) => !sameAction(a, bindings[Number(k)])));
    bindUnsaved.value = new Set(unsaved_bindings);
    takeHypershift(hypershift);
    hsEdits.value = Object.fromEntries(Object.entries(hsEdits.value).filter(([k, mm]) => hypershift.values[Number(k)] !== mm));
    hsBindEdits.value = Object.fromEntries(
      Object.entries(hsBindEdits.value).filter(([k, a]) => !sameAction(a, hypershift.bindings[Number(k)])),
    );
    bindErrors.value = {};
    loadedProfile = profile;
    message.value = "";
    await loadMacros();
    await loadProfiles();
  } catch (error) {
    message.value = String(error);
  } finally {
    progress.value = null;
    busy.value = false;
  }
}

function revert() {
  edits.value = {};
  rapidEdits.value = {};
  snapEdit.value = null;
  errors.value = {};
  bindEdits.value = {};
  hsEdits.value = {};
  hsBindEdits.value = {};
  bindErrors.value = {};
}

async function onStatus(s: DeviceStatus) {
  if (s.error) showError(s.error);
  // Mid read or apply the table is being rewritten; the next event brings a fresh status anyway.
  if (busy.value) return;
  status.value = s;
  if (!s.device || s.synapse) {
    loadedProfile = null;
    progress.value = null;
    return;
  }
  if (loadedProfile !== null && s.profile !== null && s.profile !== loadedProfile) revert();
  if (loadedProfile === null || loadedProfile !== s.profile) await load();
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
  const together = !hsSplit.value;
  const next = { ...edits.value };
  for (const k of selection.value) {
    if (baseline.value[k] === mm) delete next[k];
    else next[k] = mm;
  }
  edits.value = next;
  if (together) setHs(() => mm);
}

function selectAll() {
  selection.value = new Set(layout.value.filter((k) => k.editable).map((k) => k.key));
}

async function profileOp(command: string, args: Record<string, unknown> = {}) {
  busy.value = true;
  try {
    profiles.value = await invoke<ProfilesView>(command, args);
  } catch (error) {
    showError(error);
  } finally {
    busy.value = false;
  }
  await refresh();
}

// Loading another profile drops unapplied edits; ask first.
function switchTo(op: () => void) {
  if (dirty.value > 0) switching.value = op;
  else op();
}

function onLoad(id: number) {
  switchTo(() => {
    void profileOp("load_profile", { id });
  });
}

function onCreate() {
  switchTo(() => {
    void profileOp("create_profile");
  });
}

function confirmSwitch() {
  const op = switching.value;
  switching.value = null;
  revert();
  op?.();
}

function confirmRemove() {
  const id = removing.value;
  removing.value = null;
  if (id != null) void profileOp("delete_profile", { id });
}

type Written = { status: "ok" | "unconfirmed"; key: number };
type Failed = { status: "error"; key: number; message: string };

// Confirmed and unconfirmed keys take the device value and leave the edits; only failed ones keep them.
function merge<O extends Written, V, E>(
  results: (O | Failed)[],
  base: KeyMap<V>,
  edits: KeyMap<E>,
  value: (r: O) => V,
  unconfirmed: (r: O) => string,
) {
  const next = { base: { ...base }, edits: { ...edits }, errors: {} as KeyMap<string> };
  for (const r of results) {
    if (r.status === "error") {
      next.errors[r.key] = r.message;
      continue;
    }
    next.base[r.key] = value(r);
    delete next.edits[r.key];
    if (r.status === "unconfirmed") next.errors[r.key] = unconfirmed(r);
  }
  return next;
}

async function write(command: "apply" | "save", done: string) {
  busy.value = true;
  let failed = false;
  try {
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm]);
    const rapid = command === "apply" ? rapidEdits.value : null;
    const snapNext = command === "apply" ? snapEdit.value : null;
    const bindings = Object.entries(bindEdits.value).map(([k, a]) => [Number(k), a]);
    const hypershift = {
      values: Object.entries(hsEdits.value).map(([k, mm]) => [Number(k), mm]),
      bindings: Object.entries(hsBindEdits.value).map(([k, a]) => [Number(k), a]),
    };
    const {
      results,
      unsaved: keys,
      bindings: bound,
      unsaved_bindings,
      hypershift: layer,
    } = await invoke<WriteResult>(
      command,
      rapid ? { changes, rapid, snap: snapNext, bindings, hypershift } : { changes, bindings, hypershift },
    );
    // The backend takes the layer whole or refuses it, so a result means every edit is in.
    takeHypershift(layer);
    hsEdits.value = {};
    hsBindEdits.value = {};
    if (rapid) {
      rapidBase.value = { ...rapidBase.value, ...rapid };
      rapidEdits.value = {};
      if (snapNext) snapBase.value = snapNext;
      snapEdit.value = null;
    }
    unsaved.value = new Set(keys);
    const applied = merge(
      results,
      baseline.value,
      edits.value,
      (r) => r.mm,
      (r) => t("actuation.unconfirmed", { v: r.mm.toFixed(1) }),
    );
    baseline.value = applied.base;
    edits.value = applied.edits;
    errors.value = applied.errors;
    bindUnsaved.value = new Set(unsaved_bindings);
    const binds = merge(
      bound,
      bindBase.value,
      bindEdits.value,
      (r) => r.action,
      () => t("bindings.unconfirmed"),
    );
    bindBase.value = binds.base;
    bindEdits.value = binds.edits;
    bindErrors.value = binds.errors;
    const bad = Object.keys(applied.errors).length + Object.keys(binds.errors).length;
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
    : [...Object.values(bindEdits.value), ...Object.values(hsBindEdits.value)].flatMap((a) =>
        a.type === "macro" && !macroState.value?.macros[a.id]?.written ? [a.id] : [],
      );
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

const {
  asking,
  write: save,
  onConfirm,
} = useConfirmWrite(
  async () => write("save", "actuation.saved"),
  (error) => (message.value = String(error)),
);

async function onClose(action: CloseAction, remember: boolean) {
  closing.value = false;
  // The choice still applies this session even if it could not be saved.
  if (remember) await invoke("set_close_action", { action }).catch(showError);
  await invoke(action === "tray" ? "hide_window" : "quit").catch(showError);
}

async function hide() {
  hiding.value = false;
  await invoke("hide_window").catch(showError);
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
  // Before anything that takes the device lock: a connect can hold it for the whole read.
  layout.value = await invoke<KeyView[]>("layout");
  unlistenClose = await listen("close-requested", () => (closing.value = true));
  unlistenHide = await listen("hide-requested", () => {
    if (hasUnapplied()) hiding.value = true;
    else void hide();
  });
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
  // A connect reads the keyboard on its own, before the status that starts load() arrives.
  unlistenProgress = await listen<[number, number, number | null, number | null, Rapid | null]>("read-progress", (e) => {
    const [done, total, key, mm, rapid] = e.payload;
    progress.value = [done, total];
    if (key == null || mm == null) return;
    baseline.value[key] = mm;
    if (rapid) rapidBase.value[key] = rapid;
  });
  await invoke("window_ready");
  await loadMacros();
  unlistenSnap = await listen<boolean>("snap-tap", (e) => {
    snapBase.value = { ...snapBase.value, enabled: e.payload };
    if (snapEdit.value) setSnap({ ...snapEdit.value, enabled: e.payload });
  });
  unlistenStatus = await listen<DeviceStatus>("status", (e) => {
    void onStatus(e.payload);
  });
  await refresh();
});

onUnmounted(() => {
  unlistenStatus?.();
  unlistenClose?.();
  unlistenHide?.();
  unlistenError?.();
  unlistenProgress?.();
  unlistenSnap?.();
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
      <ProfileMenu
        class="ml-auto"
        :view="profiles"
        :writable="writable"
        :next-combo="nextCombo"
        @load="onLoad"
        @create="onCreate"
        @duplicate="(id) => profileOp('duplicate_profile', { id })"
        @rename="(id, name) => profileOp('rename_profile', { id, name })"
        @remove="(id) => (removing = id)"
        @startup="(id) => profileOp('set_startup', { id })"
        @free="(id) => profileOp('free_slot', { id })"
        @write="(id) => profileOp('write_profile', { id })"
      />
      <span class="inline-flex" :title="$t('mode.hint')">
        <button
          v-for="on in [false, true]"
          :key="String(on)"
          class="px-3 py-0 seg-btn inline-flex items-center"
          :class="driver !== on ? 'text-muted' : on ? 'text-driver' : 'text-accent'"
          :aria-label="$t(on ? 'mode.driver' : 'mode.hw')"
          :aria-pressed="driver === on"
          :disabled="!writable"
          @click="setMode(on)"
        >
          <AppIcon class="h-7 w-7" :name="on ? 'rust' : 'chip'" />
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
          :snap="driver && section === 'actuation' ? snapColors(snap) : {}"
          :rapid-edited="rapidEdited"
          :hs-values="mapHs"
          :hs-edited="hsEdited"
          :single="section === 'bindings'"
          :loaded="progress && Math.floor((progress[0] / progress[1]) * editableCount)"
        />
        <div class="gap-4 grid grid-cols-2">
          <div class="px-4 py-3 card flex flex-col gap-3">
            <ActuationCard
              v-if="section === 'actuation'"
              :count="selection.size"
              :value="selectedValue"
              :rapid="selectedRapid"
              :split="split"
              :hs="selectedHs"
              :hs-split="hsSplit"
              :driver="driver"
              @set="setValue"
              @rapid="(on) => setRapid((r) => ({ ...r, enabled: on }))"
              @press="(v) => setRapid((r) => ({ ...r, press: v, release: split ? r.release : v }))"
              @release="(v) => setRapid((r) => ({ ...r, release: v }))"
              @split="onSplit"
              @hs="(v) => setHs(() => v)"
              @hs-split="onHsSplit"
            />
            <BindingCard
              v-else
              :title="$t('bindings.main')"
              :key-id="bindKey"
              :action="bindKey == null ? null : bindingOf(bindKey)"
              :factory-action="bindKey == null ? null : factory(bindKey)"
              :layout="layout"
              :macros="macroState?.macros ?? {}"
              @set="setBinding"
            />
          </div>
          <div class="px-4 py-3 card flex flex-col gap-3">
            <SnapTapCard
              v-if="section === 'actuation'"
              :snap="snap"
              :selection="selection"
              :layout="layout"
              :driver="driver"
              :combo="snapCombo"
              @update="setSnap"
            />
            <BindingCard
              v-else
              hypershift
              :title="bindKey == null ? 'Hypershift' : `Hypershift · ${combo(bindKey)}`"
              :key-id="bindKey"
              :action="bindKey == null ? null : hsBindingOf(bindKey)"
              :factory-action="bindKey == null ? null : hsFactory(bindKey)"
              :taken="taken"
              :layout="layout"
              :macros="macroState?.macros ?? {}"
              @set="setHsBinding"
            />
          </div>
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
    <ModalDialog
      v-if="moving"
      :title="$t('bindings.moveTitle', { action: $t(`bindings.system.${moving.action}`), to: combo(bindKey) })"
      @cancel="moving = null"
    >
      <p>{{ $t("bindings.moveText", { from: combo(moving.from) }) }}</p>
      <template #actions>
        <button autofocus @click="moving = null">{{ $t("profiles.cancel") }}</button>
        <button class="primary" @click="confirmMove">{{ $t("bindings.move") }}</button>
      </template>
    </ModalDialog>
    <ModalDialog v-if="hiding" :title="$t('dialogs.close.title')" @cancel="hiding = false">
      <p>{{ $t("dialogs.close.unapplied") }}</p>
      <template #actions>
        <button autofocus @click="hiding = false">{{ $t("profiles.cancel") }}</button>
        <button class="primary" @click="hide">{{ $t("dialogs.close.tray") }}</button>
      </template>
    </ModalDialog>
    <ModalDialog
      v-if="removing != null"
      :title="$t('profiles.removeTitle', { name: profiles?.profiles.find((p) => p.id === removing)?.name ?? '' })"
      @cancel="removing = null"
    >
      <p>{{ $t("profiles.removeText") }}</p>
      <template #actions>
        <button autofocus @click="removing = null">{{ $t("profiles.cancel") }}</button>
        <button class="primary" @click="confirmRemove">{{ $t("profiles.remove") }}</button>
      </template>
    </ModalDialog>
    <ModalDialog v-if="switching != null" :title="$t('profiles.dropTitle')" @cancel="switching = null">
      <template #actions>
        <button autofocus @click="switching = null">{{ $t("profiles.cancel") }}</button>
        <button class="primary" @click="confirmSwitch">{{ $t("profiles.drop") }}</button>
      </template>
    </ModalDialog>
    <ModalDialog v-if="offering && !closing" :title="$t('dialogs.autostart.title')" @cancel="offering = false">
      <p>{{ $t("dialogs.autostart.text") }}</p>
      <template #actions>
        <button @click="onOffer(false)">{{ $t("dialogs.autostart.no") }}</button>
        <button class="primary" autofocus @click="onOffer(true)">{{ $t("dialogs.autostart.yes") }}</button>
      </template>
    </ModalDialog>
    <footer class="text-xs text-muted">{{ version }} · {{ $t("footer") }}</footer>
  </main>
</template>
