import type { Rgb } from "./types";

/** Hue in degrees, saturation and value from 0 to 1. */
export type Hsv = [number, number, number];

export const hexOf = (rgb: Rgb) => `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;

export function rgbOfHex(text: string): Rgb | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(text.trim());
  if (!m) return null;
  return [0, 2, 4].map((i) => Number.parseInt(m[1].slice(i, i + 2), 16)) as Rgb;
}

export function hsvOf([r, g, b]: Rgb): Hsv {
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d + 6) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
  }
  return [h * 60, max === 0 ? 0 : d / max, max / 255];
}

export function rgbOfHsv([h, s, v]: Hsv): Rgb {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return Math.round(255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1))));
  };
  return [f(5), f(3), f(1)];
}
