import { reactive, ref, shallowRef } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppSettings, CommandInfo, DevPanel, DevWindow, LogEntry } from "./types";

const CAPACITY = 1000; // logged::CAPACITY

export const dev = reactive({
  mode: false,
  panel: null as DevPanel | null,
  window: null as DevWindow | null,
  language: null as string | null,
});
// Replaced, never mutated: a full read logs hundreds of exchanges, and each change re-renders the log.
export const log = shallowRef<LogEntry[]>([]);
export const registry = ref<Record<string, CommandInfo>>({});

export const nameOf = (cmd: string) => Object.entries(registry.value).find(([, c]) => c.cmd === cmd)?.[0];

async function loadSettings() {
  const s = await invoke<AppSettings>("app_settings");
  Object.assign(dev, { mode: s.dev_mode, panel: s.dev_panel, window: s.dev_window, language: s.language });
}

let started: Promise<void> | undefined;
let pending: LogEntry[] = [];

// A timer, not a frame: frames stop while the window is hidden and the batch would grow unbounded.
function flush() {
  const last = log.value.at(-1)?.seq ?? -1;
  log.value = [...log.value, ...pending.filter((e) => e.seq > last)].slice(-CAPACITY);
  pending = [];
}

/** Loads the mode, the log and the registry once per window and follows their changes. */
export async function startDev() {
  started ??= (async () => {
    await listen("settings-changed", () => {
      void loadSettings();
    });
    await listen<LogEntry>("hid-log", (e) => {
      if (pending.push(e.payload) === 1) setTimeout(flush, 100);
    });
    await loadSettings();
    const [fetched, commands] = await Promise.all([invoke<LogEntry[]>("dev_log"), invoke<Record<string, CommandInfo>>("dev_commands")]);
    // Entries that arrived while the log was fetched.
    const last = fetched.at(-1)?.seq ?? -1;
    log.value = [...fetched, ...log.value.filter((e) => e.seq > last)];
    registry.value = commands;
  })();
  await started;
}

export async function setPanel(p: Partial<DevPanel>) {
  if (!dev.panel) return;
  dev.panel = { ...dev.panel, ...p };
  await invoke("set_dev_panel", { panel: dev.panel });
}
