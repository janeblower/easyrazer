import { reactive, ref, shallowRef } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { call, keyboard, ours, pick } from "./keyboard";
import type { AppSettings, CommandInfo, DevPanel, DevWindow, DevicesView, LogEntry } from "./types";

export const dev = reactive({
  mode: false,
  /** Entries the backend keeps in this mode; the window keeps as many. */
  logSize: 10_000,
  panel: null as DevPanel | null,
  window: null as DevWindow | null,
  language: null as string | null,
});
// Replaced, never mutated: a full read logs hundreds of exchanges, and each change re-renders the log.
export const log = shallowRef<LogEntry[]>([]);
export const registry = ref<Record<string, CommandInfo>>({});

/** The exchange whose details are open, by `seq`; the console opens it too. */
export const selected = ref<number | null>(null);
/** Exchanges this window's console sent. */
export const fromConsole = reactive(new Set<number>());

export const infoOf = (cmd: string) => Object.values(registry.value).find((c) => c.cmd === cmd);

export const nameOf = (cmd: string) => Object.entries(registry.value).find(([, c]) => c.cmd === cmd)?.[0];

async function loadSettings() {
  const s = await invoke<AppSettings>("app_settings");
  Object.assign(dev, { mode: s.dev_mode, logSize: s.dev_log_size, panel: s.dev_panel, window: s.dev_window, language: s.language });
}

let started: Promise<void> | undefined;
let pending: LogEntry[] = [];

// A timer, not a frame: frames stop while the window is hidden and the batch would grow unbounded.
function flush() {
  const last = log.value.at(-1)?.seq ?? -1;
  log.value = [...log.value, ...pending.filter((e) => e.seq > last)].slice(-dev.logSize);
  pending = [];
}

/** Refetches the log of the selected keyboard. */
export async function reloadLog() {
  log.value = [];
  pending = [];
  if (!keyboard.value) return;
  const fetched = await call<LogEntry[]>("dev_log");
  // Entries that arrived while the log was fetched.
  const last = fetched.at(-1)?.seq ?? -1;
  log.value = [...fetched, ...log.value.filter((e) => e.seq > last)];
}

/** The detached window has no App to pick a keyboard, so it follows the same choice itself. */
export async function followKeyboard() {
  const take = async () => {
    const view = await invoke<DevicesView>("devices");
    const next = pick(view.devices, keyboard.value);
    if (next === keyboard.value) return;
    keyboard.value = next;
    if (started) await reloadLog();
  };
  await listen("devices", () => {
    void take();
  });
  await take();
}

/** Loads the mode, the log and the registry once per window and follows their changes. */
export async function startDev() {
  started ??= (async () => {
    await listen("settings-changed", () => {
      void loadSettings();
    });
    await listen<{ id: string; entry: LogEntry }>("hid-log", (e) => {
      if (ours(e.payload.id) && pending.push(e.payload.entry) === 1) setTimeout(flush, 100);
    });
    await loadSettings();
    await Promise.all([reloadLog(), invoke<Record<string, CommandInfo>>("dev_commands").then((c) => (registry.value = c))]);
  })();
  await started;
}

export async function setPanel(p: Partial<DevPanel>) {
  if (!dev.panel) return;
  dev.panel = { ...dev.panel, ...p };
  await invoke("set_dev_panel", { panel: dev.panel });
}
