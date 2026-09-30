<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyMap, KeyView, Rgb } from "../types";

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
    edited?: Set<number>;
    errors?: KeyMap<string>;
    unsaved?: Set<number>;
    colors?: KeyMap<Rgb>;
    /** Keys with Rapid Trigger on, and those whose Rapid Trigger is not applied yet. */
    rapid?: Set<number>;
    rapidEdited?: Set<number>;
    /** Shrink the whole map to the container width instead of scrolling. */
    fit?: boolean;
    /** One key at a time: no Ctrl+click and no box. */
    single?: boolean;
  }>(),
  {
    values: () => ({}),
    edited: () => new Set(),
    errors: () => ({}),
    unsaved: () => new Set(),
    colors: undefined,
    rapid: () => new Set(),
    rapidEdited: () => new Set(),
    fit: false,
    single: false,
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

onMounted(() => {
  if (!props.fit) return;
  observer = new ResizeObserver(() => {
    scale.value = Math.min(1, box.value!.clientWidth / width.value);
  });
  observer.observe(box.value!);
});
onUnmounted(() => observer?.disconnect());

function norm(b: Band): Band {
  return { x0: Math.min(b.x0, b.x1), y0: Math.min(b.y0, b.y1), x1: Math.max(b.x0, b.x1), y1: Math.max(b.y0, b.y1) };
}

const bandStyle = computed(() => {
  const b = norm(band.value!);
  return { left: `${b.x0}px`, top: `${b.y0}px`, width: `${b.x1 - b.x0}px`, height: `${b.y1 - b.y0}px` };
});

const keys = computed(() => props.layout.filter((k) => k.shape !== "ring"));
const rings = computed(() => props.layout.filter((k) => k.shape === "ring"));

function ringStyle(k: KeyView) {
  const c = props.colors?.[k.key];
  return {
    left: `${k.x * U}px`,
    top: `${k.y * U}px`,
    width: `${k.w * U}px`,
    height: `${k.h * U}px`,
    borderWidth: `${RING * U}px`,
    borderColor: c ? `rgb(${c.join(" ")})` : "var(--line)",
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
    ? ["cursor-pointer", selected && "outline-2 outline-solid outline-text outline-offset-1"]
    : k.editable
      ? "bg-key cursor-pointer"
      : "bg-key-off text-muted cursor-default";
  return [border, look, k.shape === "round" ? "rounded-full !items-center !justify-center" : "rounded-md"];
}

// Painted keys keep their label readable on light and dark colors.
function keyStyle(k: KeyView) {
  const box = { left: `${k.x * U}px`, top: `${k.y * U}px`, width: `${k.w * U - GAP}px`, height: `${k.h * U - GAP}px` };
  if (!props.colors) return box;
  const [r, g, b] = props.colors[k.key] ?? [0, 0, 0];
  const light = 0.299 * r + 0.587 * g + 0.114 * b > 140;
  return { ...box, background: `rgb(${r} ${g} ${b})`, color: light ? "#111" : "#eee" };
}

function valueClass(key: number) {
  if (props.edited.has(key)) return "font-semibold text-edited";
  return props.unsaved.has(key) ? "text-accent" : "text-muted";
}

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
    <div ref="root" class="relative" :style="size" @pointerdown="down" @pointermove="move" @pointerup="up">
      <div
        v-for="k in rings"
        :key="`ring-${k.key}`"
        class="rounded-lg border-solid pointer-events-none box-border absolute"
        :class="{ 'outline-2 outline-solid outline-text outline-offset-2': selection.has(k.key) }"
        :style="ringStyle(k)"
      ></div>
      <div
        v-for="k in keys"
        :key="k.key"
        :data-key="k.key"
        class="px-[5px] py-[3px] border-2 border-solid flex flex-col justify-between absolute"
        :class="keyClass(k)"
        :style="keyStyle(k)"
        :title="title(k.key)"
      >
        <span class="text-xs">{{ k.key === 203 ? $t("zones.wrist") : k.label }}</span>
        <span v-if="!colors" class="text-[11px] flex gap-1 items-baseline">
          <span v-if="rapid.has(k.key)" :class="rapidEdited.has(k.key) ? 'font-semibold text-edited' : 'text-accent'">RT</span>
          <span v-if="k.editable && values[k.key] != null" class="ml-auto truncate" :class="valueClass(k.key)">{{ values[k.key] }}</span>
        </span>
      </div>
      <div v-if="band" class="border border-accent border-dashed bg-accent/8 pointer-events-none absolute" :style="bandStyle"></div>
    </div>
  </div>
</template>
