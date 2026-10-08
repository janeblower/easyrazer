<script setup lang="ts">
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";

interface RapidView {
  enabled: boolean | null;
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
}>();
const emit = defineEmits<{
  set: [mm: number];
  rapid: [on: boolean];
  press: [mm: number];
  release: [mm: number];
  split: [on: boolean];
  hs: [mm: number];
  hsSplit: [on: boolean];
}>();
const { t } = useI18n();

const mm = (e: Event) => Math.round(Number((e.target as HTMLInputElement).value) * 10) / 10;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const rtOn = computed(() => props.driver && props.rapid.enabled === true);
const label = (v: number | null) => (v == null ? (props.count ? t("actuation.mixed") : "") : v.toFixed(1));

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
// With nothing selected the thumbs rest mid-track; "any" lets them sit between the 0.1 steps.
const step = computed(() => (props.count ? 0.1 : "any"));
const mid = (min: number, max: number, v: number | null | undefined, fallback: number) => (props.count ? (v ?? fallback) : (min + max) / 2);

const NONE = { "--from": "0px", "--to": "0px" };
const span = (from: number, to: number) => ({ "--from": at(from), "--to": at(to) });
const fill = computed(() => (props.driver && lead.value[0] ? span(0, lead.value[0] / 255) : NONE));
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

const shown = computed(() => mid(MIN, MAX, props.value, MIN));
const hsShown = computed(() => mid(MIN, MAX, props.hs, MIN));
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
      <span class="text-sm h-5">{{ $t("actuation.title") }}, {{ $t("actuation.unit") }}</span>
      <span class="inline-flex h-6" role="group" :title="$t('actuation.hsSplit')">
        <button
          v-for="on in [false, true]"
          :key="String(on)"
          class="text-xs px-2 py-0 seg-btn"
          :class="hsSplit === on && (on ? 'border-hs bg-hs text-ink' : 'seg-on')"
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
          <span class="text-xs text-muted">1.5</span>
          <!-- Room on both sides for the marks and the value, so the scale stays centred. -->
          <div class="mx-7 relative">
            <input
              class="depth h-[200px] block [writing-mode:vertical-lr]"
              type="range"
              :min="MIN"
              :max="MAX"
              :step="step"
              :value="shown"
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
                <span class="text-xs right-[calc(100%+4px)] top-[-8px] absolute">{{ m }}</span>
              </div>
            </template>
            <span class="thumb-value" :style="{ top: at((shown - MIN) / (MAX - MIN)) }">{{ label(value) }}</span>
          </div>
          <span class="text-xs text-muted">3.6</span>
          <span class="text-xs text-muted" :class="{ invisible: !hsSplit }">{{ $t("actuation.normal") }}</span>
        </div>
        <div v-if="hsSplit" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">1.5</span>
          <div class="mx-7 relative">
            <input
              class="depth depth-hs h-[200px] block [writing-mode:vertical-lr]"
              type="range"
              :min="MIN"
              :max="MAX"
              :step="step"
              :value="hsShown"
              :disabled="!count"
              :style="fill"
              @input="emit('hs', mm($event))"
            />
            <span class="thumb-value" :style="{ top: at((hsShown - MIN) / (MAX - MIN)) }">{{ label(hs) }}</span>
          </div>
          <span class="text-xs text-muted">3.6</span>
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
      <label class="switch text-sm h-5 whitespace-nowrap">
        <input
          type="checkbox"
          role="switch"
          :checked="rapid.enabled === true"
          :indeterminate="count > 0 && rapid.enabled === null"
          :disabled="!count || !driver"
          @change="emit('rapid', checked($event))"
        />
        {{ $t("rapid.title") }}, {{ $t("actuation.unit") }}
      </label>
      <span v-if="!driver" class="text-xs text-muted flex h-6 items-center">{{ $t("rapid.hwOnly") }}</span>
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
      <div class="flex w-[144px]" :class="split ? 'justify-between' : 'justify-center'">
        <div v-for="s in rtSliders" :key="s.kind" class="flex flex-col gap-1 items-center">
          <span class="text-xs text-muted">0.1</span>
          <div class="mx-7 relative">
            <input
              class="depth h-[200px] block [writing-mode:vertical-lr]"
              type="range"
              :min="RT_MIN"
              :max="RT_MAX"
              :step="step"
              :value="s.shown"
              :disabled="!rtOn"
              :style="s.fill"
              @input="s.kind === 'press' ? emit('press', mm($event)) : emit('release', mm($event))"
            />
            <span class="thumb-value" :style="{ top: at((s.shown - RT_MIN) / (RT_MAX - RT_MIN)) }">{{ label(s.value) }}</span>
          </div>
          <span class="text-xs text-muted">1.0</span>
          <span class="text-xs text-muted" :class="{ invisible: !split }">{{ $t(`rapid.${s.kind}`) }}</span>
        </div>
      </div>
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
.thumb-value {
  position: absolute;
  left: calc(100% + 6px);
  translate: 0 -50%;
  font-size: 12px;
  white-space: nowrap;
  pointer-events: none;
}
.depth:disabled::-webkit-slider-thumb {
  background: var(--muted);
}
.chev {
  fill: none;
  stroke: var(--accent);
  stroke-width: 5;
  stroke-linecap: round;
  stroke-linejoin: round;
  animation: 2s ease-in-out infinite;
}
.press {
  animation-name: press;
}
.release {
  animation-name: release;
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
