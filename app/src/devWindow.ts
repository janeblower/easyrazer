import { LogicalPosition, LogicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invoke } from "@tauri-apps/api/core";
import { setPanel } from "./dev";
import type { DevWindow, Dock } from "./types";

// tauri.conf.json minWidth/minHeight
const MIN_W = 1153;
const MIN_H = 923;
const LABEL = "devtools";
/** The raw-data block on the keys tab and the gap above it (`DevKeys.vue`, `gap-4`). */
export const DEV_BLOCK_H = 100;
const DEV_EXTRA_H = DEV_BLOCK_H + 16;

// What the panel added to the main window, and how far a left dock moved it.
const grown = { w: 0, h: 0, left: 0 };

async function logical<T>(physical: Promise<{ toLogical: (scale: number) => T }>, scale: number) {
  const value = await physical;
  return value.toLogical(scale);
}

async function grow(dock: Dock | null, size: number, extra: number) {
  const win = getCurrentWindow();
  // A maximized window keeps its size; the panel takes room from the content.
  if (await win.isMaximized()) {
    if (dock === null && extra === 0) {
      await win.setMinSize(new LogicalSize(MIN_W, MIN_H));
      Object.assign(grown, { w: 0, h: 0, left: 0 });
    }
    return;
  }
  const scale = await win.scaleFactor();
  const inner = await logical(win.innerSize(), scale);
  const outer = await logical(win.outerSize(), scale);
  const pos = await logical(win.outerPosition(), scale);
  const mon = await currentMonitor();
  const area = mon && {
    x: mon.workArea.position.x / mon.scaleFactor,
    y: mon.workArea.position.y / mon.scaleFactor,
    w: mon.workArea.size.width / mon.scaleFactor,
    h: mon.workArea.size.height / mon.scaleFactor,
  };
  const frame = { w: outer.width - inner.width, h: outer.height - inner.height };
  const base = { w: inner.width - grown.w, h: inner.height - grown.h, x: pos.x + grown.left };
  let addW = dock === "left" || dock === "right" ? size : 0;
  let addH = (dock === "bottom" ? size : 0) + extra;
  let x = base.x - (dock === "left" ? addW : 0);
  let y = pos.y;
  if (area) {
    // What does not fit the screen the panel takes from the content, which scrolls.
    addW = Math.max(0, Math.min(addW, area.w - frame.w - base.w));
    addH = Math.max(0, Math.min(addH, area.h - frame.h - base.h));
    x = base.x - (dock === "left" ? addW : 0);
    x = Math.max(area.x, Math.min(x, area.x + area.w - (base.w + addW + frame.w)));
    y = Math.max(area.y, Math.min(y, area.y + area.h - (base.h + addH + frame.h)));
  }
  await win.setMinSize(new LogicalSize(MIN_W + addW, MIN_H + addH));
  await win.setSize(new LogicalSize(base.w + addW, base.h + addH));
  if (x !== pos.x || y !== pos.y) await win.setPosition(new LogicalPosition(x, y));
  Object.assign(grown, { w: addW, h: addH, left: base.x - x });
}

async function openDetached(geom: DevWindow | null, title: string) {
  if (await WebviewWindow.getByLabel(LABEL)) return;
  // eslint-disable-next-line no-new -- the window lives on its own once created
  new WebviewWindow(LABEL, {
    url: "index.html#devtools",
    title,
    width: geom?.w ?? 900,
    height: geom?.h ?? 600,
    minWidth: 480,
    minHeight: 300,
    ...(geom && { x: geom.x, y: geom.y }),
  });
}

async function closeDetached() {
  const win = await WebviewWindow.getByLabel(LABEL);
  await win?.destroy();
}

let running: Promise<void> | null = null;
let latest: (() => Promise<void>) | null = null;

/** Fits the main window and the separate one to the panel; one at a time, and only the newest waiting call runs. */
export async function applyLayout(mode: boolean, on: boolean, dock: Dock, size: number, geom: DevWindow | null, title: string) {
  latest = async () => {
    try {
      // The dev blocks on the tabs need room too, or the content scrolls.
      await grow(on && dock !== "window" ? dock : null, size, mode ? DEV_EXTRA_H : 0);
      await (on && dock === "window" ? openDetached(geom, title) : closeDetached());
    } catch (error) {
      // The Vite dev server has no windows to resize.
      console.error(error);
    }
  };
  if (running) return running;
  running = (async () => {
    while (latest) {
      const next = latest;
      latest = null;
      await next();
    }
    running = null;
  })();
  await running;
}

/** In the separate window: remembers where it is, and closing it closes the panel. */
export async function trackDetached() {
  const win = getCurrentWindow();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const persist = async () => {
    const scale = await win.scaleFactor();
    const pos = await logical(win.outerPosition(), scale);
    const size = await logical(win.innerSize(), scale);
    await invoke("set_dev_window", { geom: { x: pos.x, y: pos.y, w: size.width, h: size.height } });
  };
  const save = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      void persist();
    }, 500);
  };
  await win.onMoved(save);
  await win.onResized(save);
  await win.onCloseRequested(async () => {
    await setPanel({ open: false });
  });
}
