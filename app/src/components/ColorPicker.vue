<script setup lang="ts">
import { ref, useId, useTemplateRef, watch } from "vue";
import type { Rgb } from "../types";
import { type Hsv, hexOf, hsvOf, rgbOfHex, rgbOfHsv } from "../color";

const model = defineModel<Rgb | null>({ required: true });
const props = defineProps<{ disabled: boolean }>();
const PRESETS: Rgb[] = [
  [0xff, 0x00, 0x00],
  [0xff, 0x80, 0x00],
  [0xff, 0xff, 0x00],
  [0x44, 0xd6, 0x2c],
  [0x00, 0xff, 0x80],
  [0x00, 0xff, 0xff],
  [0x00, 0x40, 0xff],
  [0x80, 0x00, 0xff],
  [0xff, 0x00, 0xff],
  [0xff, 0x40, 0x80],
  [0xff, 0xff, 0xff],
  [0xff, 0xc0, 0x80],
];
// Arrow keys move the point across the square by this much.
const STEP = 0.02;

const id = useId();
const pop = useTemplateRef<HTMLElement>("pop");
const area = useTemplateRef<HTMLElement>("area");
// Colour to come back to after "no colour" is switched off again.
const last = ref<Rgb>(model.value ?? [0x44, 0xd6, 0x2c]);
// Kept apart from the colour: grey and black carry no hue, and the slider must not jump to red.
const hsv = ref<Hsv>(hsvOf(last.value));
watch(model, (v) => {
  if (!v) return;
  last.value = v;
  if (hexOf(rgbOfHsv(hsv.value)) !== hexOf(v)) hsv.value = hsvOf(v);
});
watch(
  () => props.disabled,
  (off) => off && pop.value?.hidePopover(),
);

const clamp = (n: number) => Math.min(1, Math.max(0, n));

function setHsv(next: Hsv) {
  hsv.value = next;
  model.value = rgbOfHsv(next);
}

function pick(rgb: Rgb) {
  hsv.value = hsvOf(rgb);
  model.value = rgb;
}

function drag(e: PointerEvent) {
  if (e.type === "pointerdown") area.value!.setPointerCapture(e.pointerId);
  else if (e.buttons === 0) return;
  const r = area.value!.getBoundingClientRect();
  setHsv([hsv.value[0], clamp((e.clientX - r.left) / r.width), clamp(1 - (e.clientY - r.top) / r.height)]);
}

function nudge(e: KeyboardEvent) {
  const [h, s, v] = hsv.value;
  const by: Record<string, [number, number]> = { ArrowLeft: [-STEP, 0], ArrowRight: [STEP, 0], ArrowUp: [0, STEP], ArrowDown: [0, -STEP] };
  const d = by[e.key];
  if (!d) return;
  e.preventDefault();
  setHsv([h, clamp(s + d[0]), clamp(v + d[1])]);
}

function typed(e: Event) {
  const input = e.target as HTMLInputElement;
  const rgb = rgbOfHex(input.value);
  if (rgb) pick(rgb);
  input.value = hexOf(model.value ?? last.value).toUpperCase();
}

const toggleNone = () => (model.value = model.value ? null : last.value);
</script>

<template>
  <span class="inline-flex">
    <button
      class="px-1 py-0 flex gap-1.5 h-8 items-center"
      :style="{ anchorName: `--${id}` }"
      :popovertarget="id"
      :disabled="disabled"
      :title="model ? $t('picker.color') : $t('picker.none')"
      :aria-label="model ? $t('picker.colorHex', { hex: hexOf(model) }) : $t('picker.none')"
    >
      <span class="swatch rounded h-6 w-6" :class="{ none: !model }" :style="{ background: hexOf(last) }"></span>
      <span class="text-xs font-mono pr-1 min-w-[56px]" :class="{ 'text-muted': !model }">{{
        model ? hexOf(model).toUpperCase() : $t("picker.none")
      }}</span>
    </button>
    <div :id="id" ref="pop" popover class="pop" :style="{ positionAnchor: `--${id}` }">
      <div
        ref="area"
        class="area"
        tabindex="0"
        :aria-label="$t('picker.area')"
        :style="{ background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hsv[0]} 100% 50%))` }"
        @pointerdown="drag"
        @pointermove="drag"
        @keydown="nudge"
      >
        <span class="dot" :style="{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%` }"></span>
      </div>
      <input
        class="hue"
        type="range"
        min="0"
        max="359"
        :value="Math.round(hsv[0])"
        :aria-label="$t('picker.hue')"
        @input="setHsv([Number(($event.target as HTMLInputElement).value), hsv[1] || 1, hsv[2] || 1])"
      />
      <div class="flex flex-wrap gap-1.5">
        <button
          v-for="c in PRESETS"
          :key="hexOf(c)"
          class="swatch p-0 rounded h-5 w-5"
          :class="{ on: model && hexOf(model) === hexOf(c) }"
          :style="{ background: hexOf(c) }"
          :aria-label="hexOf(c)"
          @click="pick(c)"
        ></button>
      </div>
      <div class="flex gap-2 items-center">
        <input
          class="text-xs font-mono w-[90px]"
          maxlength="7"
          spellcheck="false"
          :value="hexOf(model ?? last).toUpperCase()"
          :aria-label="$t('picker.hex')"
          @change="typed"
        />
        <button class="text-xs ml-auto" :class="{ 'border-accent text-accent': !model }" :aria-pressed="!model" @click="toggleNone">
          {{ $t("picker.noColor") }}
        </button>
      </div>
    </div>
  </span>
</template>

<style scoped>
.pop {
  inset: auto;
  top: anchor(bottom);
  left: anchor(left);
  position-try-fallbacks: flip-block;
  margin: 6px 0;
  width: 236px;
  padding: 10px;
  display: none;
  flex-direction: column;
  gap: 10px;
  background: var(--panel);
  color: inherit;
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgb(0 0 0 / 60%);
}
.pop:popover-open {
  display: flex;
}
.swatch {
  border: 1px solid rgb(255 255 255 / 15%);
}
.swatch.on {
  outline: 2px solid var(--text);
  outline-offset: 1px;
}
/* No colour: the kept colour, faded and struck through. */
.swatch.none {
  opacity: 0.35;
  background-image: linear-gradient(to top right, transparent 46%, var(--text) 46% 54%, transparent 54%) !important;
}
.area {
  position: relative;
  height: 120px;
  border-radius: 5px;
  cursor: crosshair;
  touch-action: none;
}
.dot {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border: 2px solid #fff;
  border-radius: 50%;
  box-shadow: 0 0 0 1px rgb(0 0 0 / 50%);
  pointer-events: none;
}
.hue {
  appearance: none;
  width: 100%;
  height: 10px;
  margin: 0;
  border-radius: 5px;
  background: linear-gradient(to right, red, #ff0, lime, cyan, blue, #f0f, red);
}
.hue::-webkit-slider-thumb {
  appearance: none;
  width: 14px;
  height: 14px;
  border: 2px solid #fff;
  border-radius: 50%;
  background: transparent;
  box-shadow: 0 0 0 1px rgb(0 0 0 / 50%);
}
</style>
