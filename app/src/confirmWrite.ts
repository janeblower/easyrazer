import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "./types";

/** Asks before a flash write unless the user turned the question off; `write` starts it, the dialog calls `onConfirm`. */
export function useConfirmWrite(doWrite: () => Promise<void>, onError: (error: unknown) => void) {
  const asking = ref(false);

  async function write() {
    try {
      const settings = await invoke<AppSettings>("app_settings");
      if (settings.confirm_write) {
        asking.value = true;
        return;
      }
    } catch (error) {
      onError(error);
      return;
    }
    await doWrite();
  }

  async function onConfirm(dontAsk: boolean) {
    asking.value = false;
    if (dontAsk) await invoke("set_confirm_write", { on: false }).catch(onError);
    await doWrite();
  }

  return { asking, write, onConfirm };
}
