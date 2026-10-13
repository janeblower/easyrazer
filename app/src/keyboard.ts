// The keyboard the window shows; every keyboard command and event is about one keyboard.
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { i18n } from "./i18n";
import type { DeviceEntry } from "./types";

export const keyboard = ref<string | null>(null);

/** The shown keyboard stays while it is known, so a disconnected one keeps its profiles on screen. */
export function pick(list: DeviceEntry[], current: string | null, last: string | null): string | null {
  const known = (id: string | null) => list.find((d) => d.id === id)?.id;
  return known(current) ?? known(last) ?? list.find((d) => d.connected)?.id ?? list[0]?.id ?? null;
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
