<script setup lang="ts">
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ours } from "../keyboard";
import type { Analog, RapidMode } from "../types";
import AppIcon from "./AppIcon.vue";

interface RapidView {
  mode: RapidMode | null;
  press: number | null;
  release: number | null;
}

const props = defineProps<{
  count: number;
  value: number | null;
  rapid: RapidView;
  split: boolean;
  /** Hypershift press point of the selection; with `hsSplit` off it follows `value`. */
  hs: number | null;
  hsSplit: boolean;
  driver: boolean;
  /** The firmware's continuous Rapid Trigger for the whole profile; `null` where it is per key. */
  crt: boolean | null;
  /** Scale of the connected keyboard; `null` falls back to the Huntsman V2 Analog's. */
  analog: Analog | null;
}>();
const emit = defineEmits<{
  set: [mm: number];
  rapid: [mode: RapidMode];
  crt: [on: boolean];
  press: [mm: number];
  release: [mm: number];
  split: [on: boolean];
  hs: [mm: number];
  hsSplit: [on: boolean];
}>();
const { t } = useI18n();

const mm = (e: Event) => Math.round(Number((e.target as HTMLInputElement).value) * 10) / 10;
// The host runs Rapid Trigger in driver mode, some firmwares in HW mode too.
const rtHere = computed(() => props.driver || !!props.analog?.firmware_rapid);
const rtOn = computed(() => rtHere.value && props.rapid.mode != null && props.rapid.mode !== "off");
const modes = computed<RapidMode[]>(() => (props.crt == null ? ["off", "rt", "crt"] : ["off", "rt"]));
const label = (v: number | null) => (v == null ? t("actuation.mixed") : v.toFixed(1));

const V2: Analog = { zero_mm: 1.5, full_mm: 3.6, range: [1.5, 3.6], hw_range: [1.62, 3.56], firmware_rapid: false };
const scale = computed(() => props.analog ?? V2);
const MIN = computed(() => scale.value.range[0]);
const MAX = computed(() => scale.value.range[1]);
// Where the firmware clamps the press point in HW mode, if it does.
const hwMarks = computed(() => (props.driver ? [] : (scale.value.hw_range ?? [])));
const RT_MIN = 0.1;
const RT_MAX = 1;
// Half of the slider thumb: the thumb centre never reaches the track ends.
const THUMB = 8;

// Where `f` of the slider's range sits along the track.
const at = (f: number) => `calc(${THUMB}px + ${f} * (100% - ${2 * THUMB}px))`;
const frac = (v: number) => (v - MIN.value) / (MAX.value - MIN.value);
const markTop = (v: number) => at(frac(v));
// The mark lights up only when the chosen point is past it and the firmware will clamp it.
const clamped = (m: number) => props.value != null && (m === hwMarks.value[0] ? props.value < m : props.value > m);

// The first key held, streamed only in driver mode: depth 0..255 over zero_mm..full_mm, travel since
// its last extreme on the same scale, and whether it is down.
const lead = ref<[number, number, boolean]>([0, 0, false]);
let unlisten: UnlistenFn | undefined;
onMounted(
  async () =>
    (unlisten = await listen<[string, number, number, boolean]>("key-depth", (e) => {
      const [id, ...depth] = e.payload;
      if (ours(id)) lead.value = depth;
    })),
);
onUnmounted(() => unlisten?.());
const units = (mm: number) => (mm / (scale.value.full_mm - scale.value.zero_mm)) * 255;
const depthMm = (d: number) => scale.value.zero_mm + (d / 255) * (scale.value.full_mm - scale.value.zero_mm);
// With nothing selected the thumbs rest mid-track; "any" lets them sit between the 0.1 steps.
const step = computed(() => (props.count ? 0.1 : "any"));
const mid = (min: number, max: number, v: number | null | undefined, fallback: number) => (props.count ? (v ?? fallback) : (min + max) / 2);

const NONE = { "--from": "0px", "--to": "0px" };
const span = (from: number, to: number) => ({ "--from": at(from), "--to": at(to) });
const fill = computed(() => (props.driver && lead.value[0] ? span(0, Math.min(1, Math.max(0, frac(depthMm(lead.value[0]))))) : NONE));
// With nothing selected the scale has no thumb and reads the lead key's depth instead.
const live = computed(() => (props.driver && lead.value[0] ? depthMm(lead.value[0]).toFixed(1) : ""));
const hwHint = computed(() =>
  hwMarks.value.length > 0 ? t("actuation.hwLimits", { min: hwMarks.value[0], max: hwMarks.value.at(-1) }) : undefined,
);
// Next move of the key: up to release once it is down, down to press otherwise.
const arrow = computed(() => (props.driver && lead.value[0] ? (lead.value[2] ? "up" : "down") : null));
// Press grows down from the top and meets the thumb as the key goes down; release grows up
// from the thumb and reaches the top as the key lets go.
// With nothing selected it previews the lead key against the idle thumb, like the actuation slider.
function rtFill(v: number, down: boolean) {
  const [, travel, isDown] = lead.value;
  if (!props.driver || (props.count && !rtOn.value) || !travel || isDown !== down) return NONE;
  if (!down) return span(0, (travel - units(RT_MIN)) / units(RT_MAX - RT_MIN));
  const thumb = (v - RT_MIN) / (RT_MAX - RT_MIN);
  return span(thumb * (1 - Math.min(1, travel / units(v))), thumb);
}

