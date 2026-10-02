import { onUnmounted, reactive, watchEffect } from "vue";

// The window is destroyed in the tray: edits not yet sent to the keyboard go with it.
const views = reactive(new Set<symbol>());

export const hasUnapplied = () => views.size > 0;

export function trackUnapplied(dirty: () => boolean) {
  const id = Symbol("view");
  watchEffect(() => (dirty() ? views.add(id) : views.delete(id)));
  onUnmounted(() => views.delete(id));
}
