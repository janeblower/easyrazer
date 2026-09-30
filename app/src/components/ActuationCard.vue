<script setup lang="ts">
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";

export interface RapidView {
  enabled: boolean | null;
  press: number | null;
  release: number | null;
}

const props = defineProps<{
  count: number;
  value: number | null;
  rapid: RapidView;
  split: boolean;
  driver: boolean;
}>();
const emit = defineEmits<{
  set: [mm: number];
  rapid: [on: boolean];
  press: [mm: number];
  release: [mm: number];
  split: [on: boolean];
}>();
const { t } = useI18n();

const mm = (e: Event) => Math.round(Number((e.target as HTMLInputElement).value) * 10) / 10;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const rtOn = computed(() => props.driver && props.rapid.enabled === true);
const label = (v: number | null) => (v == null ? (props.count ? t("actuation.mixed") : "") : t("actuation.mm", { v: v.toFixed(1) }));

const MIN = 1.5;
const MAX = 3.6;
// In hardware mode the firmware clamps the press point to thresholds 15..250.
const HW_MIN = 1.62;
const HW_MAX = 3.56;
// Half of the native slider thumb: the thumb centre never reaches the track ends.
const THUMB = 8;
const HW_MARKS = [HW_MIN, HW_MAX];

const effective = (v: number) => (props.driver ? v : Math.min(HW_MAX, Math.max(HW_MIN, v)));
const markTop = (v: number) => `calc(${THUMB}px + ${(v - MIN) / (MAX - MIN)} * (100% - ${2 * THUMB}px))`;
const pressLabel = computed(() => {
  if (props.value == null) return label(null);
  const v = effective(props.value);
  return t("actuation.mm", { v: v === HW_MIN || v === HW_MAX ? v.toFixed(2) : v.toFixed(1) });
});

// Depth of the first key held, 0..255 over MIN..MAX; the keyboard streams it only in driver mode.
const depth = ref(0);
let unlisten: UnlistenFn | undefined;
onMounted(async () => (unlisten = await listen<number>("key-depth", (e) => (depth.value = e.payload))));
onUnmounted(() => unlisten?.());
const fill = computed(() => (props.driver && depth.value ? markTop(MIN + (depth.value / 255) * (MAX - MIN)) : "0px"));

function onPress(e: Event) {
  const input = e.target as HTMLInputElement;
  const v = Math.round(effective(Number(input.value)) * 10) / 10;
  emit("set", v);
  // The slider moves in 0.01 so the thumb can stop on the hardware marks; snap it to what was set.
  input.value = String(effective(v));
}
</script>

<template>
  <div class="flex gap-10 justify-center">
    <div class="flex flex-col gap-1 items-center">
      <span class="text-sm">{{ $t("actuation.title") }}</span>
      <span class="text-xs text-muted">1.5</span>
      <div class="relative">
        <input
          class="depth h-[200px] block [writing-mode:vertical-lr]"
          type="range"
          :min="MIN"
          :max="MAX"
          step="0.01"
          :value="effective(value ?? MIN)"
          :disabled="!count"
          :style="{ '--fill': fill }"
          @input="onPress"
        />
        <template v-if="!driver">
          <div
            v-for="m in HW_MARKS"
            :key="m"
            class="bg-warn h-[2px] w-[calc(100%+12px)] pointer-events-none left-[-6px] absolute"
            :style="{ top: markTop(m) }"
          >
            <span class="text-xs text-warn left-[calc(100%+4px)] top-[-8px] absolute">{{ m }}</span>
          </div>
        </template>
      </div>
      <span class="text-xs text-muted">3.6</span>
      <span class="text-sm text-center min-w-[60px]">{{ pressLabel }}</span>
    </div>
    <img src="/switch.gif" alt="" class="h-[240px] self-center" @error="($event.target as HTMLImageElement).style.visibility = 'hidden'" />
    <div class="flex flex-col gap-1 items-center">
      <label class="text-sm flex gap-2 items-center">
        <input
          type="checkbox"
          :checked="rapid.enabled === true"
          :indeterminate="count > 0 && rapid.enabled === null"
          :disabled="!count || !driver"
          @change="emit('rapid', checked($event))"
        />
        {{ $t("rapid.title") }}
        <span v-if="!driver" class="text-xs text-muted">{{ $t("rapid.hwOnly") }}</span>
      </label>
      <div class="flex gap-6">
        <div class="flex flex-col gap-1 items-center">
          <span v-if="split" class="text-xs text-muted">{{ $t("rapid.press") }}</span>
          <span class="text-xs text-muted">0.1</span>
          <input
            class="h-[160px] [writing-mode:vertical-lr]"
            type="range"
            min="0.1"
            max="1"
            step="0.1"
            :value="rapid.press ?? 0.4"
            :disabled="!rtOn"
            @input="emit('press', mm($event))"
          />
          <span class="text-xs text-muted">1.0</span>
          <span class="text-sm text-center min-w-[60px]">{{ label(rapid.press) }}</span>
        </div>
        <div v-if="split" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">{{ $t("rapid.release") }}</span>
          <span class="text-xs text-muted">0.1</span>
          <input
            class="h-[160px] [writing-mode:vertical-lr]"
            type="range"
            min="0.1"
            max="1"
            step="0.1"
            :value="rapid.release ?? 0.4"
            :disabled="!rtOn"
            @input="emit('release', mm($event))"
          />
          <span class="text-xs text-muted">1.0</span>
          <span class="text-sm text-center min-w-[60px]">{{ label(rapid.release) }}</span>
        </div>
      </div>
      <label class="text-xs flex gap-2 items-center">
        <input type="checkbox" :checked="split" :disabled="!rtOn" @change="emit('split', checked($event))" />
        {{ $t("rapid.split") }}
      </label>
    </div>
  </div>
</template>

<style scoped>
.depth {
  appearance: none;
  width: 16px;
  background: transparent;
}
.depth::-webkit-slider-runnable-track {
  width: 6px;
  border-radius: 3px;
  background: linear-gradient(to bottom, var(--accent) var(--fill), var(--key) var(--fill));
}
.depth::-webkit-slider-thumb {
  appearance: none;
  width: 16px;
  height: 16px;
  margin-left: -5px;
  border-radius: 50%;
  background: var(--text);
}
</style>
