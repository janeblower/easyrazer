// Payloads of the Rust commands and events (app/src-tauri/src/commands.rs, device.rs).

export type Rgb = [number, number, number];

export interface Status {
  device: boolean;
  synapse: boolean;
  mode: number | null;
  driver_mode: boolean;
  profile: number | null;
  model: string | null;
  unsupported: number | null;
  error: string | null;
}

export interface KeyView {
  key: number;
  name: string;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  editable: boolean;
  shape: "key" | "round" | "ring";
}

export type ApplyResult = { status: "ok" | "unconfirmed"; key: number; mm: number } | { status: "error"; key: number; message: string };

export type Mouse = "left" | "right" | "middle" | "back" | "forward" | "wheel_up" | "wheel_down";
export type Media = "play" | "prev" | "next" | "stop" | "mute" | "volume_up" | "volume_down";

// `key` is a fwID; `mods` is the HID modifier byte, bit 0 left Ctrl to bit 3 left Win.
export type Action =
  { type: "disabled" } | { type: "key"; key: number; mods: number } | { type: "mouse"; button: Mouse } | { type: "media"; media: Media };

export type BindResult =
  { status: "ok" | "unconfirmed"; key: number; action: Action | null } | { status: "error"; key: number; message: string };

export interface WriteResult {
  results: ApplyResult[];
  unsaved: number[];
  bindings: BindResult[];
  unsaved_bindings: number[];
}

export interface Rapid {
  enabled: boolean;
  press: number;
  release: number;
}

export interface Actuation {
  values: KeyMap<number>;
  unsaved: number[];
  rapid: KeyMap<Rapid>;
  // null: a binding the app does not edit, such as a macro.
  bindings: KeyMap<Action | null>;
  unsaved_bindings: number[];
}

export interface Effect {
  name: string;
  rgb1?: Rgb;
  rgb2?: Rgb;
  dir?: string;
  speed?: number;
  colors?: KeyMap<Rgb>;
}

export interface Look {
  effect: Effect;
  brightness: number;
}

export interface EffectInfo {
  name: string;
  colors: number;
  dirs: string[];
  speed: [number, number] | null;
  fast_low: boolean;
}

export interface LightingState {
  effects: EffectInfo[];
  applied: Look | null;
  saved: Look | null;
  dynamic_lighting: boolean;
  confirm_write: boolean;
  custom: KeyMap<Rgb> | null;
}

export type CloseAction = "ask" | "tray" | "exit";

export interface AppSettings {
  autostart: boolean;
  close_action: CloseAction;
  watch_synapse: boolean;
  confirm_write: boolean;
  autostart_offered: boolean;
  language: string | null;
}

// fwID -> value
export type KeyMap<T> = Record<number, T>;
