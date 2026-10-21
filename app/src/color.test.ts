import { expect, test } from "bun:test";
import type { Rgb } from "./types";
import { hexOf, hsvOf, rgbOfHex, rgbOfHsv } from "./color";

test("hex round-trips", () => {
  expect(hexOf([0x44, 0xd6, 0x2c])).toBe("#44d62c");
  expect(rgbOfHex("#44D62C")).toEqual([0x44, 0xd6, 0x2c]);
  expect(rgbOfHex(" 44d62c ")).toEqual([0x44, 0xd6, 0x2c]);
});

test("malformed hex is refused", () => {
  expect(rgbOfHex("#44d62")).toBe(null);
  expect(rgbOfHex("green")).toBe(null);
});

test("hsv keeps the color", () => {
  const colors: Rgb[] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [0x44, 0xd6, 0x2c],
    [255, 255, 255],
    [0, 0, 0],
    [12, 200, 180],
  ];
  for (const c of colors) expect(rgbOfHsv(hsvOf(c))).toEqual(c);
});

test("hsv of the primaries", () => {
  expect(hsvOf([255, 0, 0])).toEqual([0, 1, 1]);
  expect(hsvOf([0, 0, 255])).toEqual([240, 1, 1]);
  expect(hsvOf([0, 0, 0])).toEqual([0, 0, 0]);
});
