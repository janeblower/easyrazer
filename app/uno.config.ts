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
    "seg-on": "border-accent bg-accent text-ink",
    "icon-btn": "inline-flex h-8 w-[34px] items-center justify-center p-0",
    "write-btn": "icon-btn text-write border-write-line bg-transparent",
  },
});
