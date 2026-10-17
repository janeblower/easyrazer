import { expect, test } from "bun:test";
import { ANALOG_BINDINGS, GAMEPAD_BINDINGS, capturedKey, defaultGamepad, gamepadState, sameGamepad, stickDisplay } from "./gamepad";

test("default layout has independent WASD and QE assignments", () => {
  const config = defaultGamepad();
  expect(config).toEqual({ enabled: false, bindings: { left_up: 18, left_down: 32, left_left: 31, left_right: 33, lt: 17, rt: 19 } });
  config.bindings.left_up = 37;
  expect(defaultGamepad().bindings.left_up).toBe(18);
});

test("configuration equality ignores assignment order", () => {
  expect(sameGamepad({ enabled: true, bindings: { a: 1, b: 2 } }, { enabled: true, bindings: { b: 2, a: 1 } })).toBe(true);
  expect(sameGamepad({ enabled: false, bindings: {} }, { enabled: true, bindings: {} })).toBe(false);
  expect(sameGamepad({ enabled: true, bindings: { a: 1 } }, { enabled: true, bindings: { a: 2 } })).toBe(false);
});
test("unassigned controls stay neutral", () => {
  const state = gamepadState({}, [255, 255]);
  expect(Object.values(state.buttons).some(Boolean)).toBe(false);
  expect(state.left).toEqual({ x: 0, y: 0 });
  expect(state.right).toEqual({ x: 0, y: 0 });
  expect(state.lt).toBe(0);
  expect(state.rt).toBe(0);
});

test("opposite axes cancel and preserve full linear depth", () => {
  const state = gamepadState({ left_left: 0, left_right: 1, left_up: 2, right_down: 3, lt: 4, rt: 5 }, [255, 255, 255, 128, 255, 128]);
  expect(state.left).toEqual({ x: 0, y: -1 });
  expect(state.right).toEqual({ x: 0, y: 128 / 255 });
  expect(state.lt).toBe(1);
  expect(state.rt).toBe(128 / 255);
});

test("every digital control uses the same inclusive threshold", () => {
  for (const binding of GAMEPAD_BINDINGS) {
    if (ANALOG_BINDINGS.includes(binding)) continue;
    expect(gamepadState({ [binding]: 0 }, [127]).buttons[binding]).toBe(false);
    expect(gamepadState({ [binding]: 0 }, [128]).buttons[binding]).toBe(true);
  }
});

test("analog controls reach full output only at their configured maximum", () => {
  const bindings = { left_right: 0, lt: 1, rt: 2 };
  const half = gamepadState(bindings, [128, 128, 128]);
  expect(half.left.x).toBe(128 / 255);
  expect(half.lt).toBe(128 / 255);
  expect(half.buttons.lt).toBe(false);
  const scaled = gamepadState(bindings, [128, 128, 128], { left_right: 50, lt: 50 });
  expect(scaled.left.x).toBe(1);
  expect(scaled.lt).toBe(1);
  expect(scaled.rt).toBe(128 / 255);
  expect(scaled.buttons.lt).toBe(true);
  expect(gamepadState(bindings, [255, 255, 255]).lt).toBe(1);
  expect(gamepadState(bindings, [255, 255, 255], { lt: 50 }).lt).toBe(1);
  expect(sameGamepad({ enabled: true, bindings }, { enabled: true, bindings, max_depth: { lt: 100 } })).toBe(true);
  expect(sameGamepad({ enabled: true, bindings }, { enabled: true, bindings, max_depth: { lt: 50 } })).toBe(false);
});

test("stick presentation clamps diagonals and hides neutral overlays", () => {
  expect(stickDisplay({ x: 0, y: 0 }).opacity).toBe(0);
  const diagonal = stickDisplay({ x: 1, y: -1 });
  expect(diagonal.magnitude).toBe(1);
  expect(diagonal.angle).toBe(45);
  expect(Math.abs(Math.hypot(diagonal.x, diagonal.y) - 1) < 0.000_001).toBe(true);
});

test("capture ignores held and noneditable keys until a fresh press", () => {
  expect(capturedKey([255, 0, 0], [255, 255, 255], [0, 2])).toBe(2);
  expect(capturedKey([255], [255], [0])).toBeNull();
  expect(capturedKey([127], [128], [0])).toBe(0);
  expect(capturedKey([0], [127], [0])).toBeNull();
});
