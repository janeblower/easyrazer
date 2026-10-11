import { hex, isWrite } from "./devConsole";
import type { CommandInfo, LogEntry } from "./types";

export type Kind = "r" | "w" | "e";
export type Show = "all" | Kind;
export type Column = "time" | "name" | "code" | "args" | "status" | "reply" | "ms";

export const SHOWS: Show[] = ["all", "r", "w", "e"];
export const COLUMNS: Column[] = ["time", "name", "code", "args", "status", "reply", "ms"];
export const DEFAULT_COLUMNS: Column[] = ["time", "name", "code", "args", "status", "ms"];
const WIDTH: Record<Column, number> = { time: 96, name: 150, code: 52, args: 150, status: 70, reply: 150, ms: 52 };
// The last here is the first to go when the table is narrow.
const PRIORITY: Column[] = ["name", "status", "time", "ms", "code", "args", "reply"];
/** Offsets of the size, class and id bytes in a 90-byte report. */
export const HEADER = [5, 6, 7];

export const isError = (e: LogEntry) => e.error !== null || (e.status !== null && e.status !== "Ok");
export const kindOf = (e: LogEntry): Kind => (isError(e) ? "e" : isWrite(e.cmd) ? "w" : "r");

/** A failed write still counts as a write: the type chips and "errors" overlap. */
export function matches(e: LogEntry, show: Show, background: boolean, query: string, name?: string) {
  if (e.background && !background) return false;
  if (show === "e" && !isError(e)) return false;
  if ((show === "r" || show === "w") && isWrite(e.cmd) !== (show === "w")) return false;
  const q = query.trim().toLowerCase();
  return !q || `${e.cmd} ${name ?? ""} ${hex(e.args)} ${hex(e.reply)}`.toLowerCase().includes(q);
}

export function visibleColumns(enabled: Column[], width: number): Column[] {
  const keep = PRIORITY.filter((c) => c === "name" || enabled.includes(c));
  while (keep.length > 1 && keep.reduce((sum, c) => sum + WIDTH[c], 0) > width) keep.pop();
  return COLUMNS.filter((c) => keep.includes(c));
}

/** The last names cover variable data, so bytes past the names go under the last one. */
export function argRows(info: CommandInfo | undefined, args: number[]): [string, string][] {
  const names = info?.args.map(([n]) => n) ?? [];
  return args.map((b, i) => [names[Math.min(i, names.length - 1)] ?? `args[${i}]`, hex([b])]);
}

export function dump(bytes: number[]): { offset: string; cells: string[] }[] {
  const rows = [];
  for (let o = 0; o < bytes.length; o += 16) {
    rows.push({ offset: o.toString(16).toUpperCase().padStart(2, "0"), cells: hex(bytes.slice(o, o + 16)).split(" ") });
  }
  return rows;
}

export const clock = (ms: number) =>
  `${new Date(ms).toLocaleTimeString(undefined, { hour12: false })}.${String(ms % 1000).padStart(3, "0")}`;

export function selectedEntry(log: LogEntry[], seq: number | null, clearedAt: number): LogEntry | null {
  if (seq === null || seq <= clearedAt) return null;
  return log.find((e) => e.seq === seq) ?? null;
}

export interface Prefs {
  columns: Column[];
  show: Show;
  background: boolean;
}

export function parsePrefs(raw: string | null): Prefs {
  const prefs: Prefs = { columns: [...DEFAULT_COLUMNS], show: "all", background: false };
  try {
    const p = JSON.parse(raw ?? "{}") as Record<string, unknown>;
    if (Array.isArray(p.columns)) prefs.columns = COLUMNS.filter((c) => (p.columns as unknown[]).includes(c));
    if (SHOWS.includes(p.show as Show)) prefs.show = p.show as Show;
    if (typeof p.background === "boolean") prefs.background = p.background;
  } catch {
    // Unreadable settings are the defaults.
  }
  return prefs;
}
