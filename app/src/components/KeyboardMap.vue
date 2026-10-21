<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyMap, KeyView, Rgb, System } from "../types";
import { escapesSelection } from "../keyboard";
import { keyName } from "../bindings";
import { ARROWS, step } from "../mapNav";
import AppIcon from "./AppIcon.vue";

interface Band {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

const selection = defineModel<Set<number>>("selection", { required: true });
// With `colors` the keys are painted instead of showing `values`.
const props = withDefaults(
  defineProps<{
    layout: KeyView[];
    /** Text under the label, and the keys whose value is not applied yet. */
    values?: KeyMap<string>;
    /** Show `values` in place of the label. */
    inline?: boolean;
    /** Hypershift press point, on keys where it differs from `values`; shown above it. */
    hsValues?: KeyMap<string>;
    hsEdited?: Set<number>;
    /** Hypershift binding, on keys where it differs from the normal one; system actions show as icons. */
    hsBinds?: KeyMap<{ text: string; icon?: System }>;
    edited?: Set<number>;
    errors?: KeyMap<string>;
    unsaved?: Set<number>;
    colors?: KeyMap<Rgb>;
    /** Rapid Trigger label of the keys with it on, and keys whose Rapid Trigger is not applied yet. */
    rapid?: Map<number, string>;
    rapidEdited?: Set<number>;
    /** Snap Tap group color per key, shown as a bar along the top edge. */
    snap?: KeyMap<string>;
    /** Shrink the whole map to the container width instead of scrolling. */
    fit?: boolean;
    /** One key at a time: no Ctrl+click and no box. */
    single?: boolean;
    /** Editable keys shown as read, in layout order; `null` when the map is not being read. */
    loaded?: number | null;
  }>(),
  {
    values: () => ({}),
    hsValues: () => ({}),
    hsEdited: () => new Set(),
    hsBinds: () => ({}),
    edited: () => new Set(),
    errors: () => ({}),
    unsaved: () => new Set(),
    colors: undefined,
    rapid: () => new Map(),
    rapidEdited: () => new Set(),
    snap: () => ({}),
    loaded: null,
  },
);
// Width of a ring's frame, in key units.
const RING = 0.3;
const { t } = useI18n();
// Zones are named by the window; key caps keep the labels from the layout.
const ZONES: Record<number, string> = { 200: "zones.media", 201: "zones.dial", 202: "zones.edge", 203: "zones.wrist" };
const U = 50;
// Space between key caps; the map ends at the last cap, not at the space after it.
const GAP = 4;
const box = useTemplateRef<HTMLElement>("box");
const root = useTemplateRef<HTMLElement>("root");
const scale = ref(1);
let observer: ResizeObserver | undefined;
const band = ref<Band | null>(null);
let start: [number, number] | null = null;
let additive = false;

const width = computed(() => Math.max(0, Math.max(0, ...props.layout.map((k) => k.x + k.w)) * U - GAP));
const size = computed(() => ({
  width: `${width.value}px`,
  height: `${Math.max(0, Math.max(0, ...props.layout.map((k) => k.y + k.h)) * U - GAP)}px`,
  zoom: scale.value,
}));

function escapeSelection(event: KeyboardEvent) {
  if (!escapesSelection(event)) return;
  selection.value = new Set();
  band.value = null;
  start = null;
}

onMounted(() => {
  window.addEventListener("keydown", escapeSelection);
  if (!props.fit) return;
  observer = new ResizeObserver(() => {
    scale.value = Math.min(1, box.value!.clientWidth / width.value);
  });
  observer.observe(box.value!);
});
onUnmounted(() => {
  observer?.disconnect();
  window.removeEventListener("keydown", escapeSelection);
});

function norm(b: Band): Band {
  return { x0: Math.min(b.x0, b.x1), y0: Math.min(b.y0, b.y1), x1: Math.max(b.x0, b.x1), y1: Math.max(b.y0, b.y1) };
}

const bandStyle = computed(() => {
  const b = norm(band.value!);
  return { left: `${b.x0}px`, top: `${b.y0}px`, width: `${b.x1 - b.x0}px`, height: `${b.y1 - b.y0}px` };
});

// The editable keys light up in layout order as the whole read goes on.
const readOrder = computed(() => new Map(props.layout.filter((k) => k.editable).map((k, i) => [k.key, i])));
function isRead(key: number) {
  const i = readOrder.value.get(key);
  return props.loaded == null || i == null || i < props.loaded;
}

const keys = computed(() => props.layout.filter((k) => k.shape !== "ring"));
const rings = computed(() => props.layout.filter((k) => k.shape === "ring"));

// One tab stop for the whole map; arrows move between the keys.
const cursor = ref<number | null>(null);
const navKeys = computed(() => keys.value.filter((k) => k.editable));
const navRing = computed(() => rings.value.find((k) => k.editable));
const focusable = computed(() => {
  const all = [...navKeys.value, ...(navRing.value ? [navRing.value] : [])];
  if (all.some((k) => k.key === cursor.value)) return cursor.value;
  return (all.find((k) => selection.value.has(k.key)) ?? all[0])?.key ?? null;
});
// The ring frames the whole board: it is past every edge, and leaving it returns to the key it was entered from.
let entered: KeyView | undefined;

function tabindex(k: KeyView) {
  if (!k.editable) return;
  return k.key === focusable.value ? 0 : -1;
}

function label(k: KeyView) {
  if (k.key in ZONES) return t(ZONES[k.key]);
  const value = props.inline ? undefined : props.values[k.key];
  return [keyName(k, t), value, props.hsBinds[k.key]?.text, props.errors[k.key]].filter((p) => p != null).join(", ");
}

async function focus(key: number) {
  cursor.value = key;
  await nextTick();
  root.value?.querySelector<HTMLElement>(`[data-nav="${key}"]`)?.focus();
}

function target(from: KeyView, e: KeyboardEvent) {
  const dir = ARROWS[e.key];
  if (dir) {
    if (from.shape === "ring") return entered ?? navKeys.value[0];
    const next = step(navKeys.value, from, dir);
    if (!next && navRing.value) entered = from;
    return next ?? navRing.value;
  }
  if (e.key === "Home") return navKeys.value[0];
  if (e.key === "End") return navKeys.value.at(-1);
}

function onKey(e: KeyboardEvent) {
  const from = props.layout.find((k) => k.key === Number((e.target as HTMLElement).dataset.nav));
  if (!from) return;
  const multi = !props.single;
  if (e.key === " " || e.key === "Enter") {
    e.preventDefault();
    const toggle = (e.ctrlKey || e.metaKey) && multi;
    const next = toggle ? new Set(selection.value) : new Set<number>();
    if (toggle && next.has(from.key)) next.delete(from.key);
    else next.add(from.key);
    selection.value = next;
    return;
  }
  if (e.key === "a" && (e.ctrlKey || e.metaKey) && multi) {
    e.preventDefault();
    selection.value = new Set(props.layout.filter((k) => k.editable).map((k) => k.key));
    return;
  }
  const to = target(from, e);
  if (!to) return;
  e.preventDefault();
  if (e.shiftKey && multi) selection.value = new Set([...selection.value, from.key, to.key]);
  void focus(to.key);
}

function ringStyle(k: KeyView) {
  const c = props.colors?.[k.key];
  return {
    left: `${k.x * U}px`,
    top: `${k.y * U}px`,
    width: `${k.w * U}px`,
    height: `${k.h * U}px`,
    borderWidth: `${RING * U}px`,
    borderColor: "var(--line)",
    // The color runs along the frame's inner edge, like the strip on a painted key.
    boxShadow: c ? `inset 0 0 0 4px rgb(${c.join(" ")})` : undefined,
  };
}

// A ring is hit on its frame only; the keys inside stay reachable.
function onRing(k: KeyView, x0: number, y0: number, x1: number, y1: number) {
  const f = RING * U;
  const [l, t, r, b] = [k.x * U, k.y * U, (k.x + k.w) * U, (k.y + k.h) * U];
  const overlaps = l < x1 && r > x0 && t < y1 && b > y0;
  const inside = x0 >= l + f && x1 <= r - f && y0 >= t + f && y1 <= b - f;
  return overlaps && !inside;
}

function keyClass(k: KeyView) {
  const selected = selection.value.has(k.key);
  const border = k.key in props.errors ? "border-error" : selected ? "border-accent" : "border-transparent";
  const look = props.colors
    ? ["bg-key cursor-pointer", selected && "outline-2 outline-solid outline-text outline-offset-1"]
    : k.editable && isRead(k.key)
      ? "bg-key cursor-pointer"
      : "bg-key-off text-muted cursor-default";
  return [border, look, k.shape === "round" ? "rounded-full !items-center !justify-center" : "rounded-md"];
}

// A painted key stays a dark cap with its color as a strip along the bottom: filled caps glared in
// the dark theme. A shrunk map keeps its labels at a readable size.
function keyStyle(k: KeyView) {
  const bar = props.snap[k.key];
  const box = {
    left: `${k.x * U}px`,
    top: `${k.y * U}px`,
    width: `${k.w * U - GAP}px`,
    height: `${k.h * U - GAP}px`,
    boxShadow: bar ? `inset 0 3px 0 ${bar}` : undefined,
  };
  if (!props.colors) return box;
  const c = props.colors[k.key];
  return {
    ...box,
    boxShadow: c ? `inset 0 -5px 0 rgb(${c.join(" ")})` : undefined,
    fontSize: `${scale.value < 1 ? 11 / scale.value : 12}px`,
  };
}

function valueClass(key: number) {
  if (props.edited.has(key)) return "font-semibold text-edited";
  return props.unsaved.has(key) ? "text-accent" : "text-muted";
}

const labelClass = (key: number) => (props.edited.has(key) ? "font-semibold text-edited" : "text-accent");

function title(key: number) {
  if (key in ZONES) return t(ZONES[key]);
  return props.errors[key] ?? (props.unsaved.has(key) ? t("actuation.unsavedKey") : undefined);
}

function point(e: PointerEvent): [number, number] {
  // The rect is zoomed, the key geometry is not.
  const r = root.value!.getBoundingClientRect();
  return [(e.clientX - r.left) / scale.value, (e.clientY - r.top) / scale.value];
}

function down(e: PointerEvent) {
  start = point(e);
  additive = e.ctrlKey && !props.single;
  root.value!.setPointerCapture(e.pointerId);
}

function move(e: PointerEvent) {
  if (!start || props.single) return;
  const [x, y] = point(e);
  if (band.value || Math.hypot(x - start[0], y - start[1]) > 4) {
    band.value = { x0: start[0], y0: start[1], x1: x, y1: y };
  }
}

function up(e: PointerEvent) {
  if (!start) return;
  const next = additive ? new Set(selection.value) : new Set<number>();
  if (band.value) {
    const b = norm(band.value);
    for (const k of props.layout) {
      const hit =
        k.shape === "ring"
          ? onRing(k, b.x0, b.y0, b.x1, b.y1)
          : k.x * U < b.x1 && (k.x + k.w) * U > b.x0 && k.y * U < b.y1 && (k.y + k.h) * U > b.y0;
      if (k.editable && hit) next.add(k.key);
    }
  } else {
    // Pointer capture retargets events to the root, so find the key under the cursor explicitly.
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-key]");
    const [x, y] = point(e);
    const k = el ? props.layout.find((k) => k.key === Number(el.dataset.key)) : rings.value.find((r) => onRing(r, x, y, x, y));
    if (k?.editable) {
      cursor.value = k.key;
      if (additive && next.has(k.key)) next.delete(k.key);
      else next.add(k.key);
    }
  }
  start = null;
  band.value = null;
  selection.value = next;
}
</script>

