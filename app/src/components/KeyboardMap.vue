<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { KeyMap, KeyView, Rgb } from "../types";

interface Band {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

// With `colors` the keys are painted instead of showing press points.
const props = withDefaults(
  defineProps<{
    layout: KeyView[];
    baseline?: KeyMap<number>;
    edits?: KeyMap<number>;
    errors?: KeyMap<string>;
    unsaved?: Set<number>;
    colors?: KeyMap<Rgb>;
    selection: Set<number>;
    /** Shrink the whole map to the container width instead of scrolling. */
    fit?: boolean;
  }>(),
  { baseline: () => ({}), edits: () => ({}), errors: () => ({}), unsaved: () => new Set(), colors: undefined, fit: false },
);
const emit = defineEmits<{ "update:selection": [selection: Set<number>] }>();

// Width of a ring's frame, in key units.
const RING = 0.3;
const U = 50;
const box = ref<HTMLElement | null>(null);
const root = ref<HTMLElement | null>(null);
const scale = ref(1);
let observer: ResizeObserver | undefined;
const band = ref<Band | null>(null);
let start: [number, number] | null = null;
let additive = false;

const width = computed(() => Math.max(0, ...props.layout.map((k) => k.x + k.w)) * U);
const size = computed(() => ({
  width: `${width.value}px`,
  height: `${Math.max(0, ...props.layout.map((k) => k.y + k.h)) * U}px`,
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
  const [r, g, b] = props.colors?.[k.key] ?? [0x3a, 0x3a, 0x3a];
  const u = U;
  return {
    left: `${k.x * u}px`,
    top: `${k.y * u}px`,
    width: `${k.w * u}px`,
    height: `${k.h * u}px`,
    borderWidth: `${RING * u}px`,
    borderColor: `rgb(${r} ${g} ${b})`,
  };
}

// A ring is hit on its frame only; the keys inside stay reachable.
function onRing(k: KeyView, x0: number, y0: number, x1: number, y1: number) {
  const u = U;
  const t = RING * u;
  const [l, t0, r, b] = [k.x * u, k.y * u, (k.x + k.w) * u, (k.y + k.h) * u];
  const overlaps = l < x1 && r > x0 && t0 < y1 && b > y0;
  const inside = x0 >= l + t && x1 <= r - t && y0 >= t0 + t && y1 <= b - t;
  return overlaps && !inside;
}

function keyClass(k: KeyView) {
  const selected = props.selection.has(k.key);
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
  const u = U;
  const box = { left: `${k.x * u}px`, top: `${k.y * u}px`, width: `${k.w * u - 4}px`, height: `${k.h * u - 4}px` };
  if (!props.colors) return box;
  const [r, g, b] = props.colors[k.key] ?? [0, 0, 0];
  const light = 0.299 * r + 0.587 * g + 0.114 * b > 140;
  return { ...box, background: `rgb(${r} ${g} ${b})`, color: light ? "#111" : "#eee" };
}

function valueClass(key: number) {
  if (key in props.edits) return "font-semibold text-edited";
  return props.unsaved.has(key) ? "text-accent" : "text-muted";
}

function title(key: number) {
  return props.errors[key] ?? (props.unsaved.has(key) ? "Применено, но не записано в клавиатуру" : undefined);
}

function value(key: number): number | undefined {
  return props.edits[key] ?? props.baseline[key];
}

function point(e: PointerEvent): [number, number] {
  // The rect is zoomed, the key geometry is not.
  const r = root.value!.getBoundingClientRect();
  return [(e.clientX - r.left) / scale.value, (e.clientY - r.top) / scale.value];
}

function down(e: PointerEvent) {
  start = point(e);
  additive = e.ctrlKey;
  root.value!.setPointerCapture(e.pointerId);
}

function move(e: PointerEvent) {
  if (!start) return;
  const [x, y] = point(e);
  if (band.value || Math.hypot(x - start[0], y - start[1]) > 4) {
    band.value = { x0: start[0], y0: start[1], x1: x, y1: y };
  }
}

function up(e: PointerEvent) {
  if (!start) return;
  const next = additive ? new Set(props.selection) : new Set<number>();
  if (band.value) {
    const b = norm(band.value);
    const u = U;
    for (const k of props.layout) {
      const hit =
        k.shape === "ring"
          ? onRing(k, b.x0, b.y0, b.x1, b.y1)
          : k.x * u < b.x1 && (k.x + k.w) * u > b.x0 && k.y * u < b.y1 && (k.y + k.h) * u > b.y0;
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
  emit("update:selection", next);
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
        <span class="text-xs">{{ k.label }}</span>
        <span v-if="!colors && k.editable && value(k.key) != null" class="text-[11px] self-end" :class="valueClass(k.key)">{{
          value(k.key)!.toFixed(1)
        }}</span>
      </div>
      <div v-if="band" class="border border-accent border-dashed bg-accent/8 pointer-events-none absolute" :style="bandStyle"></div>
    </div>
  </div>
</template>
