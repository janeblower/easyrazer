// Payloads of the Rust commands and events (app/src-tauri/src/commands.rs, device.rs).

export type Rgb = [number, number, number]

export interface Status {
  device: boolean
  synapse: boolean
  mode: number | null
  profile: number | null
  model: string | null
  unsupported: number | null
}

export interface KeyView {
  key: number
  label: string
  x: number
  y: number
  w: number
  h: number
  editable: boolean
}

export type ApplyResult =
  | { status: 'ok' | 'unconfirmed'; key: number; mm: number }
  | { status: 'error'; key: number; message: string }

export interface Effect {
  name: string
  rgb1?: Rgb
  rgb2?: Rgb
  dir?: string
  speed?: number
}

export interface Look {
  effect: Effect
  brightness: number
}

export interface EffectInfo {
  name: string
  colors: number
  dirs: string[]
  speed: [number, number] | null
  fast_low: boolean
}

export interface LightingState {
  effects: EffectInfo[]
  applied: Look | null
  saved: Look | null
  dynamic_lighting: boolean
  confirm_write: boolean
}

export type CloseAction = 'ask' | 'tray' | 'exit'

export interface AppSettings {
  autostart: boolean
  close_action: CloseAction
  watch_synapse: boolean
  confirm_write: boolean
  autostart_offered: boolean
}

// fwID -> value
export type KeyMap<T> = Record<number, T>
