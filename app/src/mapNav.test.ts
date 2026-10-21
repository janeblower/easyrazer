import { expect, test } from "bun:test";
import type { KeyView } from "./types";
import { step } from "./mapNav";

const cap = (key: number, x: number, y: number, w = 1): KeyView => ({
  key,
  name: "",
  label: "",
  x,
  y,
  w,
  h: 1,
  editable: true,
  shape: "key",
});
// Tab Q W / Caps A S / a wide Shift under them.
const tab = cap(1, 0, 0, 1.5);
const q = cap(2, 1.5, 0);
const w = cap(3, 2.5, 0);
const caps = cap(4, 0, 1, 1.75);
const a = cap(5, 1.75, 1);
const s = cap(6, 2.75, 1);
const shift = cap(7, 0, 2, 3.75);
const keys = [tab, q, w, caps, a, s, shift];

test("arrows keep to the row", () => {
  expect(step(keys, tab, "right")).toBe(q);
  expect(step(keys, q, "right")).toBe(w);
  expect(step(keys, w, "left")).toBe(q);
  expect(step(keys, a, "left")).toBe(caps);
});

test("up and down take the nearest key of the next row", () => {
  expect(step(keys, q, "down")).toBe(a);
  expect(step(keys, w, "down")).toBe(s);
  expect(step(keys, s, "up")).toBe(w);
  expect(step(keys, a, "down")).toBe(shift);
  expect(step(keys, shift, "up")).toBe(a);
});

test("nothing past the edge", () => {
  expect(step(keys, tab, "left")).toBe(undefined);
  expect(step(keys, w, "up")).toBe(undefined);
  expect(step(keys, shift, "down")).toBe(undefined);
});
