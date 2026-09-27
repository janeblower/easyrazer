<script setup lang="ts">
import { computed, ref } from "vue";
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
  }>(),
  { baseline: () => ({}), edits: () => ({}), errors: () => ({}), unsaved: () => new Set(), colors: undefined },
);
const emit = defineEmits<{ "update:selection": [selection: Set<number>] }>();

const U = 50;
const root = ref<HTMLElement | null>(null);
const band = ref<Band | null>(null);
let start: [number, number] | null = null;
let additive = false;

const size = computed(() => ({
  width: `${Math.max(0, ...props.layout.map((k) => k.x + k.w)) * U}px`,
  height: `${Math.max(0, ...props.layout.map((k) => k.y + k.h)) * U}px`,
}));

function norm(b: Band): Band {
  return { x0: Math.min(b.x0, b.x1), y0: Math.min(b.y0, b.y1), x1: Math.max(b.x0, b.x1), y1: Math.max(b.y0, b.y1) };
}

const bandStyle = computed(() => {
  const b = norm(band.value!);
  return { left: `${b.x0}px`, top: `${b.y0}px`, width: `${b.x1 - b.x0}px`, height: `${b.y1 - b.y0}px` };
});

function keyClass(k: KeyView) {
  const selected = props.selection.has(k.key);
  const border = k.key in props.errors ? "border-error" : selected ? "border-accent" : "border-transparent";
  const look = props.colors
    ? ["cursor-pointer", selected && "outline-2 outline-solid outline-text outline-offset-1"]
    : k.editable
      ? "bg-key cursor-pointer"
      : "bg-key-off text-muted cursor-default";
  return [border, look, k.round ? "rounded-full !items-center !justify-center" : "rounded-md", !k.label && "!p-0"];
}

// Painted keys keep their label readable on light and dark colors.
function keyStyle(k: KeyView) {
  const box = { left: `${k.x * U}px`, top: `${k.y * U}px`, width: `${k.w * U - 4}px`, height: `${k.h * U - 4}px` };
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
  const r = root.value!.getBoundingClientRect();
  return [e.clientX - r.left, e.clientY - r.top];
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
    for (const k of props.layout) {
      const hit = k.x * U < b.x1 && (k.x + k.w) * U > b.x0 && k.y * U < b.y1 && (k.y + k.h) * U > b.y0;
      if (k.editable && hit) next.add(k.key);
    }
  } else {
    // Pointer capture retargets events to the root, so find the key under the cursor explicitly.
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-key]");
    const k = el && props.layout.find((k) => k.key === Number(el.dataset.key));
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
  <div ref="root" class="relative" :style="size" @pointerdown="down" @pointermove="move" @pointerup="up">
    <div
      v-for="(k, i) in layout"
      :key="i"
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
</template>
