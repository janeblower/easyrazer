<script setup lang="ts">
import { onUnmounted, ref, watch } from "vue";
import type { Rgb } from "../types";

const props = defineProps<{ modelValue: Rgb | null; disabled: boolean }>();
const emit = defineEmits<{ "update:modelValue": [rgb: Rgb | null] }>();

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

const open = ref(false);
const root = ref<HTMLElement | null>(null);
// Colour to come back to after "no colour" is switched off again.
const last = ref<Rgb>(props.modelValue ?? [0x44, 0xd6, 0x2c]);
watch(
  () => props.modelValue,
  (v) => v && (last.value = v),
);

const hex = (rgb: Rgb) => `#${rgb.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
const rgbOf = (h: string) => [1, 3, 5].map((i) => Number.parseInt(h.slice(i, i + 2), 16)) as Rgb;

const pick = (h: string) => {
  emit("update:modelValue", rgbOf(h));
};
const toggleNone = () => {
  emit("update:modelValue", props.modelValue ? null : last.value);
};

const onOutside = (e: MouseEvent) => root.value && !root.value.contains(e.target as Node) && (open.value = false);
watch(open, (o) => {
  o ? document.addEventListener("mousedown", onOutside) : document.removeEventListener("mousedown", onOutside);
});
watch(
  () => props.disabled,
  (d) => d && (open.value = false),
);
onUnmounted(() => {
  document.removeEventListener("mousedown", onOutside);
});
</script>

<template>
  <span ref="root" class="inline-flex relative">
    <button
      class="p-0 border-[#555] rounded-md h-[30px] w-9 relative overflow-hidden"
      :class="{ empty: !modelValue }"
      :style="{ background: hex(last) }"
      :disabled="disabled"
      :title="modelValue ? $t('picker.color') : $t('picker.none')"
      :aria-label="modelValue ? $t('picker.colorHex', { hex: hex(modelValue) }) : $t('picker.none')"
      @click="open = !open"
    ></button>
    <div
      v-if="open"
      class="p-2.5 border border-line rounded-lg border-solid bg-panel w-[200px] shadow-[0_6px_20px_rgba(0,0,0,0.5)] left-0 top-9 absolute z-10"
      role="dialog"
      :aria-label="$t('picker.open')"
      @keydown.esc="open = false"
    >
      <div class="gap-1.5 grid grid-cols-6">
        <button
          v-for="c in PRESETS"
          :key="c"
          class="p-0 border-[#555] rounded-[5px] size-[26px]"
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
          :class="{ 'border-error bg-error text-white': !modelValue }"
          :aria-pressed="!modelValue"
          @click="toggleNone"
        >
          {{ $t("picker.noColor") }}
        </button>
      </div>
    </div>
  </span>
</template>

<style scoped>
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
