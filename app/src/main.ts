import { createApp } from "vue";
import App from "./App.vue";
import DevPanel from "./components/DevPanel.vue";
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
  const state = {
    values: all(() => 2) as Record<number, unknown>,
    unsaved: [] as number[],
    rapid: all(() => ({ enabled: true, press: 0.4, release: 0.4 })) as Record<number, unknown>,
    snap: {
      enabled: true,
      groups: [
        { keys: [31, 33], rule: "last" },
        { keys: [18, 32], rule: "neutral" },
        { keys: [79, 89], rule: "deeper" },
      ],
    } as unknown,
    bindings: {
      112: { type: "macro", id: 1, mode: "times", count: 1 },
      113: { type: "macro", id: 2, mode: "toggle", count: 1 },
    } as Record<number, unknown>,
    unsaved_bindings: [] as number[],
    hypershift: {
      values: all((_, i) => (i % 2 ? 3 : 2)) as Record<number, unknown>,
      bindings: all(hsFactory) as Record<number, unknown>,
      unsaved: [] as number[],
    },
    profile: 1,
  };
  type Pairs = [number, unknown][];
  interface Write {
    changes: Pairs;
    bindings: Pairs;
    hypershift: { values: Pairs; bindings: Pairs };
    rapid?: Record<number, unknown>;
    snap?: unknown;
  }
  const { emit } = await import("@tauri-apps/api/event");
  const hex2 = (n: number) => n.toString(16).toUpperCase().padStart(2, "0");
  const devSettings = { dev_mode: false, dev_panel: null as unknown, dev_window: null as unknown };
  let seq = 0;
  const devLog: unknown[] = [];
  const entry = (cmd: string, args: number[], reply: number[] = [], live = true) => {
    const e = {
      seq: seq++,
      time: Date.now(),
      cmd,
      size: args.length,
      args,
      status: "Ok",
      reply,
      polls: 0,
      ms: 3,
      error: null,
      background: false,
      tx: args,
      rx: reply,
    };
    devLog.push(e);
    if (live) void emit("hid-log", e);
    return e;
  };
  entry("00:84", [], [3, 0], false);
  entry("05:84", [], [1], false);
  entry("02:92", [0, 30, 0], [0, 30, 0, 128, 112, 2, 2, 0, 4], false);
  const info = (cmd: string, help: string, size: number | null, args: [string, string][] = []) => ({ cmd, size, help, args, example: "" });
  const devReplies: Record<string, (a: Record<string, unknown>) => unknown> = {
    set_dev_mode: ({ on }) => {
      devSettings.dev_mode = on as boolean;
      devSettings.dev_panel ??= { open: true, dock: "right", size: 420 };
      void emit("settings-changed");
    },
    set_dev_panel: ({ panel }) => {
      devSettings.dev_panel = panel;
      void emit("settings-changed");
    },
    set_dev_window: ({ geom }) => {
      devSettings.dev_window = geom;
    },
    dev_log: () => devLog,
    dev_commands: () => ({
      get_mode: info("00:84", "Device mode in the first reply byte: 00 hardware, 03 driver.", 2),
      set_mode: info("00:04", "Switches the device mode.", 2, [
        ["mode", "00 hardware, 03 driver"],
        ["reserved", "00"],
      ]),
      get_key_assignment: info("02:92", "Reads one key's press and release points and its function on a layer.", 80, [
        ["profile", ""],
        ["key", "fwID"],
        ["layer", ""],
      ]),
    }),
    dev_device: () => ({
      model: "Phantom Analog 104",
      vid: 0x15_32,
      pid: 0x02_66,
      interface: 3,
      tid: 0x1f,
      lighting_protocol: "extended",
      led: 5,
      lamp_array: true,
      mode: 3,
      startup_slot: 1,
      loaded_profile: 1,
      firmware: "1.02",
      serial: "PM0000000000000",
    }),
    dev_exchange: ({ class: c, id, args }) => entry(`${hex2(c as number)}:${hex2(id as number)}`, args as number[], [0]),
    dev_keys: ({ keys }) =>
      Object.fromEntries(
        (keys as number[]).map((k) => [
          k,
          { name: "KEY", normal: { thr_low: 128, thr_high: 112, fn_id: 2, fn_data: [2, 0, 4] }, hypershift: null },
        ]),
      ),
    dev_macro: () => [1, 4, 0, 2, 1, 4],
    dev_diagnostics: ({ uncensored }) => `EasyRazer dev\nuncensored: ${String(uncensored)}`,
  };
  // Applied edits stay unsaved until a save; a save writes everything.
  const write = (cmd: string, w: Write) => {
    const keep = (into: Record<number, unknown>, pairs: Pairs, unsaved: number[]) => {
      for (const [k, v] of pairs) into[k] = v;
      return cmd === "save" ? [] : [...new Set([...unsaved, ...pairs.map(([k]) => k)])];
    };
    state.unsaved = keep(state.values, w.changes, state.unsaved);
    state.unsaved_bindings = keep(state.bindings, w.bindings, state.unsaved_bindings);
    const hs = state.hypershift;
    hs.unsaved = keep(hs.bindings, w.hypershift.bindings, keep(hs.values, w.hypershift.values, hs.unsaved));
    if (w.rapid) Object.assign(state.rapid, w.rapid);
    if (w.snap) state.snap = w.snap;
    for (const [k] of w.changes) entry("02:12", [0, k, 0]);
    return {
      results: w.changes.map(([key, mm]) => ({ status: "ok", key, mm })),
      unsaved: state.unsaved,
      bindings: w.bindings.map(([key, action]) => ({ status: "ok", key, action })),
      unsaved_bindings: state.unsaved_bindings,
      hypershift: hs,
    };
  };
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
    read_all: state,
  };
  mockIPC(
    (cmd, args) => {
      if (cmd === "apply" || cmd === "save") return write(cmd, args as unknown as Write);
      if (cmd === "app_settings") return { ...(replies.app_settings as object), ...devSettings };
      return cmd in devReplies ? devReplies[cmd](args as Record<string, unknown>) : replies[cmd];
    },
    { shouldMockEvents: true },
  );
}

const detached = location.hash === "#devtools";
void mockBackend().then(() => {
  const app = detached ? createApp(DevPanel, { detached: true }) : createApp(App);
  app.use(i18n).mount("#app");
});
