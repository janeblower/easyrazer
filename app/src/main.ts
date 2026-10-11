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
  // Mirrors kb_layout::lighting_keys and lighting::effect_infos for the Huntsman V2 Analog.
  const zone = (key: number, label: string, x: number, y: number, w: number, h: number, shape: string) => ({
    key,
    name: "",
    label,
    x,
    y,
    w,
    h,
    editable: true,
    shape,
  });
  const effect = (name: string, colors = 0, speed: number[] | null = null, dirs: string[] = []) => ({
    name,
    colors,
    dirs,
    speed,
    fast_low: speed !== null,
  });
  const replies: Record<string, unknown> = {
    layout,
    lighting_layout: [
      ...layout.map((k) => ({ ...k, x: k.x + 0.5, y: k.y + 0.5, editable: true })),
      zone(200, "● ● ●", 19, 0.5, 3, 1, "round"),
      zone(201, "◎", 22, 0.5, 1, 1, "round"),
      zone(202, "Edge", 0.1, 0.1, 23.3, 7.05, "ring"),
      zone(203, "Wrist rest", 2, 7.5, 19.5, 1, "key"),
    ],
    lighting_state: {
      effects: [
        effect("off"),
        effect("custom"),
        effect("static", 1),
        effect("breathing_one", 1),
        effect("breathing_two", 2),
        effect("breathing_random"),
        effect("spectrum"),
        effect("wave", 0, [1, 255], ["left", "right"]),
        effect("reactive_key_one", 1),
        effect("reactive_key_random"),
        effect("reactive_ripple_one", 1),
        effect("reactive_ripple_random"),
        effect("starlight_random", 0, [1, 3]),
        effect("starlight_one", 1, [1, 3]),
        effect("starlight_two", 2, [1, 3]),
        effect("fire"),
      ],
      applied: { effect: { name: "wave", dir: "right", speed: 40 }, brightness: 200 },
      saved: { effect: { name: "static", rgb1: [0, 255, 0] }, brightness: 255 },
      dynamic_lighting: false,
      custom: null,
    },
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
      model: "Phantom Analog 104",
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
