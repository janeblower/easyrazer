import { expect, test } from "bun:test";
import { call, keyboard, ours, pick } from "./keyboard";
import { messages } from "./i18n";
import type { DeviceEntry } from "./types";

const e = (id: string, connected: boolean): DeviceEntry => ({ id, model: "Razer Huntsman V2 Analog", connected, driver_mode: false });

test("keeps a connected choice", () => {
  expect(pick([e("A", true), e("B", true)], "B")).toBe("B");
});
test("moves off an unplugged one to a connected one", () => {
  expect(pick([e("A", false), e("B", true)], "A")).toBe("B");
});
test("stays on a known one when nothing is connected", () => {
  expect(pick([e("A", false), e("B", false)], "B")).toBe("B");
});
test("takes the first known when the choice is gone", () => {
  expect(pick([e("A", false)], "Z")).toBe("A");
});
test("is null with nothing known", () => {
  expect(pick([], "A")).toBeNull();
});

test("accepts only the selected keyboard", () => {
  keyboard.value = "A";
  expect(ours("A")).toBe(true);
  expect(ours("B")).toBe(false);
  expect(ours(null)).toBe(false);
  keyboard.value = null;
  expect(ours(null)).toBe(true);
});

test("stays on an unplugged one while it has unapplied edits", () => {
  expect(pick([e("A", false), e("B", true)], "A", true)).toBe("A");
});
test("stay does not keep a keyboard that is gone", () => {
  expect(pick([e("B", true)], "A", true)).toBe("B");
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
