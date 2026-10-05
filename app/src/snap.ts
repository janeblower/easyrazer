import type { KeyMap, SnapRule, SnapTap } from "./types";

export const SNAP_RULES: SnapRule[] = ["last", "neutral", "deeper"];

// Group colors, cycled; none is the accent, edited, error or driver color of the map.
const COLORS = ["#3cc8e0", "#b48cff", "#ff7ab6", "#5b8cff"];

export const snapColor = (i: number) => COLORS[i % COLORS.length];

export function snapColors(s: SnapTap): KeyMap<string> {
  return Object.fromEntries(s.groups.flatMap((g, i) => g.keys.map((k) => [k, snapColor(i)])));
}

/** The new group takes its keys from the old ones; a group left with one key goes. */
export function addGroup(s: SnapTap, keys: number[]): SnapTap {
  const rest = s.groups.map((g) => ({ ...g, keys: g.keys.filter((k) => !keys.includes(k)) })).filter((g) => g.keys.length > 1);
  return { ...s, groups: [...rest, { keys, rule: "last" }] };
}

export const removeGroup = (s: SnapTap, i: number): SnapTap => ({ ...s, groups: s.groups.filter((_, j) => j !== i) });

export const setRule = (s: SnapTap, i: number, rule: SnapRule): SnapTap => ({
  ...s,
  groups: s.groups.map((g, j) => (j === i ? { ...g, rule } : g)),
});

export const sameSnap = (a: SnapTap, b: SnapTap) => JSON.stringify(a) === JSON.stringify(b);
