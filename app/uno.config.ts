import { defineConfig, presetWind4 } from "unocss";

export default defineConfig({
  // The browser defaults the layout was built on stay; style.css sets the few base styles.
  presets: [presetWind4({ preflights: { reset: false } })],
  theme: {
    colors: {
      bg: "var(--bg)",
      panel: "var(--panel)",
      key: "var(--key)",
      "key-off": "var(--key-off)",
      text: "var(--text)",
      muted: "var(--muted)",
      accent: "var(--accent)",
      edited: "var(--edited)",
      error: "var(--error)",
      line: "#3a3a3a",
      ink: "#0b0b0b",
      "warn-bg": "#3a2a10",
      warn: "#ffcf7a",
    },
  },
  shortcuts: {
    card: "bg-panel rounded-md",
    "dlg-backdrop": "fixed inset-0 bg-black/55 flex items-center justify-center",
    dlg: "bg-panel border border-solid border-line rounded-[10px] p-[18px] max-w-[380px]",
    "dlg-title": "mt-0 mb-2 text-[15px]",
    "dlg-check": "block mt-2.5 text-muted",
    "dlg-btns": "flex justify-end gap-2 mt-3.5",
    msg: "m-0 text-muted",
    hint: "text-xs text-muted",
    field: "my-2.5 flex items-center gap-2.5",
    "field-label": "min-w-[100px] text-muted",
    "seg-btn": "rounded-none first:rounded-l-md last:rounded-r-md",
    "seg-on": "border-accent bg-accent text-ink",
    "icon-btn": "inline-flex h-8 w-[34px] items-center justify-center p-0",
  },
});
