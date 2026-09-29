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
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  editable: boolean;
  shape: "key" | "round" | "ring";
}

export type ApplyResult = { status: "ok" | "unconfirmed"; key: number; mm: number } | { status: "error"; key: number; message: string };

export interface WriteResult {
  results: ApplyResult[];
  unsaved: number[];
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
