import { defineConfig, presetWind4 } from "unocss";

// Colors live as CSS variables in style.css, so scoped styles and utilities share them.
const COLORS = [
  "bg",
  "panel",
  "key",
  "key-off",
  "line",
  "text",
  "muted",
  "ink",
  "accent",
  "hs",
  "driver",
  "edited",
  "error",
  "warn",
  "warn-bg",
  "write",
  "write-line",
  "dev",
];

export default defineConfig({
  // The browser defaults the layout was built on stay; style.css sets the few base styles.
  presets: [presetWind4({ preflights: { reset: false } })],
  theme: {
    colors: Object.fromEntries(COLORS.map((c) => [c, `var(--${c})`])),
  },
  shortcuts: {
    card: "bg-panel rounded-md",
    "dlg-title": "mt-0 mb-2 text-[15px]",
    "dlg-check": "block mt-2.5 text-muted",
    "dlg-btns": "flex justify-end gap-2 mt-3.5",
    msg: "m-0 text-muted",
    hint: "text-xs text-muted",
    field: "my-2.5 flex items-center gap-2.5",
    "field-label": "min-w-[100px] text-muted",
    "seg-btn": "rounded-none first:rounded-l-md last:rounded-r-md",
    "icon-btn": "inline-flex h-8 w-[34px] items-center justify-center p-0",
    "write-btn": "icon-btn text-write border-write-line bg-transparent",
    "dev-btn": "bg-transparent border-0 rounded px-1.5 py-0.5 text-muted hover:bg-key hover:text-text",
    "dev-tab": "bg-transparent border-0 border-b-2 border-solid border-transparent rounded-none h-full px-2.5 text-muted",
    "dev-tab-on": "text-text border-dev",
    "dev-pop": "absolute z-30 bg-key border border-solid border-line rounded-md py-1.5 shadow-lg text-xs",
  },
});
