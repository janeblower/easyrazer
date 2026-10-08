import { createApp } from "vue";
import App from "./App.vue";
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
    macros: { macros: {}, free: null },
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
      snap: { enabled: false, groups: [] },
      bindings: {},
      unsaved_bindings: [],
      hypershift: { values: all((_, i) => (i % 2 ? 3 : 2)), bindings: {}, unsaved: [] },
      profile: 1,
    },
  };
  mockIPC((cmd) => replies[cmd], { shouldMockEvents: true });
}

void mockBackend().then(() => createApp(App).use(i18n).mount("#app"));
