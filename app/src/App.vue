<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { call, keyboard, loadKey, ours, pick, statusAction } from "./keyboard";
import type {
  Action,
  Actuation,
  AppSettings,
  CloseAction,
  Status as DeviceStatus,
  DevicesView,
  Hypershift,
  ImportView,
  KeyMap,
  KeyView,
  MacroState,
  ProfilesView,
  Rapid,
  RapidMode,
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
import GamepadTab from "./components/GamepadTab.vue";
import { type GamepadConfig, type GamepadControl, type GamepadInput, defaultGamepad, sameGamepad } from "./gamepad";
import { UNIQUE, common, factory, hsFactory, keyName, sameAction, shortLabel } from "./bindings";
import StatusBar from "./components/StatusBar.vue";
import LightingTab from "./components/LightingTab.vue";
import SettingsTab from "./components/SettingsTab.vue";
import ProfileMenu from "./components/ProfileMenu.vue";
import DeviceMenu from "./components/DeviceMenu.vue";
import ModalDialog from "./components/ModalDialog.vue";
import ConfirmWrite from "./components/ConfirmWrite.vue";
import AppIcon from "./components/AppIcon.vue";
import { useConfirmWrite } from "./confirmWrite";
import { hasUnapplied, trackUnapplied } from "./unapplied";
import { setLanguage, systemLanguage } from "./i18n";
import DevPanel from "./components/DevPanel.vue";
import { dev, reloadLog, setPanel, startDev } from "./dev";
import DevKeys from "./components/DevKeys.vue";
import { applyLayout } from "./devWindow";

const { t } = useI18n();
const version = __APP_VERSION__;
const status = ref<DeviceStatus | null>(null);
const layout = ref<KeyView[]>([]);
// Kept when the keyboard is unplugged, so the map does not jump to another board.
let layoutModel: string | null = null;
const baseline = ref<KeyMap<number>>({}); // mm, as last read from the keyboard
const edits = ref<KeyMap<number>>({}); // mm, not applied yet
const DEFAULT_RAPID: Rapid = { enabled: false, press: 0.4, release: 0.4, continuous: false };
const rapidBase = ref<KeyMap<Rapid>>({}); // as last applied
const rapidEdits = ref<KeyMap<Rapid>>({}); // not applied yet
const splitOn = ref(false);
const SNAP_OFF: SnapTap = { enabled: false, groups: [] };
const snapBase = ref<SnapTap>(SNAP_OFF); // as last applied
const snapEdit = ref<SnapTap | null>(null); // not applied yet
// The firmware's continuous Rapid Trigger, one for the whole profile.
const crtBase = ref(false); // as last applied
const crtEdit = ref<boolean | null>(null); // not applied yet
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
const section = ref<"actuation" | "bindings" | "macros" | "gamepad">("actuation");
const gamepadBase = ref<GamepadConfig>(defaultGamepad());
const gamepadSelected = ref<GamepadControl | null>(null);
const gamepadEdit = ref<GamepadConfig | null>(null);
const gamepadReady = ref(false);
const gamepadInput = ref<GamepadInput>({ depth: [], running: false, error: null });
const gamepadConfig = computed(() => gamepadEdit.value ?? gamepadBase.value);
const visible = ref(!document.hidden);
const macroState = ref<MacroState | null>(null);
const selection = ref(new Set<number>());
const progress = ref<[number, number] | null>(null); // reads done, reads in all
const docked = computed(() => (dev.mode && dev.panel?.open && dev.panel.dock !== "window" ? dev.panel.dock : null));
const editableCount = computed(() => layout.value.filter((k) => k.editable).length);
const busy = ref(false);
// The backend sends a status only when something changed, so one dropped while busy would be lost.
let missed = false;
const profiles = ref<ProfilesView | null>(null);
const removing = ref<number | null>(null); // asked before deleting
const switching = ref<(() => void | Promise<void>) | null>(null); // asked before dropping edits
const message = ref("");
const TABS = ["keys", "lighting", "settings"] as const;
const tab = ref<(typeof TABS)[number]>("keys");
const closing = ref(false);
const closeRemember = ref(false);
const hiding = ref(false); // asked before dropping edits with the window
const offering = ref(false);
const macrosToWrite = ref<number[]>([]); // asked before applying bindings to them
const devices = ref<DevicesView | null>(null);
let loadedProfile: number | null = null; // profile the baseline was read from
let loadedKey: string | null = null; // connection and profile of that read
const forgetting = ref<string | null>(null); // asked before forgetting
const unlisten: UnlistenFn[] = [];
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const keyDirty = computed(
  () =>
    new Set([
      ...Object.keys(edits.value),
      ...Object.keys(rapidEdits.value),
      ...Object.keys(bindEdits.value),
      ...Object.keys(hsEdits.value),
      ...Object.keys(hsBindEdits.value),
    ]).size +
    (snapEdit.value ? 1 : 0) +
    (crtEdit.value == null ? 0 : 1),
);
const dirty = computed(() => keyDirty.value + (gamepadEdit.value ? 1 : 0));
trackUnapplied(() => dirty.value > 0);
const online = computed(() => !!status.value?.device);
const writable = computed(() => online.value && !status.value?.synapse && !busy.value);
// An offline keyboard still has its profiles, which live in the app.
const editable = computed(() => !!status.value && !status.value.synapse && !busy.value);
const shown = computed(() => devices.value?.devices.find((d) => d.id === keyboard.value) ?? null);
const driver = computed(() => !!status.value?.driver_mode);
const fwRapid = computed(() => !!status.value?.analog?.firmware_rapid);
// Rapid Trigger and Snap Tap work now: on the host in driver mode, or in the keyboard's firmware.
const rapidHere = computed(() => driver.value || fwRapid.value);
// The firmware runs Rapid Trigger, so continuous is one switch for the profile, not per key.
const fwHw = computed(() => fwRapid.value && !driver.value);
const crt = computed(() => crtEdit.value ?? crtBase.value);
const retryGamepad = computed(() => gamepadReady.value && gamepadBase.value.enabled && driver.value && !!gamepadInput.value.error);
const canApply = computed(
  () =>
    (writable.value && (dirty.value > 0 || retryGamepad.value)) ||
    (editable.value && gamepadReady.value && !!gamepadEdit.value && keyDirty.value === 0),
);
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
const valueOf = (k: number) => edits.value[k] ?? baseline.value[k];
const hsOf = (k: number) => hsEdits.value[k] ?? hsBase.value[k];
const selectedValue = computed(() => common(Array.from(selection.value, valueOf)));
const selectedHs = computed(() => common(Array.from(selection.value, hsOf)));
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
const hsEdited = computed(() => new Set(Object.keys(section.value === "actuation" ? hsEdits.value : hsBindEdits.value).map(Number)));

function takeHypershift(h: Hypershift) {
  hsBase.value = h.values;
  hsBindBase.value = h.bindings;
  hsUnsaved.value = new Set(h.unsaved);
}

const rapidOf = (k: number) => rapidEdits.value[k] ?? rapidBase.value[k] ?? DEFAULT_RAPID;
const sameRapid = (a: Rapid, b: Rapid) =>
  a.enabled === b.enabled && a.press === b.press && a.release === b.release && a.continuous === b.continuous;
const modeOf = (r: Rapid): RapidMode => (r.enabled ? (r.continuous && !fwHw.value ? "crt" : "rt") : "off");

const selectedRapid = computed(() => {
  const r = Array.from(selection.value, rapidOf);
  return { mode: common(r.map(modeOf)), press: common(r.map((x) => x.press)), release: common(r.map((x) => x.release)) };
});
const split = computed(() => splitOn.value || Array.from(selection.value, rapidOf).some((r) => r.release !== r.press));
const rapidKeys = computed(
  () =>
    new Map(
      layout.value
        .filter((k) => rapidOf(k.key).enabled)
        .map((k) => [k.key, modeOf(rapidOf(k.key)) === "crt" || (fwHw.value && crt.value) ? "CRT" : "RT"]),
    ),
);
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

watch(
  () => [dev.mode, dev.mode && !!dev.panel?.open, dev.panel?.dock, dev.panel?.size] as const,
  ([mode, on, dock, size]) => {
    if (dock && size) void applyLayout(mode, on, dock, size, dev.window, t("dev.title"));
  },
);

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
      if (!sameAction(a, factory(k))) out[k] = shortLabel(a, byKey.value, t);
    }
  }
  return out;
});
const mapHsBinds = computed(() => {
  const out: KeyMap<{ text: string; icon?: System }> = {};
  if (section.value !== "bindings") return out;
  for (const { key: k } of layout.value) {
    const a = hsBindingOf(k);
    if (!a || sameAction(a, bindingOf(k) ?? factory(k))) continue;
    out[k] = { text: shortLabel(a, byKey.value, t), icon: a.type === "system" ? a.action : undefined };
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
  if (!keyboard.value) return;
  try {
    const id = keyboard.value;
    const r = await call<MacroState>("macros");
    if (ours(id)) macroState.value = r;
  } catch (error) {
    showError(error);
  }
}

async function loadProfiles() {
  if (!keyboard.value) return;
  try {
    const id = keyboard.value;
    const r = await call<ProfilesView>("profiles");
    if (ours(id)) profiles.value = r;
  } catch (error) {
    showError(error);
  }
}

function editGamepad(config: GamepadConfig) {
  gamepadEdit.value = sameGamepad(config, gamepadBase.value) ? null : config;
}

async function loadGamepad(id: string, profile: number | null) {
  gamepadReady.value = false;
  if (profile === null) return;
  const config = await call<GamepadConfig>("gamepad_config", {}, id);
  if (!ours(id) || loadedProfile !== profile) return;
  gamepadBase.value = config;
  if (gamepadEdit.value && sameGamepad(gamepadEdit.value, config)) gamepadEdit.value = null;
  gamepadReady.value = true;
}

async function storeGamepad(config = gamepadEdit.value) {
  const id = keyboard.value;
  const profile = loadedProfile;
  if (!id || profile === null || !config) return;
  await call("set_gamepad_config", { profile, config }, id);
  if (!ours(id) || loadedProfile !== profile) return;
  gamepadBase.value = config;
  gamepadEdit.value = null;
}

watch([keyboard, () => status.value?.profile], () => {
  gamepadInput.value = { depth: [], running: false, error: null };
  gamepadReady.value = false;
});

watch([keyboard, () => status.value?.profile, tab, section, driver, online, busy, visible], (_, __, cleanup) => {
  const id = keyboard.value;
  gamepadInput.value = { depth: [], running: false, error: null };
  if (!id || tab.value !== "keys" || section.value !== "gamepad" || !online.value || !visible.value || busy.value) return;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  cleanup(() => {
    stopped = true;
    clearTimeout(timer);
  });
  const poll = async () => {
    let delay = 33;
    try {
      const input = await call<GamepadInput>("gamepad_input", {}, id);
      if (!stopped) gamepadInput.value = input;
    } catch (error) {
      if (!stopped) gamepadInput.value = { depth: [], running: false, error: String(error) };
      delay = 500;
    }
    if (!stopped)
      timer = setTimeout(() => {
        void poll();
      }, delay);
  };
  void poll();
});

async function load() {
  const id = keyboard.value;
  // Before any keyboard is known there is nothing to read, and the error would hide the status text.
  if (id === null) return;
  busy.value = true;
  progress.value = [0, 1];
  let stale = false;
  try {
    const {
      values: base,
      unsaved: keys,
      rapid,
      snap: snapRead,
      continuous,
      bindings,
      unsaved_bindings,
      hypershift,
      profile,
    } = await call<Actuation>("read_all");
    // The keyboard changed while reading: nothing here is for the new one.
    if (!ours(id)) {
      stale = true;
      return;
    }
    baseline.value = base;
    rapidBase.value = rapid;
    rapidEdits.value = Object.fromEntries(
      Object.entries(rapidEdits.value).filter(([k, r]) => !sameRapid(r, rapid[Number(k)] ?? DEFAULT_RAPID)),
    );
    snapBase.value = snapRead;
    if (snapEdit.value && sameSnap(snapEdit.value, snapRead)) snapEdit.value = null;
    crtBase.value = continuous;
    if (crtEdit.value === continuous) crtEdit.value = null;
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
    loadedKey = loadKey({ device: !!status.value?.device, profile });
    message.value = "";
    await loadGamepad(id, profile);
    await loadMacros();
    await loadProfiles();
  } catch (error) {
    if (ours(id)) message.value = String(error);
  } finally {
    progress.value = null;
    busy.value = false;
  }
  // eslint-disable-next-line @typescript-eslint/no-use-before-define -- refresh and load call each other
  if (stale) await refresh();
}

function revert() {
  gamepadEdit.value = null;
  edits.value = {};
  rapidEdits.value = {};
  snapEdit.value = null;
  crtEdit.value = null;
  errors.value = {};
  bindEdits.value = {};
  hsEdits.value = {};
  hsBindEdits.value = {};
  bindErrors.value = {};
}

async function onStatus(s: DeviceStatus) {
  if (!ours(s.id)) return;
  if (s.error) showError(s.error);
  // Mid read or apply the table is being rewritten; the status is asked again once it is done.
  if (busy.value) {
    missed = true;
    return;
  }
  status.value = s;
  if (s.model && s.model !== layoutModel) {
    layoutModel = s.model;
    layout.value = await invoke<KeyView[]>("layout", { model: s.model });
  }
  if (!s.device) progress.value = null;
  if (s.synapse) {
    loadedProfile = null;
    loadedKey = null;
    return;
  }
  const act = statusAction(s, loadedProfile, loadedKey);
  if (act.revert) revert();
  if (act.load) await load();
}

watch(busy, (b) => {
  if (b || !missed) return;
  missed = false;
  // eslint-disable-next-line @typescript-eslint/no-use-before-define -- refresh and load call each other
  void refresh();
});

async function refresh() {
  try {
    await onStatus(await invoke<DeviceStatus>("status", { id: keyboard.value }));
  } catch (error) {
    message.value = String(error);
  }
}

async function settle() {
  const next = pick(devices.value?.devices ?? [], keyboard.value, devices.value?.last ?? null);
  if (next === keyboard.value) return;
  keyboard.value = next;
  // Another keyboard: nothing read from the last one applies.
  revert();
  loadedProfile = null;
  loadedKey = null;
  layoutModel = null;
  await reloadLog();
  await refresh();
}

async function loadDevices() {
  try {
    devices.value = await invoke<DevicesView>("devices");
  } catch (error) {
    showError(error);
    return;
  }
  await settle();
}

async function renameKeyboard(id: string, name: string) {
  try {
    devices.value = await call<DevicesView>("rename_device", { name }, id);
  } catch (error) {
    showError(error);
  }
}

async function confirmForget() {
  const id = forgetting.value;
  forgetting.value = null;
  if (id === null) return;
  try {
    devices.value = await call<DevicesView>("forget_device", {}, id);
  } catch (error) {
    showError(error);
    return;
  }
  await settle();
}

async function setMode(on: boolean) {
  if (on === driver.value) return;
  busy.value = true;
  try {
    await call("set_driver_mode", { on });
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

const FILTERS = [{ name: "EasyRazer", extensions: ["json"] }];

async function exportProfile(profile: number) {
  const kb = keyboard.value;
  const name = profiles.value?.profiles.find((p) => p.id === profile)?.name ?? "profile";
  const path = await saveDialog({ defaultPath: `${name}.easyrazer.json`, filters: FILTERS });
  if (!path) return;
  try {
    await call("export_profile", { profile, path }, kb);
    message.value = t("profiles.exported", { name });
  } catch (error) {
    showError(error);
  }
}

async function importProfile() {
  const kb = keyboard.value;
  const path = await open({ multiple: false, directory: false, filters: FILTERS });
  if (typeof path !== "string") return;
  try {
    const r = await call<ImportView>("import_profile", { path }, kb);
    if (!ours(kb)) return;
    profiles.value = r.view;
    message.value =
      r.dropped > 0 ? t("profiles.importedDropped", { name: r.name, n: r.dropped }) : t("profiles.imported", { name: r.name });
  } catch (error) {
    showError(error);
  }
}

async function profileOp(command: string, args: Record<string, unknown> = {}) {
  busy.value = true;
  try {
    const id = keyboard.value;
    const r = await call<ProfilesView>(command, args);
    if (ours(id)) profiles.value = r;
  } catch (error) {
    showError(error);
  } finally {
    busy.value = false;
  }
  await refresh();
}

// Loading another profile drops unapplied edits; ask first.
function switchTo(op: () => void | Promise<void>) {
  if (dirty.value > 0) switching.value = op;
  else void op();
}

function confirmSwitch() {
  const op = switching.value;
  switching.value = null;
  revert();
  void op?.();
}

function onPickKeyboard(id: string) {
  if (id === keyboard.value) return;
  switchTo(async () => {
    keyboard.value = id;
    revert();
    loadedProfile = null;
    loadedKey = null;
    layoutModel = null;
    try {
      await invoke("set_last_device", { id });
    } catch (error) {
      showError(error);
    }
    await reloadLog();
    await refresh();
  });
}

function confirmRemove() {
  const id = removing.value;
  removing.value = null;
  if (id != null) void profileOp("delete_profile", { profile: id });
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
  const id = keyboard.value;
  busy.value = true;
  let failed = false;
  try {
    if (command === "apply") await storeGamepad(gamepadEdit.value ?? (retryGamepad.value ? gamepadBase.value : null));
    const changes = Object.entries(edits.value).map(([k, mm]) => [Number(k), mm]);
    const rapid = command === "apply" ? rapidEdits.value : null;
    const snapNext = command === "apply" ? snapEdit.value : null;
    const crtNext = command === "apply" ? crtEdit.value : null;
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
    } = await call<WriteResult>(
      command,
      rapid ? { changes, rapid, snap: snapNext, continuous: crtNext, bindings, hypershift } : { changes, bindings, hypershift },
    );
    // The window moved to another keyboard meanwhile; its edits were dropped with the switch.
    if (!ours(id)) return;
    // The backend takes the layer whole or refuses it, so a result means every edit is in.
    takeHypershift(layer);
    hsEdits.value = {};
    hsBindEdits.value = {};
    if (rapid) {
      rapidBase.value = { ...rapidBase.value, ...rapid };
      rapidEdits.value = {};
      if (snapNext) snapBase.value = snapNext;
      snapEdit.value = null;
      if (crtNext != null) crtBase.value = crtNext;
      crtEdit.value = null;
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
  if (keyDirty.value === 0 && (gamepadEdit.value || retryGamepad.value)) {
    const config = gamepadEdit.value ?? gamepadBase.value;
    busy.value = true;
    try {
      await storeGamepad(config);
      message.value = t("gamepad.applied");
    } catch (error) {
      showError(error);
    } finally {
      busy.value = false;
    }
    await refresh();
    return;
  }
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
  const kb = keyboard.value;
  try {
    for (const id of ids) {
      const r = await call<MacroState>("write_macro", { macroId: id }, kb);
      if (!ours(kb)) return;
      macroState.value = r;
    }
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

async function onClose(action: CloseAction) {
  closing.value = false;
  // The choice still applies this session even if it could not be saved.
  if (closeRemember.value) await invoke("set_close_action", { action }).catch(showError);
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

let firstKeyboard: Promise<void> | null = null;
const onVisibility = () => (visible.value = !document.hidden);

function escapeSelection(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  selection.value = new Set();
  gamepadSelected.value = null;
}

onMounted(async () => {
  window.addEventListener("keydown", escapeSelection);
  document.addEventListener("visibilitychange", onVisibility);
  // Before anything that takes the device lock: a connect can hold it for the whole read.
  layout.value = await invoke<KeyView[]>("layout");
  unlisten.push(
    await listen("close-requested", () => {
      closeRemember.value = false;
      closing.value = true;
    }),
    await listen("hide-requested", () => {
      if (hasUnapplied()) hiding.value = true;
      else void hide();
    }),
    await listen<string>("app-error", (e) => {
      showError(e.payload);
    }),
  );
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
  unlisten.push(
    await listen<[string, number, number, number | null, number | null]>("read-progress", (e) => {
      // The first keyboard attaches, and reads, before its `devices` event arrives.
      if (keyboard.value === null) firstKeyboard ??= loadDevices().finally(() => (firstKeyboard = null));
      if (!ours(e.payload[0])) return;
      const [, done, total, key, mm] = e.payload;
      progress.value = [done, total];
      if (key != null && mm != null) baseline.value[key] = mm;
    }),
  );
  await invoke("window_ready");
  await startDev();
  await loadDevices();
  await loadMacros();
  unlisten.push(
    await listen<[string, number, boolean]>("gamepad-enabled", (e) => {
      const [id, profile, enabled] = e.payload;
      if (!ours(id) || loadedProfile !== profile) return;
      gamepadBase.value = { ...gamepadBase.value, enabled };
      if (gamepadEdit.value) editGamepad({ ...gamepadEdit.value, enabled });
    }),
    await listen<[string, boolean]>("snap-tap", (e) => {
      const [id, on] = e.payload;
      if (!ours(id)) return;
      snapBase.value = { ...snapBase.value, enabled: on };
      if (snapEdit.value) setSnap({ ...snapEdit.value, enabled: on });
    }),
    await listen<DeviceStatus>("status", (e) => {
      void onStatus(e.payload);
    }),
    await listen("devices", () => {
      void loadDevices();
    }),
  );
  // With a keyboard picked, loadDevices has refreshed already.
  if (keyboard.value === null) await refresh();
});

onUnmounted(() => {
  window.removeEventListener("keydown", escapeSelection);
  document.removeEventListener("visibilitychange", onVisibility);
  for (const f of unlisten) f();
  clearTimeout(errorTimer);
});
</script>

<template>
  <div class="flex h-screen" :class="{ 'flex-row-reverse': docked === 'left', 'flex-col': docked === 'bottom' }">
    <div class="flex-1 min-h-0 min-w-0 overflow-auto">
      <main class="mx-auto p-4 flex flex-col gap-4 max-w-[1153px]" :class="{ 'h-full': tab === 'keys' && section === 'gamepad' }">
        <nav class="flex gap-1">
          <button
            v-for="name in TABS"
            :key="name"
            class="rounded-b-none"
            :class="
              tab === name
                ? 'border-x-transparent border-t-transparent border-b-2 border-b-accent'
                : 'bg-panel text-muted border-transparent'
            "
            :aria-current="tab === name ? 'page' : undefined"
            @click="tab = name"
          >
            {{ $t(`tabs.${name}`) }}
          </button>
          <DeviceMenu
            v-if="devices && devices.devices.length > 1"
            class="ml-auto"
            :devices="devices.devices"
            :selected="keyboard"
            @pick="onPickKeyboard"
            @rename="renameKeyboard"
            @forget="(id) => (forgetting = id)"
          />
          <ProfileMenu
            :class="{ 'ml-auto': !(devices && devices.devices.length > 1) }"
            :view="profiles"
            :writable="writable"
            :editable="editable"
            :next-combo="nextCombo"
            @load="(id) => switchTo(() => profileOp('load_profile', { profile: id }))"
            @create="switchTo(() => profileOp('create_profile'))"
            @duplicate="(id) => profileOp('duplicate_profile', { profile: id })"
            @rename="(id, name) => profileOp('rename_profile', { profile: id, name })"
            @remove="(id) => (removing = id)"
            @startup="(id) => profileOp('set_startup', { profile: id })"
            @free="(id) => profileOp('free_slot', { profile: id })"
            @write="(id) => profileOp('write_profile', { profile: id })"
            @export="exportProfile"
            @import="importProfile"
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
            <button
              v-if="dev.mode"
              class="font-bold font-mono px-3 py-0"
              :class="dev.panel?.open ? 'border-dev text-dev' : 'text-muted'"
              :title="$t('dev.title')"
              :aria-pressed="!!dev.panel?.open"
              @click="setPanel({ open: !dev.panel?.open })"
            >
              &lt;/&gt;
            </button>
          </span>
        </nav>
        <StatusBar
          :status="status"
          :progress="progress"
          :unsupported="devices?.unsupported ?? null"
          :name="shown?.name ?? null"
          :message="tab !== 'lighting' ? message : ''"
          :error="appError"
        />
        <!-- With the map shown, the tab is as wide as the full-size map on every model, not the window. -->
        <div
          v-if="tab === 'keys'"
          class="flex flex-col gap-4"
          :class="{ 'min-w-[1121px]': section === 'actuation' || section === 'bindings', 'flex-1 min-h-0': section === 'gamepad' }"
        >
          <div class="flex gap-3 items-center">
            <span class="inline-flex">
              <button
                v-for="s in ['actuation', 'bindings', 'macros', 'gamepad'] as const"
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
              :count="section === 'gamepad' ? Number(gamepadSelected !== null) : selection.size"
              :single="section === 'bindings' || section === 'gamepad'"
              @select-all="selectAll"
              @clear="section === 'gamepad' ? (gamepadSelected = null) : (selection = new Set())"
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
          <template v-else-if="section === 'gamepad'">
            <GamepadTab
              :key="`${keyboard}:${status?.profile}`"
              v-model:selected="gamepadSelected"
              :config="gamepadConfig"
              :depth="gamepadInput.depth"
              :layout="layout"
              :editable="editable && gamepadReady"
              :driver="driver"
              :running="gamepadInput.running"
              :error="gamepadInput.error"
              @update="editGamepad"
            />
            <ActionBar v-bind="actions" apply-only @revert="revert" @apply="apply" />
          </template>
          <template v-else>
            <KeyboardMap
              v-model:selection="selection"
              :layout="layout"
              :values="mapValues"
              :inline="section === 'bindings'"
              :edited="mapEdited"
              :errors="section === 'actuation' ? errors : bindErrors"
              :unsaved="section === 'actuation' ? unsaved : bindUnsaved"
              :rapid="rapidHere && section === 'actuation' ? rapidKeys : new Map()"
              :snap="rapidHere && section === 'actuation' ? snapColors(snap) : {}"
              :rapid-edited="rapidEdited"
              :hs-values="mapHs"
              :hs-edited="hsEdited"
              :hs-binds="mapHsBinds"
              :class="{ 'opacity-60': !online }"
              :single="section === 'bindings'"
              :loaded="progress && Math.floor((progress[0] / progress[1]) * editableCount)"
            />
            <div class="gap-4 grid grid-cols-2" :class="{ 'opacity-60': !online }" :inert="!online">
              <div class="px-4 py-3 card flex flex-col gap-3">
                <ActuationCard
                  v-if="section === 'actuation'"
                  :count="selection.size"
                  :value="selectedValue"
                  :rapid="selectedRapid"
                  :crt="fwHw ? crt : null"
                  :split="split"
                  :hs="selectedHs"
                  :hs-split="hsSplit"
                  :driver="driver"
                  :analog="status?.analog ?? null"
                  @set="setValue"
                  @rapid="
                    (m) => setRapid((r) => ({ ...r, enabled: m !== 'off', continuous: m === 'off' || fwHw ? r.continuous : m === 'crt' }))
                  "
                  @crt="(on) => (crtEdit = on === crtBase ? null : on)"
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
                  :firmware="fwRapid && !driver"
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
            <DevKeys v-if="dev.mode" :keys="[...selection]" :depth="section === 'actuation'" :driver="driver" />
            <ActionBar v-bind="actions" @revert="revert" @apply="apply" @write="save" />
          </template>
        </div>
        <LightingTab v-else-if="tab === 'lighting'" :key="keyboard ?? undefined" :status="status" />
        <SettingsTab v-else />
        <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
        <ConfirmWrite
          v-if="macrosToWrite.length > 0"
          :text="$t('bindings.writeMacros', { names: macrosToWrite.map((id) => macroState?.macros[id]?.name ?? `#${id}`).join(', ') })"
          @yes="onWriteMacros"
          @no="macrosToWrite = []"
        />
        <ModalDialog v-if="closing" :title="$t('dialogs.close.title')" @cancel="closing = false">
          <p>{{ $t("dialogs.close.text") }}</p>
          <p v-if="hasUnapplied()">{{ $t("dialogs.close.unapplied") }}</p>
          <label class="dlg-check"><input v-model="closeRemember" type="checkbox" /> {{ $t("common.dontAsk") }}</label>
          <template #actions>
            <button @click="onClose('exit')">{{ $t("dialogs.close.exit") }}</button>
            <button class="primary" autofocus @click="onClose('tray')">{{ $t("dialogs.close.tray") }}</button>
          </template>
        </ModalDialog>
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
        <ModalDialog v-if="forgetting != null" :title="$t('devices.forget')" @cancel="forgetting = null">
          <p>{{ $t("devices.forgetConfirm", { name: devices?.devices.find((d) => d.id === forgetting)?.name ?? "" }) }}</p>
          <template #actions>
            <button autofocus @click="forgetting = null">{{ $t("profiles.cancel") }}</button>
            <button class="primary" @click="confirmForget">{{ $t("devices.forget") }}</button>
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
    </div>
    <DevPanel
      v-if="docked && dev.panel"
      class="shrink-0"
      :dock="docked"
      :style="docked === 'bottom' ? { height: `${dev.panel.size}px` } : { width: `${dev.panel.size}px` }"
    />
  </div>
</template>