<template>
  <div ref="box">
    <div
      ref="root"
      class="mx-auto relative"
      :style="size"
      role="listbox"
      :aria-label="$t('common.keyMap')"
      :aria-multiselectable="!single"
      @pointerdown="down"
      @pointermove="move"
      @pointerup="up"
      @keydown="onKey"
    >
      <div
        v-for="k in rings"
        :key="`ring-${k.key}`"
        class="map-cap rounded-lg border-solid pointer-events-none box-border absolute"
        :class="{ 'outline-2 outline-solid outline-text outline-offset-2': selection.has(k.key) }"
        :style="ringStyle(k)"
        role="option"
        :data-nav="k.key"
        :tabindex="tabindex(k)"
        :aria-label="label(k)"
        :aria-selected="selection.has(k.key)"
        :aria-disabled="!k.editable || undefined"
      ></div>
      <div
        v-for="k in keys"
        :key="k.key"
        :data-key="k.key"
        :data-nav="k.key"
        role="option"
        :tabindex="tabindex(k)"
        :aria-label="label(k)"
        :aria-selected="selection.has(k.key)"
        :aria-disabled="!k.editable || undefined"
        class="map-cap px-[5px] py-[3px] border-2 border-solid flex flex-col transition-colors duration-300 justify-between absolute"
        :class="keyClass(k)"
        :style="keyStyle(k)"
        :title="title(k.key)"
      >
        <span v-if="inline && values[k.key] != null" class="text-xs truncate" :class="labelClass(k.key)">{{ values[k.key] }}</span>
        <span v-else :class="{ 'text-xs': !colors }">{{ k.key === 203 ? $t("zones.wrist") : k.label }}</span>
        <span v-if="!colors && isRead(k.key)" class="text-[11px] flex gap-1 items-end">
          <span
            v-if="rapid.has(k.key)"
            :class="[rapidEdited.has(k.key) ? 'font-semibold text-edited' : 'text-accent', { 'text-[9px]': rapid.get(k.key) === 'CRT' }]"
            >{{ rapid.get(k.key) }}</span
          >
          <span
            v-if="hsBinds[k.key]"
            class="flex min-w-0"
            :class="hsEdited.has(k.key) ? 'font-semibold text-edited' : 'text-hs'"
            :title="hsBinds[k.key].text"
          >
            <AppIcon v-if="hsBinds[k.key].icon" class="h-[13px] w-[13px]" :name="hsBinds[k.key].icon!" />
            <span v-else class="truncate">{{ hsBinds[k.key].text }}</span>
          </span>
          <span
            v-if="k.editable && !inline && values[k.key] != null"
            class="ml-auto flex flex-col min-w-0 items-end"
            :class="{ 'text-[9px] leading-[10px]': hsValues[k.key] != null }"
          >
            <span v-if="hsValues[k.key] != null" class="truncate" :class="hsEdited.has(k.key) ? 'font-semibold text-edited' : 'text-hs'">{{
              hsValues[k.key]
            }}</span>
            <span class="truncate" :class="valueClass(k.key)">{{ values[k.key] }}</span>
          </span>
        </span>
      </div>
      <div v-if="band" class="border border-accent border-dashed bg-accent/8 pointer-events-none absolute" :style="bandStyle"></div>
    </div>
  </div>
</template>

<style scoped>
.map-cap:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
</style>
