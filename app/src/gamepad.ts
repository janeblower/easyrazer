export const GAMEPAD_BINDINGS = [
  "left_up",
  "left_down",
  "left_left",
  "left_right",
  "left_click",
  "right_up",
  "right_down",
  "right_left",
  "right_right",
  "right_click",
  "a",
  "b",
  "x",
  "y",
  "lb",
  "rb",
  "lt",
  "rt",
  "back",
  "start",
  "dpad_up",
  "dpad_down",
  "dpad_left",
  "dpad_right",
] as const;

export type GamepadBinding = (typeof GAMEPAD_BINDINGS)[number];
export interface GamepadConfig {
  enabled: boolean;
  bindings: Partial<Record<GamepadBinding, number>>;
}
export interface GamepadInput {
  depth: number[];
  running: boolean;
  error: string | null;
}
export function defaultGamepad(): GamepadConfig {
  return { enabled: false, bindings: { left_up: 18, left_down: 32, left_left: 31, left_right: 33, lt: 17, rt: 19 } };
}
export interface StickState {
  x: number;
  y: number;
}

export function sameGamepad(a: GamepadConfig, b: GamepadConfig): boolean {
  return a.enabled === b.enabled && GAMEPAD_BINDINGS.every((id) => a.bindings[id] === b.bindings[id]);
}

export function gamepadState(bindings: GamepadConfig["bindings"], depth: number[]) {
  const value = (id: GamepadBinding) => {
    const key = bindings[id];
    const raw = key === undefined ? 0 : (depth[key] ?? 0);
    return Number.isFinite(raw) ? Math.max(0, Math.min(255, raw)) / 255 : 0;
  };
  return {
    buttons: Object.fromEntries(GAMEPAD_BINDINGS.map((id) => [id, value(id) >= 128 / 255])) as Record<GamepadBinding, boolean>,
    left: { x: value("left_right") - value("left_left"), y: value("left_down") - value("left_up") },
    right: { x: value("right_right") - value("right_left"), y: value("right_down") - value("right_up") },
    lt: value("lt"),
    rt: value("rt"),
  };
}

export function stickDisplay(stick: StickState) {
  const length = Math.hypot(stick.x, stick.y);
  const magnitude = Math.min(1, length);
  return {
    x: stick.x / Math.max(1, length),
    y: stick.y / Math.max(1, length),
    magnitude,
    angle: length ? (Math.round((Math.atan2(-stick.y, stick.x) * 180) / Math.PI) + 360) % 360 : 0,
    opacity: magnitude <= 0.01 ? 0 : Math.min(1, (magnitude - 0.01) / 0.15),
  };
}

export function capturedKey(previous: number[], depth: number[], editableKeys: number[]): number | null {
  return editableKeys.find((key) => (previous[key] ?? 0) < 128 && (depth[key] ?? 0) >= 128) ?? null;
}
