import type { KeyView } from "./types";

export type Direction = "left" | "right" | "up" | "down";

export const ARROWS: Record<string, Direction> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

const center = (k: KeyView): [number, number] => [k.x + k.w / 2, k.y + k.h / 2];

/** The key an arrow moves to: the caps are not a grid, so the nearest one that way wins, keeping to the row or column. */
export function step(keys: KeyView[], from: KeyView, dir: Direction): KeyView | undefined {
  const [fx, fy] = center(from);
  const horizontal = dir === "left" || dir === "right";
  const sign = dir === "right" || dir === "down" ? 1 : -1;
  let best: KeyView | undefined;
  let bestScore = Infinity;
  for (const k of keys) {
    if (k === from) continue;
    const [x, y] = center(k);
    const along = (horizontal ? x - fx : y - fy) * sign;
    if (along <= 0.01) continue;
    const across = Math.abs(horizontal ? y - fy : x - fx);
    // Straying from the row costs more than going further along it.
    const score = along + across * 2;
    if (score < bestScore) {
      best = k;
      bestScore = score;
    }
  }
  return best;
}