const shown = computed(() => mid(MIN.value, MAX.value, props.value, MIN.value));
const hsShown = computed(() => mid(MIN.value, MAX.value, props.hs, MIN.value));
const rtShown = computed(() => [mid(RT_MIN, RT_MAX, props.rapid.press, 0.4), mid(RT_MIN, RT_MAX, props.rapid.release, 0.4)]);
const rtSliders = computed(() => {
  const both = [
    // Unsplit, the one slider stands for both.
    {
      kind: "press",
      value: props.rapid.press,
      shown: rtShown.value[0],
      fill: rtFill(rtShown.value[0], props.split ? false : lead.value[2]),
    },
    { kind: "release", value: props.rapid.release, shown: rtShown.value[1], fill: rtFill(rtShown.value[1], true) },
  ] as const;
  return props.split ? both : both.slice(0, 1);
});
</script>

<template>
  <div class="flex gap-6 items-start justify-center">
    <div class="flex flex-col gap-1 items-center">
      <span class="text-sm flex h-6 items-center">{{ $t("actuation.title") }}, {{ $t("actuation.unit") }}</span>
      <span class="inline-flex h-6" role="group" :title="$t('actuation.hsSplit')">
        <button
          v-for="on in [false, true]"
          :key="String(on)"
          class="text-xs px-2 py-0 seg-btn"
          :class="{ 'seg-on': hsSplit === on }"
          :aria-pressed="hsSplit === on"
          :disabled="!count"
          @click="emit('hsSplit', on)"
        >
          {{ on ? "Hypershift" : $t("actuation.shared") }}
        </button>
      </span>
      <!-- Two slider columns wide: one sits centred, a split puts both at the edges, so nothing around moves. -->
      <div class="flex w-[144px]" :class="hsSplit ? 'justify-between' : 'justify-center'">
        <div class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">{{ MIN.toFixed(1) }}</span>
          <!-- Room on both sides for the arrow and the marks, so the scale stays centred. -->
          <div class="mx-7 relative">
            <input
              class="depth h-[144px] block [writing-mode:vertical-lr]"
              :class="{ idle: !count }"
              type="range"
              :min="MIN"
              :max="MAX"
              :step="step"
              :value="shown"
              :disabled="!count"
              :title="hwHint"
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
            <div
              v-for="m in hwMarks"
              :key="m"
              class="h-[2px] w-[calc(100%+12px)] pointer-events-none left-[-6px] absolute"
              :class="clamped(m) ? 'bg-warn' : 'bg-muted'"
              :style="{ top: markTop(m) }"
            />
          </div>
          <span class="text-xs text-muted">{{ MAX.toFixed(1) }}</span>
          <span class="readout">{{ count ? label(value) : live }}</span>
          <span class="text-xs text-muted" :class="{ invisible: !hsSplit }">{{ $t("actuation.normal") }}</span>
        </div>
        <div v-if="hsSplit" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">{{ MIN.toFixed(1) }}</span>
          <div class="mx-7 relative">
            <input
              class="depth depth-hs h-[144px] block [writing-mode:vertical-lr]"
              :class="{ idle: !count }"
              type="range"
              :min="MIN"
              :max="MAX"
              :step="step"
              :value="hsShown"
              :disabled="!count"
              :title="hwHint"
              :style="fill"
              @input="emit('hs', mm($event))"
            />
          </div>
          <span class="text-xs text-muted">{{ MAX.toFixed(1) }}</span>
          <span class="readout">{{ count ? label(hs) : live }}</span>
          <span class="text-xs text-hs">Hypershift</span>
        </div>
      </div>
    </div>
    <svg class="h-[200px] self-center" viewBox="0 0 120 200" aria-hidden="true">
      <g class="press chev">
        <path d="M46 40 60 52 74 40M46 58 60 70 74 58" />
      </g>
      <g class="release chev" style="stroke: var(--text)">
        <path d="M46 52 60 40 74 52M46 70 60 58 74 70" />
      </g>
      <path d="M34 120H86L92 160H28Z" style="fill: var(--key)" />
      <path
        d="M10 168 20 112Q22 100 34 100H86Q98 100 100 112L110 168"
        style="fill: none; stroke: var(--muted)"
        stroke-width="6"
        stroke-linejoin="round"
      />
      <rect x="16" y="160" width="88" height="7" style="fill: var(--line)" />
      <path d="M28 167H92V178H84V194H36V178H28Z" style="fill: var(--key)" />
      <path d="M54 180V194M60 180V194M66 180V194" style="stroke: var(--panel)" stroke-width="2" />
      <path d="M34 172H86" style="stroke: var(--error)" stroke-width="3" stroke-linecap="round" />
    </svg>
    <div class="flex flex-col gap-1 items-center">
      <span class="text-sm flex h-6 items-center">{{ $t("rapid.title") }}, {{ $t("actuation.unit") }}</span>
      <div class="flex gap-2 h-6">
        <span class="inline-flex h-6" role="group" :title="$t('rapid.title')">
          <button
            v-for="m in modes"
            :key="m"
            class="text-xs px-2 py-0 seg-btn"
            :class="{ 'seg-on': rapid.mode === m }"
            :aria-pressed="rapid.mode === m"
            :title="$t(`rapid.${m}Hint`)"
            :disabled="!count || !rtHere"
            @click="emit('rapid', m)"
          >
            {{ $t(`rapid.${m}`) }}
          </button>
        </span>
        <button
          v-if="crt != null"
          class="text-xs px-2 py-0"
          :class="{ 'seg-on': crt }"
          :aria-pressed="crt"
          :title="$t('rapid.crtProfileHint')"
          @click="emit('crt', !crt)"
        >
          CRT
        </button>
      </div>
      <div class="flex w-[144px]" :class="split ? 'justify-between' : 'justify-center'">
        <div v-for="s in rtSliders" :key="s.kind" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">0.1</span>
          <div class="mx-7 relative">
            <input
              class="depth h-[144px] block [writing-mode:vertical-lr]"
              :class="{ idle: !count }"
              type="range"
              :min="RT_MIN"
              :max="RT_MAX"
              :step="step"
              :value="s.shown"
              :disabled="!rtOn"
              :style="s.fill"
              @input="s.kind === 'press' ? emit('press', mm($event)) : emit('release', mm($event))"
            />
          </div>
          <span class="text-xs text-muted">1.0</span>
          <span class="readout">{{ count && rtOn ? label(s.value) : "" }}</span>
          <span class="text-xs text-muted" :class="{ invisible: !split }">{{ $t(`rapid.${s.kind}`) }}</span>
        </div>
      </div>
      <span v-if="!rtHere" class="text-xs text-muted flex gap-1.5 h-6 items-center"><AppIcon name="ban" />{{ $t("rapid.hwOnly") }}</span>
      <span v-else class="inline-flex h-6" role="group" :title="$t('rapid.split')">
        <button
          v-for="on in [false, true]"
          :key="String(on)"
          class="text-xs px-2 py-0 seg-btn"
          :class="{ 'seg-on': split === on }"
          :aria-pressed="split === on"
          :disabled="!rtOn"
          @click="emit('split', on)"
        >
          {{ $t(on ? "rapid.separate" : "rapid.together") }}
        </button>
      </span>
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
    var(--fill, var(--accent)) var(--from),
    var(--fill, var(--accent)) var(--to),
    var(--key) var(--to),
    var(--key) calc(100% - 8px),
    transparent calc(100% - 8px)
  );
}
.depth-hs {
  --fill: var(--hs);
}
.depth::-webkit-slider-thumb {
  appearance: none;
  width: 16px;
  height: 16px;
  margin-left: -5px;
  border-radius: 50%;
  background: var(--text);
}
.readout {
  height: 24px;
  font-size: 16px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.depth:disabled::-webkit-slider-thumb {
  background: var(--muted);
}
/* Unselected, the scale only shows live depth: a resting thumb would read as a value. */
.depth.idle::-webkit-slider-thumb {
  visibility: hidden;
}
.chev {
  fill: none;
  stroke: var(--accent);
  stroke-width: 5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
/* The minifier turns a shorthand without a name into `animation: none`. */
.press {
  animation: press 2s ease-in-out infinite;
}
.release {
  animation: release 2s ease-in-out infinite;
}
@keyframes press {
  0% {
    transform: translateY(-14px);
    opacity: 0;
  }
  15% {
    opacity: 1;
  }
  45% {
    transform: translateY(10px);
    opacity: 1;
  }
  50%,
  100% {
    opacity: 0;
  }
}
@keyframes release {
  0%,
  50% {
    transform: translateY(10px);
    opacity: 0;
  }
  65% {
    opacity: 1;
  }
  95% {
    transform: translateY(-14px);
    opacity: 1;
  }
  100% {
    opacity: 0;
  }
}
@media (prefers-reduced-motion: reduce) {
  .chev {
    animation: none;
  }
  .release {
    opacity: 0;
  }
}
</style>
