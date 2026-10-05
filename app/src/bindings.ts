import type { Action, KeyView, Macro, MacroEvent, Media, Mouse } from "./types";

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
export function shortLabel(a: Action | null, keys: Map<number, KeyView>, t: T, macros: Record<number, Macro> = {}): string {
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
    case "macro": {
      return `▶${macros[a.id]?.name ?? a.id}`;
    }
    case "system": {
      return t(`bindings.system.${a.action}`);
    }
  }
}

/** Bytes of the firmware body, as `macros::encode` in core writes it. */
export function bodySize(events: MacroEvent[]): number {
  let n = 0;
  for (const e of events) {
    if (e.type === "delay") n += e.ms === 0 ? 0 : e.ms <= 0xff ? 2 : e.ms <= 0xff_ff ? 3 : e.ms <= 0xff_ff_ff ? 4 : 5;
    else if (!(e.type === "mouse" && !e.down && e.button.startsWith("wheel"))) n += 2;
  }
  return n;
}

/** Flash a body takes: block tags and a header, in 8-byte units. */
export const footprint = (len: number) => 8 + Math.ceil((len + 6) / 8) * 8;

// `KeyboardEvent.code` to the keymap name, where the two differ beyond letters, digits and F-keys.
const CODES: Record<string, string> = {
  Backquote: "TILDE",
  Minus: "HYPEN",
  Equal: "EQUAL",
  IntlYen: "YEN",
  Backspace: "BACKSPACE",
  Tab: "TAB",
  BracketLeft: "OPEN_SQUARE_BRACKET",
  BracketRight: "CLOSE_SQUARE_BRACKET",
  Backslash: "BACKSLASH",
  CapsLock: "CAPS_LOCK",
  Semicolon: "SEMICOLON",
  Quote: "APOSTROPHE",
  Enter: "ENTER",
  ShiftLeft: "LEFT_SHIFT",
  IntlBackslash: "NON_US_BACKSLASH",
  Comma: "COMMA",
  Period: "PERIOD",
  Slash: "SLASH",
  IntlRo: "RO",
  ShiftRight: "RIGHT_SHIFT",
  ControlLeft: "LEFT_CTRL",
  MetaLeft: "LEFT_GUI",
  AltLeft: "LEFT_ALT",
  Space: "SPACEBAR",
  AltRight: "RIGHT_ALT",
  MetaRight: "RIGHT_GUI",
  ContextMenu: "APPLICATION",
  ControlRight: "RIGHT_CTRL",
  Insert: "INSERT",
  Delete: "DELETE",
  Home: "HOME",
  End: "END",
  PageUp: "PAGE_UP",
  PageDown: "PAGE_DOWN",
  ArrowLeft: "LEFT_ARROW",
  ArrowUp: "UP_ARROW",
  ArrowDown: "DOWN_ARROW",
  ArrowRight: "RIGHT_ARROW",
  NumLock: "NUMPAD_NUM_LOCK",
  NumpadDivide: "NUMPAD_SLASH",
  NumpadMultiply: "NUMPAD_ASTERISK",
  NumpadSubtract: "NUMPAD_DASH",
  NumpadAdd: "NUMPAD_PLUS",
  NumpadEqual: "NUMPAD_EQUAL",
  NumpadEnter: "NUMPAD_ENTER",
  NumpadDecimal: "NUMPAD_PERIOD",
  Escape: "ESC",
  PrintScreen: "PRINT_SCREEN",
  ScrollLock: "SCROLL_LOCK",
  Pause: "PAUSE",
};

/** Keymap name of a `KeyboardEvent.code`, for recording macros. */
export function codeName(code: string): string {
  return CODES[code] ?? code.replace(/^Key|^Digit/, "").replace(/^Numpad(\d)$/, "NUMPAD_$1");
}

/** Key name that tells apart the keys sharing a cap label. */
export function keyName(k: KeyView, t: T): string {
  if (k.name === "SPACEBAR") return t("bindings.space");
  if (k.name === "NUMPAD_NUM_LOCK") return "Num Lock";
  if (k.name.startsWith("NUMPAD_")) return `Num ${k.label}`;
  const side = /^(LEFT|RIGHT)_(?:SHIFT|CTRL|ALT|GUI)$/.exec(k.name);
  return side ? t(`bindings.${side[1].toLowerCase()}`, { k: k.label }) : k.label;
}
