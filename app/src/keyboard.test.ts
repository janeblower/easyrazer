import { expect, test } from "bun:test";
import { call, escapesSelection, keyboard, loadKey, ours, pick, statusAction } from "./keyboard";
import { messages } from "./i18n";
import type { DeviceEntry } from "./types";

const kb = (id: string, connected: boolean): DeviceEntry => ({ id, name: id, model: "M", connected, driver_mode: false });

test("the current keyboard stays even when it disconnects", () => {
  expect(pick([kb("A", false), kb("B", true)], "A", null)).toBe("A");
});

test("on start the last keyboard wins, then a connected one, then the first", () => {
  expect(pick([kb("A", true), kb("B", false)], null, "B")).toBe("B");
  expect(pick([kb("A", false), kb("B", true)], null, "gone")).toBe("B");
  expect(pick([kb("A", false), kb("B", false)], null, null)).toBe("A");
  expect(pick([], null, "B")).toBe(null);
});

test("a forgotten current keyboard gives way", () => {
  expect(pick([kb("B", true)], "A", "A")).toBe("B");
});

test("accepts only the selected keyboard", () => {
  keyboard.value = "A";
  expect(ours("A")).toBe(true);
  expect(ours("B")).toBe(false);
  expect(ours(null)).toBe(false);
  keyboard.value = null;
  expect(ours(null)).toBe(true);
});

test("a call without a keyboard fails with a plain message", async () => {
  keyboard.value = null;
  const rejected = await call("read_all").catch((error_: unknown) => error_);
  expect(rejected).toBe(messages.en.backend.noKeyboard);
});

test("an explicit id wins over the selected keyboard", async () => {
  const sent: unknown[] = [];
  (globalThis as { window?: unknown }).window = {
    __TAURI_INTERNALS__: {
      // eslint-disable-next-line @typescript-eslint/require-await -- stands in for the async IPC
      invoke: async (_cmd: string, args: unknown) => {
        sent.push(args);
      },
    },
  };
  keyboard.value = "B";
  await call("write_macro", { macroId: 1 }, "A");
  await call("write_macro", { macroId: 2 });
  expect(sent).toEqual([
    { macroId: 1, id: "A" },
    { macroId: 2, id: "B" },
  ]);
});

test("a status reads again on a reconnect or a disconnect, and reverts only for another profile", () => {
  const key = loadKey({ device: true, profile: 3 });
  expect(statusAction({ device: true, profile: 3 }, 3, key)).toEqual({ revert: false, load: false });
  expect(statusAction({ device: false, profile: 3 }, 3, key)).toEqual({ revert: false, load: true });
  expect(statusAction({ device: true, profile: 3 }, 3, loadKey({ device: false, profile: 3 }))).toEqual({ revert: false, load: true });
  expect(statusAction({ device: true, profile: 4 }, 3, key)).toEqual({ revert: true, load: true });
  expect(statusAction({ device: true, profile: null }, 3, key)).toEqual({ revert: false, load: true });
});

test("Esc clears selections everywhere but in text fields and handled events", () => {
  const esc = (target: object | null, defaultPrevented = false) =>
    escapesSelection({ key: "Escape", defaultPrevented, target: target as EventTarget | null });
  expect(esc(null)).toBe(true);
  expect(esc({ tagName: "INPUT", type: "range" })).toBe(true);
  expect(esc({ tagName: "INPUT", type: "text" })).toBe(false);
  expect(esc({ tagName: "TEXTAREA" })).toBe(false);
  expect(esc({ tagName: "DIV", isContentEditable: true })).toBe(false);
  expect(esc(null, true)).toBe(false);
  expect(escapesSelection({ key: "Enter", defaultPrevented: false, target: null })).toBe(false);
});
