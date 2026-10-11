import { expect, test } from "bun:test";
import { type Registry, hex, isWrite, parse, suggest } from "./devConsole";

const reg: Registry = {
  get_mode: { cmd: "00:84", size: 2, help: "Device mode", args: [], example: "" },
  set_mode: { cmd: "00:04", size: 2, help: "Switches the mode", args: [["mode", ""]], example: "03 00" },
  macro_write: { cmd: "06:09", size: null, help: "Writes a chunk of a macro body", args: [], example: "" },
};

test("a named command takes its registry size", () => {
  expect(parse("set_mode 03 00", reg)).toEqual({ cmd: "00:04", size: 2, args: [3, 0] });
  expect(parse("SET_MODE 3 0", reg)).toEqual({ cmd: "00:04", size: 2, args: [3, 0] });
});

test("a raw code takes the args length", () => {
  expect(parse("0a:8e 1 ff", reg)).toEqual({ cmd: "0A:8E", size: 2, args: [1, 255] });
  expect(parse("macro_write 00 01", reg)).toEqual({ cmd: "06:09", size: 2, args: [0, 1] });
});

test("size= is hex and wins", () => {
  expect(parse("get_mode size=10", reg)).toEqual({ cmd: "00:84", size: 16, args: [] });
});

test("bad input is an error, not a command", () => {
  expect(parse("   ", reg)).toBe("empty");
  expect(parse("nope", reg)).toBe("unknown command nope");
  expect(parse("get_mode zz", reg)).toBe("bad byte zz");
  expect(parse("get_mode 100", reg)).toBe("bad byte 100");
  expect(parse("get_mode size=zz", reg)).toBe("bad size zz");
  expect(parse(`get_mode ${"00 ".repeat(81)}`, reg)).toBe("more than 80 bytes");
});

test("a write is an id without bit 7", () => {
  expect(isWrite("00:04")).toBe(true);
  expect(isWrite("00:84")).toBe(false);
});

test("suggestions match the name, the code or the help", () => {
  expect(suggest("mode", reg, []).map((s) => s.text)).toEqual(["get_mode", "set_mode"]);
  expect(suggest("06:", reg, []).map((s) => s.text)).toEqual(["macro_write"]);
  expect(suggest("chunk 00", reg, []).map((s) => s.text)).toEqual(["macro_write"]);
});

test("recent lines lead only while nothing is typed", () => {
  expect(suggest("", reg, ["get_mode"])[0]).toEqual({ text: "get_mode", recent: true });
  expect(suggest("", reg, []).length).toBe(3);
  expect(suggest("get", reg, ["get_mode"]).some((s) => s.recent)).toBe(false);
});

test("hex pads and uppercases", () => {
  expect(hex([0, 10, 255])).toBe("00 0A FF");
});
