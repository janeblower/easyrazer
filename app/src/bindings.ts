import type { Action, KeyView, Media, Mouse } from "./types";

type T = (key: string, args?: Record<string, unknown>) => string;

export const MOUSE: Mouse[] = ["left", "right", "middle", "back", "forward", "wheel_up", "wheel_down"];
export const MEDIA: Media[] = ["play", "prev", "next", "stop", "mute", "volume_up", "volume_down"];
// Bits 0..3 of the HID modifier byte; 4..7 are the same modifiers on the right.
export const MODS = ["ctrl", "shift", "alt", "win"];
const GLYPHS = ["⌃", "⇧", "⌥", "⊞"];

export function common<V>(values: V[]): V | null {
  return values.length > 0 && values.every((v) => v === values[0]) ? values[0] : null;
}

export const factory = (key: number): Action => ({ type: "key", key, mods: 0 });

export function sameAction(a: Action | null | undefined, b: Action | null | undefined): boolean {
  if (!a || !b) return (a ?? null) === (b ?? null);
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  return Object.keys({ ...a, ...b }).every((k) => x[k] === y[k]);
}

/** Text on the key cap; `null` is a binding the app does not edit. */
export function shortLabel(a: Action | null, keys: Map<number, KeyView>, t: T): string {
  if (!a) return "…";
  switch (a.type) {
    case "disabled": {
      return "⊘";
    }
    case "key": {
      const mods = GLYPHS.filter((_, i) => a.mods & (0x11 << i)).join("");
      return mods + (keys.get(a.key)?.label || "␣");
    }
    case "mouse": {
      return t(`bindings.mouseShort.${a.button}`);
    }
    case "media": {
      return t(`bindings.mediaShort.${a.media}`);
    }
  }
}

/** Key name that tells apart the keys sharing a cap label. */
export function keyName(k: KeyView, t: T): string {
  if (k.name === "SPACEBAR") return t("bindings.space");
  if (k.name === "NUMPAD_NUM_LOCK") return "Num Lock";
  if (k.name.startsWith("NUMPAD_")) return `Num ${k.label}`;
  const side = /^(LEFT|RIGHT)_(?:SHIFT|CTRL|ALT|GUI)$/.exec(k.name);
  return side ? t(`bindings.${side[1].toLowerCase()}`, { k: k.label }) : k.label;
}
