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

/** Which read a status was loaded under: connection and profile. */
export function loadKey(s: { device: boolean; profile: number | null }): string {
  return `${s.device}:${s.profile}`;
}

/** What a status asks of the view; a reconnect reads again from the keyboard even under the same profile. */
export function statusAction(
  s: { device: boolean; profile: number | null },
  loadedProfile: number | null,
  loadedKey: string | null,
): { revert: boolean; load: boolean } {
  return {
    revert: loadedProfile !== null && s.profile !== null && s.profile !== loadedProfile,
    load: loadKey(s) !== loadedKey,
  };
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

/** Esc clears selections, unless it edits a text field or a component took it already. */
export function escapesSelection(event: Pick<KeyboardEvent, "key" | "defaultPrevented" | "target">): boolean {
  if (event.key !== "Escape" || event.defaultPrevented) return false;
  const target = event.target as { tagName?: string; type?: string; isContentEditable?: boolean } | null;
  const text = target?.tagName === "INPUT" && !["checkbox", "radio", "range", "button"].includes(target.type ?? "");
  return !(text || target?.tagName === "TEXTAREA" || target?.isContentEditable);
}
