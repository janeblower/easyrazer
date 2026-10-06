<script setup lang="ts">
import { ref, useId, watch } from "vue";
import type { Rgb } from "../types";

const model = defineModel<Rgb | null>({ required: true });
defineProps<{ disabled: boolean }>();
const PRESETS = [
  "#ff0000",
  "#ff8000",
  "#ffff00",
  "#44d62c",
  "#00ff80",
  "#00ffff",
  "#0040ff",
  "#8000ff",
  "#ff00ff",
  "#ff4080",
  "#ffffff",
  "#ffc080",
];

const id = useId();
// Colour to come back to after "no colour" is switched off again.
const last = ref<Rgb>(model.value ?? [0x44, 0xd6, 0x2c]);
watch(model, (v) => v && (last.value = v));

const hex = (rgb: Rgb) => `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
const rgbOf = (h: string) => [1, 3, 5].map((i) => Number.parseInt(h.slice(i, i + 2), 16)) as Rgb;
const toggleNone = () => (model.value = model.value ? null : last.value);
</script>

<template>
  <span class="inline-flex gap-1.5 items-center">
    <!-- The datalist gives the picker its swatches. -->
    <input
      class="p-0 border-line rounded-md h-[30px] w-9 cursor-pointer"
      :class="{ 'opacity-35': !model }"
      type="color"
      :list="id"
      :value="hex(last)"
      :disabled="disabled"
      :title="model ? $t('picker.color') : $t('picker.none')"
      :aria-label="model ? $t('picker.colorHex', { hex: hex(model) }) : $t('picker.none')"
      @input="model = rgbOf(($event.target as HTMLInputElement).value)"
    />
    <datalist :id="id">
      <option v-for="c in PRESETS" :key="c" :value="c" />
    </datalist>
    <button
      class="text-xs px-2 py-1"
      :class="{ 'border-error bg-error text-white': !model }"
      :disabled="disabled"
      :aria-pressed="!model"
      @click="toggleNone"
    >
      {{ $t("picker.noColor") }}
    </button>
  </span>
</template>
