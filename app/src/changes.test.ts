import { expect, test } from "bun:test";
import { type Section, applyAll, revertAll, summarize, writeAll } from "./changes";

function section(id: Section["id"], over: Partial<Section> = {}): Section & { log: string[] } {
  const log: string[] = [];
  // eslint-disable-next-line @typescript-eslint/require-await -- stands in for the async IPC
  const note = async (what: string) => {
    log.push(`${id}:${what}`);
  };
  return {
    id,
    log,
    pending: () => 0,
    parts: () => [],
    canApply: () => false,
    canRevert: () => false,
    canWrite: () => false,
    apply: async () => note("apply"),
    revert: () => {
      log.push(`${id}:revert`);
    },
    write: async () => note("write"),
    ...over,
  };
}

test("sums the edits of every section and lists the parts that have them", () => {
  const keys = section("keys", { pending: () => 3, parts: () => [{ label: "Назначения", place: "bindings" }] });
  const lighting = section("lighting", { pending: () => 1, parts: () => [{ label: "Подсветка", place: "lighting" }] });
  const macros = section("macros", { parts: () => [{ label: "Макросы", place: "macros" }] });
  const s = summarize([keys, lighting, macros]);
  expect(s.pending).toBe(4);
  expect(s.parts.map((p) => p.label)).toEqual(["Назначения", "Подсветка"]);
});

test("a button is on when any section can take it", () => {
  const s = summarize([section("keys"), section("lighting", { canApply: () => true, canWrite: () => true })]);
  expect([s.canApply, s.canRevert, s.canWrite]).toEqual([true, false, true]);
});

test("notes edits that Write passes by", () => {
  const custom = section("lighting", { pending: () => 1, skipsWrite: () => true });
  expect(summarize([custom]).skipsWrite).toBe(true);
  expect(summarize([section("lighting", { skipsWrite: () => true })]).skipsWrite).toBe(false);
});

test("each action runs on the sections that can take it, in order", async () => {
  const keys = section("keys", { canApply: () => true, canRevert: () => true });
  const lighting = section("lighting", { canApply: () => true, canWrite: () => true });
  const macros = section("macros", { canRevert: () => true, canWrite: () => true });
  const all = [keys, lighting, macros];
  await applyAll(all);
  revertAll(all);
  await writeAll(all);
  expect([...keys.log, ...lighting.log, ...macros.log]).toEqual([
    "keys:apply",
    "keys:revert",
    "lighting:apply",
    "lighting:write",
    "macros:revert",
    "macros:write",
  ]);
});

test("a section that can no longer act after an earlier one is skipped", async () => {
  let busy = false;
  const keys = section("keys", {
    canApply: () => true,
    // eslint-disable-next-line @typescript-eslint/require-await -- stands in for the async IPC
    apply: async () => {
      busy = true;
    },
  });
  const lighting = section("lighting", { canApply: () => !busy });
  await applyAll([keys, lighting]);
  expect(lighting.log).toEqual([]);
});
