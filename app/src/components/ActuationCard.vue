<script setup lang="ts">
import { computed } from "vue";
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
  dirty: number;
  canApply: boolean;
  canSave: boolean;
  busy: boolean;
  driver: boolean;
}>();
const emit = defineEmits<{
  set: [mm: number];
  rapid: [on: boolean];
  press: [mm: number];
  release: [mm: number];
  split: [on: boolean];
  apply: [];
  save: [];
  revert: [];
  selectAll: [];
  clear: [];
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

function onPress(e: Event) {
  const input = e.target as HTMLInputElement;
  const v = Math.round(effective(Number(input.value)) * 10) / 10;
  emit("set", v);
  // The slider moves in 0.01 so the thumb can stop on the hardware marks; snap it to what was set.
  input.value = String(effective(v));
}
</script>

<template>
  <div class="px-4 py-3 card flex flex-col gap-3">
    <div class="flex gap-3 items-center">
      <button @click="emit('selectAll')">{{ $t("common.selectAll") }}</button>
      <button :disabled="!count" @click="emit('clear')">{{ $t("common.clearSelection") }}</button>
      <span class="text-muted">{{ count ? $t("common.selected", { n: count }) : $t("common.selectHint") }}</span>
    </div>
    <div class="flex gap-10 justify-center">
      <div class="flex flex-col gap-1 items-center">
        <span class="text-sm">{{ $t("actuation.title") }}</span>
        <span class="text-xs text-muted">1.5</span>
        <div class="relative">
          <input
            class="h-[200px] block [writing-mode:vertical-lr]"
            type="range"
            :min="MIN"
            :max="MAX"
            step="0.01"
            :value="effective(value ?? MIN)"
            :disabled="!count"
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
      <img
        src="/switch.gif"
        alt=""
        class="h-[240px] self-center"
        @error="($event.target as HTMLImageElement).style.visibility = 'hidden'"
      />
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
    <div class="flex gap-3 items-center justify-end">
      <span v-if="dirty" class="text-xs text-edited">{{ $t("common.notAppliedN", { n: dirty }) }}</span>
      <button
        class="icon-btn"
        :title="$t('common.revert')"
        :aria-label="$t('common.revert')"
        :disabled="!dirty || busy"
        @click="emit('revert')"
      >
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M9 14 4 9l5-5" />
          <path d="M4 9h11a5 5 0 0 1 0 10h-3" />
        </svg>
      </button>
      <button class="primary" :disabled="!canApply" @click="emit('apply')">{{ $t("common.apply") }}</button>
      <button
        class="text-[#ffb070] icon-btn border-[#8a5a20] bg-transparent"
        :title="$t('common.write')"
        :aria-label="$t('common.write')"
        :disabled="!canSave"
        @click="emit('save')"
      >
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
          <path d="M7 3v5h8V3" />
          <path d="M7 21v-7h10v7" />
        </svg>
      </button>
    </div>
  </div>
</template>
