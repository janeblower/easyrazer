import type { CommandInfo } from "./types";

export type Registry = Record<string, CommandInfo>;

export interface Parsed {
  cmd: string;
  size: number;
  args: number[];
}

export interface Suggestion {
  text: string;
  info?: CommandInfo;
  recent?: boolean;
}

const CODE = /^[0-9a-f]{2}:[0-9a-f]{2}$/i;
const BYTE = /^[0-9a-f]{1,2}$/i;
const ARGS_LEN = 80; // packet::ARGS_LEN

export const hex = (bytes: number[]) => bytes.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join(" ");

// Getters have bit 7 of the id set (packet::Command::is_get).
export const isWrite = (cmd: string) => (Number.parseInt(cmd.slice(3), 16) & 0x80) === 0;

/** `name bytes…` or `CC:II bytes…`; bytes and `size=NN` in hex. A string is the error. */
export function parse(line: string, registry: Registry): Parsed | string {
  const [head = "", ...rest] = line.trim().split(/\s+/);
  if (!head) return "empty";
  const info = registry[head.toLowerCase()];
  const cmd = info?.cmd ?? (CODE.test(head) ? head.toUpperCase() : null);
  if (!cmd) return `unknown command ${head}`;
  let size: number | undefined;
  const args: number[] = [];
  for (const t of rest) {
    if (t.toLowerCase().startsWith("size=")) {
      const v = t.slice(5);
      if (!BYTE.test(v)) return `bad size ${v}`;
      size = Number.parseInt(v, 16);
    } else if (BYTE.test(t)) args.push(Number.parseInt(t, 16));
    else return `bad byte ${t}`;
  }
  if (args.length > ARGS_LEN) return `more than ${ARGS_LEN} bytes`;
  return { cmd, size: size ?? info?.size ?? args.length, args };
}

/** Recent lines first while nothing is typed, then the commands whose name, code or help hold the first word. */
export function suggest(input: string, registry: Registry, recent: string[]): Suggestion[] {
  const q = (input.trim().split(/\s+/)[0] ?? "").toLowerCase();
  const commands = Object.entries(registry)
    .filter(([name, c]) => !q || name.includes(q) || c.cmd.toLowerCase().includes(q) || c.help.toLowerCase().includes(q))
    .map(([text, info]) => ({ text, info }));
  return q ? commands : [...recent.map((text) => ({ text, recent: true })), ...commands];
}

export type Confirm =
  { kind: "slot"; slot: number } | { kind: "flash" } | { kind: "unknown" } | { kind: "unusual"; value: number; help: string };

/** Why a line is confirmed before it is sent: a flash write or an unusual value; `null` sends at once. */
export function confirmation(p: Parsed, registry: Registry): Confirm | null {
  if (!isWrite(p.cmd)) return null;
  const info = Object.values(registry).find((c) => c.cmd === p.cmd);
  // What an unknown write does is anyone's guess, so it is confirmed like the worst case.
  if (!info) return { kind: "unknown" };
  if (info.safe) {
    const [arg, values] = info.safe;
    const value = p.args[arg] ?? 0;
    if (!values.includes(value)) return { kind: "unusual", value, help: info.help };
  }
  if (info.flash === true) return { kind: "flash" };
  const slot = typeof info.flash === "number" ? (p.args[info.flash] ?? 0) : 0;
  return slot === 0 ? null : { kind: "slot", slot };
}
