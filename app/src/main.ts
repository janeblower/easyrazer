import { createApp } from "vue";
import App from "./App.vue";
import { hsFactory } from "./bindings";
import { i18n } from "./i18n";
import "virtual:uno.css";
import "./style.css";

// A plain browser on the Vite dev server has no backend: fake a keyboard with every key indicator on.
async function mockBackend() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;
  const [{ mockIPC }, { default: layout }] = await Promise.all([import("@tauri-apps/api/mocks"), import("./dev-layout.json")]);
  const keys = layout.filter((k) => k.editable).map((k) => k.key);
  const all = <T>(v: (k: number, i: number) => T) => Object.fromEntries(keys.map((k, i) => [k, v(k, i)]));
  const replies: Record<string, unknown> = {
    layout,
    app_settings: { autostart_offered: true, language: null },
    macros: {
      macros: {
        1: {
          name: "Ctrl+C, Ctrl+V",
          events: [
            { type: "key", key: 58, down: true },
            { type: "key", key: 48, down: true },
            { type: "key", key: 48, down: false },
            { type: "delay", ms: 50 },
            { type: "key", key: 49, down: true },
            { type: "key", key: 49, down: false },
            { type: "key", key: 58, down: false },
          ],
          written: true,
        },
        2: {
          name: "Double click",
          events: [
            { type: "mouse", button: "left", down: true },
            { type: "mouse", button: "left", down: false },
            { type: "delay", ms: 30 },
            { type: "mouse", button: "left", down: true },
            { type: "mouse", button: "left", down: false },
          ],
          written: true,
        },
        3: {
          name: "Draft",
          events: [
            { type: "key", key: 36, down: true },
            { type: "key", key: 36, down: false },
          ],
          written: false,
        },
      },
      free: 1536,
    },
    profiles: null,
    status: {
      device: true,
      synapse: false,
      // Rapid Trigger shows on the map only in driver mode.
      driver_mode: true,
      profile: 1,
      profile_name: "Dev",
      model: "Huntsman V2 Analog",
      unsupported: null,
      error: null,
    },
    read_all: {
      values: all(() => 2),
      unsaved: [],
      rapid: all(() => ({ enabled: true, press: 0.4, release: 0.4 })),
      snap: {
        enabled: true,
        groups: [
          { keys: [31, 33], rule: "last" },
          { keys: [18, 32], rule: "neutral" },
          { keys: [79, 89], rule: "deeper" },
        ],
      },
      bindings: {
        112: { type: "macro", id: 1, mode: "times", count: 1 },
        113: { type: "macro", id: 2, mode: "toggle", count: 1 },
      },
      unsaved_bindings: [],
      hypershift: { values: all((_, i) => (i % 2 ? 3 : 2)), bindings: all(hsFactory), unsaved: [] },
      profile: 1,
    },
  };
  mockIPC((cmd) => replies[cmd], { shouldMockEvents: true });
}

void mockBackend().then(() => createApp(App).use(i18n).mount("#app"));
