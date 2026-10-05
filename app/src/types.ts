// Payloads of the Rust commands and events (app/src-tauri/src/commands.rs, device.rs).

export type Rgb = [number, number, number];

export interface Status {
  device: boolean;
  synapse: boolean;
  driver_mode: boolean;
  profile: number | null; // id of the loaded profile
  profile_name: string | null;
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
export type MacroMode = "times" | "hold" | "toggle";

export type System = "brightness_down" | "brightness_up" | "sleep" | "game_mode" | "macro_led" | "next_profile" | "snap_tap";

export type Action =
  | { type: "disabled" }
  | { type: "key"; key: number; mods: number }
  | { type: "mouse"; button: Mouse }
  | { type: "media"; media: Media }
  | { type: "macro"; id: number; mode: MacroMode; count: number }
  | { type: "system"; action: System };

// A wheel turns on `down` and ignores the release.
export type MacroEvent =
  { type: "key"; key: number; down: boolean } | { type: "mouse"; button: Mouse; down: boolean } | { type: "delay"; ms: number };

export interface Macro {
  name: string;
  events: MacroEvent[];
  // The keyboard's flash holds these events.
  written: boolean;
}

export interface MacroState {
  macros: Record<number, Macro>;
  // Free bytes in the keyboard's macro store; null without a keyboard.
  free: number | null;
}

export type BindResult =
  { status: "ok" | "unconfirmed"; key: number; action: Action | null } | { status: "error"; key: number; message: string };

// The loaded profile's Hypershift layer.
export interface Hypershift {
  values: KeyMap<number>;
  // null: what the app does not edit, such as the Fn key.
  bindings: KeyMap<Action | null>;
  // Differ from the profile's slot.
  unsaved: number[];
}

export interface WriteResult {
  results: ApplyResult[];
  unsaved: number[];
  bindings: BindResult[];
  unsaved_bindings: number[];
  hypershift: Hypershift;
}

export interface Rapid {
  enabled: boolean;
  press: number;
  release: number;
}

export type SnapRule = "last" | "neutral" | "deeper";

export interface SnapGroup {
  keys: number[];
  rule: SnapRule;
}

export interface SnapTap {
  enabled: boolean;
  groups: SnapGroup[];
}

export interface Actuation {
  values: KeyMap<number>;
  unsaved: number[];
  rapid: KeyMap<Rapid>;
  snap: SnapTap;
  // null: a binding the app does not edit, such as a service code.
  bindings: KeyMap<Action | null>;
  unsaved_bindings: number[];
  hypershift: Hypershift;
  // id of the profile loaded after the read
  profile: number | null;
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

export interface ProfileView {
  id: number;
  name: string;
  slot: number | null;
  // Differs from its slot.
  unsaved: boolean;
}

export interface ProfilesView {
  profiles: ProfileView[];
  loaded: number | null;
  // Slot the keyboard starts with without the app.
  startup: number | null;
  free_slot: boolean;
}
