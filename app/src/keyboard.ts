// The keyboard the window shows; every keyboard command and event is about one keyboard.
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { i18n } from "./i18n";
import type { DeviceEntry } from "./types";

export const keyboard = ref<string | null>(null);

/** A connected keyboard first; with none connected, the one shown stays so its profiles keep showing. `stay` keeps the one shown while it is known. */
export function pick(list: DeviceEntry[], current: string | null, stay = false): string | null {
  const now = list.find((d) => d.id === current);
  if (now?.connected || (stay && now)) return now.id;
  return list.find((d) => d.connected)?.id ?? now?.id ?? list[0]?.id ?? null;
}

// eslint-disable-next-line @typescript-eslint/promise-function-async -- a plain wrapper of invoke
export function call<T>(cmd: string, args: Record<string, unknown> = {}, id = keyboard.value): Promise<T> {
  // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- like invoke, rejects with the message
  if (id === null) return Promise.reject(i18n.global.t("backend.noKeyboard"));
  return invoke<T>(cmd, { ...args, id });
}

export function ours(id: string | null): boolean {
  return id === keyboard.value;
}
