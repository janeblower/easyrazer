import { computed, onUnmounted, reactive } from "vue";

/** Where an edit lives, to open it from the status bar. */
export type Place = "actuation" | "bindings" | "gamepad" | "macros" | "lighting";

/** A part of the app with edits of its own, shown and acted on together with the others in the status bar. */
export interface Section {
  id: "keys" | "lighting" | "macros";
  /** Edits not applied yet; 0 when there are none. */
  pending: () => number;
  /** The parts that have them. */
  parts: () => { label: string; place: Place }[];
  canApply: () => boolean;
  canRevert: () => boolean;
  canWrite: () => boolean;
  apply: () => Promise<void>;
  revert: () => void;
  /** Puts what is applied into the flash; the caller has asked already. */
  write: () => Promise<void>;
  /** Edits the flash cannot hold, which Write passes by. */
  skipsWrite?: () => boolean;
}

export function summarize(list: Section[]) {
  const dirty = list.filter((s) => s.pending() > 0);
  return {
    pending: dirty.reduce((n, s) => n + s.pending(), 0),
    parts: dirty.flatMap((s) => s.parts()),
    canApply: list.some((s) => s.canApply()),
    canRevert: list.some((s) => s.canRevert()),
    canWrite: list.some((s) => s.canWrite()),
    skipsWrite: dirty.some((s) => s.skipsWrite?.() ?? false),
  };
}

// One after another, asking each in turn: an earlier one can leave the next unable to act.
export async function applyAll(list: Section[]) {
  for (const s of list) if (s.canApply()) await s.apply();
}

export function revertAll(list: Section[]) {
  for (const s of list) if (s.canRevert()) s.revert();
}

export async function writeAll(list: Section[]) {
  for (const s of list) if (s.canWrite()) await s.write();
}

const ORDER: Section["id"][] = ["keys", "lighting", "macros"];
const registry = reactive(new Map<symbol, Section>());

export const sections = computed(() => [...registry.values()].toSorted((a, b) => ORDER.indexOf(a.id) - ORDER.indexOf(b.id)));

// The window is destroyed in the tray: edits not yet sent to the keyboard go with it.
export const hasUnapplied = () => sections.value.some((s) => s.pending() > 0);

export function useSection(section: Section) {
  const id = Symbol(section.id);
  registry.set(id, section);
  onUnmounted(() => registry.delete(id));
}
