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
const RT_MIN = 0.1;
const RT_MAX = 1;
// Half of the slider thumb: the thumb centre never reaches the track ends.
const THUMB = 8;

// Where `f` of the slider's range sits along the track.
const at = (f: number) => `calc(${THUMB}px + ${f} * (100% - ${2 * THUMB}px))`;
const markTop = (v: number) => at((v - MIN) / (MAX - MIN));
// The mark lights up only when the chosen point is past it and the firmware will clamp it.
const clamped = (m: number) => props.value != null && (m === HW_MIN ? props.value < HW_MIN : props.value > HW_MAX);

// The first key held, streamed only in driver mode: depth 0..255 over MIN..MAX, travel since its
// last extreme on the same scale, and whether it is down.
const lead = ref<[number, number, boolean]>([0, 0, false]);
let unlisten: UnlistenFn | undefined;
onMounted(async () => (unlisten = await listen<[number, number, boolean]>("key-depth", (e) => (lead.value = e.payload))));
onUnmounted(() => unlisten?.());
const units = (mm: number) => (mm / (MAX - MIN)) * 255;
const NONE = { "--from": "0px", "--to": "0px" };
const span = (from: number, to: number) => ({ "--from": at(from), "--to": at(to) });
const fill = computed(() => (props.driver && lead.value[0] ? span(0, lead.value[0] / 255) : NONE));
// Next move of the key: up to release once it is down, down to press otherwise.
const arrow = computed(() => (props.driver && lead.value[0] ? (lead.value[2] ? "up" : "down") : null));
// Press grows down from the top and meets the thumb as the key goes down; release grows up
// from the thumb and reaches the top as the key lets go.
function rtFill(mm: number | null, down: boolean) {
  const [, travel, isDown] = lead.value;
  if (!rtOn.value || !travel || isDown !== down) return NONE;
  if (!down) return span(0, (travel - units(RT_MIN)) / units(RT_MAX - RT_MIN));
  const v = mm ?? 0.4;
  const thumb = (v - RT_MIN) / (RT_MAX - RT_MIN);
  return span(thumb * (1 - Math.min(1, travel / units(v))), thumb);
}
// Unsplit, the one slider stands for both.
const pressFill = computed(() => rtFill(props.rapid.press, props.split ? false : lead.value[2]));
const releaseFill = computed(() => rtFill(props.rapid.release, true));
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
          step="0.1"
          :value="value ?? MIN"
          :disabled="!count"
          :style="fill"
          @input="emit('set', mm($event))"
        />
        <svg
          v-if="arrow"
          class="text-accent pointer-events-none right-[calc(100%+4px)] absolute -translate-y-1/2"
          :style="{ top: fill['--to'] }"
          viewBox="0 0 10 10"
          width="10"
          height="10"
        >
          <path :d="arrow === 'up' ? 'M5 1 9 8H1z' : 'M5 9 9 2H1z'" fill="currentColor" />
        </svg>
        <template v-if="!driver">
          <div
            v-for="m in [HW_MIN, HW_MAX]"
            :key="m"
            class="h-[2px] w-[calc(100%+12px)] pointer-events-none left-[-6px] absolute"
            :class="clamped(m) ? 'bg-warn text-warn' : 'bg-muted text-muted'"
            :style="{ top: markTop(m) }"
          >
            <span class="text-xs left-[calc(100%+4px)] top-[-8px] absolute">{{ m }}</span>
          </div>
        </template>
      </div>
      <span class="text-xs text-muted">3.6</span>
      <span class="text-sm text-center min-w-[60px]">{{ label(value) }}</span>
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
            class="depth h-[160px] [writing-mode:vertical-lr]"
            type="range"
            :min="RT_MIN"
            :max="RT_MAX"
            step="0.1"
            :value="rapid.press ?? 0.4"
            :disabled="!rtOn"
            :style="pressFill"
            @input="emit('press', mm($event))"
          />
          <span class="text-xs text-muted">1.0</span>
          <span class="text-sm text-center min-w-[60px]">{{ label(rapid.press) }}</span>
        </div>
        <div v-if="split" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">{{ $t("rapid.release") }}</span>
          <span class="text-xs text-muted">0.1</span>
          <input
            class="depth h-[160px] [writing-mode:vertical-lr]"
            type="range"
            :min="RT_MIN"
            :max="RT_MAX"
            step="0.1"
            :value="rapid.release ?? 0.4"
            :disabled="!rtOn"
            :style="releaseFill"
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
/* The track shows only the thumb centre's range, so the deepest value fills it whole. */
.depth::-webkit-slider-runnable-track {
  width: 6px;
  background: linear-gradient(
    to bottom,
    transparent 8px,
    var(--key) 8px,
    var(--key) var(--from),
    var(--accent) var(--from),
    var(--accent) var(--to),
    var(--key) var(--to),
    var(--key) calc(100% - 8px),
    transparent calc(100% - 8px)
  );
}
.depth::-webkit-slider-thumb {
  appearance: none;
  width: 16px;
  height: 16px;
  margin-left: -5px;
  border-radius: 50%;
  background: var(--text);
}
.depth:disabled::-webkit-slider-thumb {
  background: var(--muted);
}
</style>
