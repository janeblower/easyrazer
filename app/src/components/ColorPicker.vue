<script setup lang="ts">
import { ref, useId, useTemplateRef, watch } from "vue";
import type { Rgb } from "../types";

const model = defineModel<Rgb | null>({ required: true });
const props = defineProps<{ disabled: boolean }>();
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
const popup = useTemplateRef<HTMLElement>("popup");
// Colour to come back to after "no colour" is switched off again.
const last = ref<Rgb>(model.value ?? [0x44, 0xd6, 0x2c]);
watch(model, (v) => v && (last.value = v));

const hex = (rgb: Rgb) => `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
const rgbOf = (h: string) => [1, 3, 5].map((i) => Number.parseInt(h.slice(i, i + 2), 16)) as Rgb;

const pick = (h: string) => (model.value = rgbOf(h));
const toggleNone = () => (model.value = model.value ? null : last.value);

watch(
  () => props.disabled,
  (d) => d && popup.value?.hidePopover(),
);
</script>

<template>
  <span class="inline-flex">
    <button
      class="p-0 border-line rounded-md h-[30px] w-9 relative overflow-hidden"
      :class="{ empty: !model }"
      :style="{ background: hex(last), anchorName: `--${id}` }"
      :popovertarget="id"
      :disabled="disabled"
      :title="model ? $t('picker.color') : $t('picker.none')"
      :aria-label="model ? $t('picker.colorHex', { hex: hex(model) }) : $t('picker.none')"
    ></button>
    <div
      :id="id"
      ref="popup"
      popover
      class="popup text-inherit p-2.5 border border-line rounded-lg border-solid bg-panel w-[200px] shadow-[0_6px_20px_rgba(0,0,0,0.5)]"
      :style="{ positionAnchor: `--${id}` }"
      role="dialog"
      :aria-label="$t('picker.open')"
    >
      <div class="gap-1.5 grid grid-cols-6">
        <button
          v-for="c in PRESETS"
          :key="c"
          class="p-0 border-line rounded-[5px] size-[26px]"
          :style="{ background: c }"
          :aria-label="c"
          @click="pick(c)"
        ></button>
      </div>
      <div class="mt-2.5 flex gap-1.5 items-center justify-between">
        <label class="text-xs px-2 py-1 border border-line rounded-md border-solid bg-key cursor-pointer relative">
          {{ $t("picker.other") }}
          <input
            class="opacity-0 cursor-pointer inset-0 absolute"
            type="color"
            :value="hex(last)"
            @input="pick(($event.target as HTMLInputElement).value)"
          />
        </label>
        <button
          class="text-xs px-2 py-1"
          :class="{ 'border-error bg-error text-white': !model }"
          :aria-pressed="!model"
          @click="toggleNone"
        >
          {{ $t("picker.noColor") }}
        </button>
      </div>
    </div>
  </span>
</template>

<style scoped>
.popup {
  inset: auto;
  margin: 6px 0 0;
  top: anchor(bottom);
  left: anchor(left);
}
.empty {
  opacity: 0.35;
}
.empty::after {
  content: "";
  position: absolute;
  left: -4px;
  top: 50%;
  width: 44px;
  border-top: 2px solid var(--error);
  transform: rotate(-35deg);
}
</style>
