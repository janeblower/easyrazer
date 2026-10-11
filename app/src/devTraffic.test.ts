import { expect, test } from "bun:test";
import type { CommandInfo, LogEntry } from "./types";
import { argRows, dump, isError, kindOf, matches, parsePrefs, selectedEntry, visibleColumns } from "./devTraffic";

const entry = (over: Partial<LogEntry>): LogEntry => ({
  seq: 1,
  time: 0,
  cmd: "02:92",
  size: 3,
  args: [1, 0x1e, 0],
  status: "Ok",
  reply: [1, 0x1e, 0, 0x3c],
  polls: 0,
  ms: 4,
  error: null,
  background: false,
  tx: [],
  rx: [],
  ...over,
});

test("an error is a transport error or a status other than Ok", () => {
  expect(isError(entry({}))).toBe(false);
  expect(isError(entry({ status: "Failure" }))).toBe(true);
  expect(isError(entry({ status: null, error: "timeout" }))).toBe(true);
  expect(kindOf(entry({}))).toBe("r");
  expect(kindOf(entry({ cmd: "02:12" }))).toBe("w");
  expect(kindOf(entry({ cmd: "02:12", status: "Failure" }))).toBe("e");
});

test("a failed write shows under writes and under errors", () => {
  const e = entry({ cmd: "02:12", status: "Failure" });
  expect(matches(e, "w", false, "")).toBe(true);
  expect(matches(e, "e", false, "")).toBe(true);
  expect(matches(e, "r", false, "")).toBe(false);
});

test("polling hides unless asked for, and the query matches name, code or bytes", () => {
  const poll = entry({ cmd: "00:84", background: true });
  expect(matches(poll, "all", false, "", "get_mode")).toBe(false);
  expect(matches(poll, "all", true, "", "get_mode")).toBe(true);
  expect(matches(entry({}), "all", false, "key_assign", "get_key_assignment")).toBe(true);
  expect(matches(entry({}), "all", false, "1e 00")).toBe(true);
  expect(matches(entry({}), "all", false, "0f:")).toBe(false);
});

test("columns drop from the least needed, the name stays", () => {
  const all = ["time", "name", "code", "args", "status", "reply", "ms"] as const;
  expect(visibleColumns([...all], 2000)).toEqual([...all]);
  expect(visibleColumns([...all], 560)).toEqual(["time", "name", "code", "status", "ms"]);
  expect(visibleColumns([...all], 400)).toEqual(["time", "name", "status", "ms"]);
  expect(visibleColumns([...all], 10)).toEqual(["name"]);
  expect(visibleColumns(["code"], 2000)).toEqual(["name", "code"]);
});

test("arg bytes take the registry names, extra bytes the last one", () => {
  const info: CommandInfo = {
    cmd: "06:03",
    size: null,
    help: "",
    args: [
      ["id", ""],
      ["data", ""],
    ],
    example: "",
  };
  expect(argRows(info, [1, 2, 3])).toEqual([
    ["id", "01"],
    ["data", "02"],
    ["data", "03"],
  ]);
  expect(argRows(undefined, [0xff])).toEqual([["args[0]", "FF"]]);
});

test("a dump has 16 bytes a row under hex offsets", () => {
  const rows = dump(Array.from({ length: 90 }, (_, i) => i));
  expect(rows.length).toBe(6);
  expect(rows[1]?.offset).toBe("10");
  expect(rows[1]?.cells[0]).toBe("10");
  expect(rows[5]?.cells.length).toBe(10);
});

test("the selected exchange is gone once the ring or a clear drops it", () => {
  const log = [entry({ seq: 5 }), entry({ seq: 6 })];
  expect(selectedEntry(log, 6, -1)?.seq).toBe(6);
  expect(selectedEntry(log, 4, -1)).toBeNull();
  expect(selectedEntry(log, 5, 5)).toBeNull();
  expect(selectedEntry(log, null, -1)).toBeNull();
});

test("stored view settings fall back to defaults", () => {
  expect(parsePrefs(null)).toEqual({ columns: ["time", "name", "code", "args", "status", "ms"], show: "all", background: false });
  expect(parsePrefs("{oops")).toEqual(parsePrefs(null));
  expect(parsePrefs('{"columns":["ms","bogus"],"show":"x","background":true}')).toEqual({ columns: ["ms"], show: "all", background: true });
});
