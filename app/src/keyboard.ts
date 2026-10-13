// The keyboard the window shows; every keyboard command and event is about one keyboard.
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceEntry } from "./types";

export const keyboard = ref<string | null>(null);

/** A connected keyboard first; with none connected, the one shown stays so its profiles keep showing. `stay` keeps the one shown while it is known. */
export function pick(list: DeviceEntry[], current: string | null, stay = false): string | null {
  const now = list.find((d) => d.id === current);
  if (now?.connected || (stay && now)) return now.id;
  return list.find((d) => d.connected)?.id ?? now?.id ?? list[0]?.id ?? null;
}

export async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>(cmd, { ...args, id: keyboard.value });
}

export function ours(id: string | null): boolean {
  return id === keyboard.value;
}
