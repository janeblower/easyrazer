import { createApp } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import "virtual:uno.css";
import "./style.css";

// A plain browser on the Vite dev server has no backend: draw the map from a layout snapshot.
async function mockBackend() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;
  const [{ mockIPC }, { default: layout }] = await Promise.all([import("@tauri-apps/api/mocks"), import("./dev-layout.json")]);
  const replies: Record<string, unknown> = {
    layout,
    app_settings: { autostart_offered: true, language: null },
    status: {
      device: false,
      synapse: false,
      driver_mode: false,
      profile: null,
      profile_name: null,
      model: null,
      unsupported: null,
      error: null,
    },
  };
  mockIPC((cmd) => replies[cmd], { shouldMockEvents: true });
}

void mockBackend().then(() => createApp(App).use(i18n).mount("#app"));
