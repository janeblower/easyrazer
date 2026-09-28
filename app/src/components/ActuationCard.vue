<script setup lang="ts">
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
const label = (v: number | null) => (v == null ? (props.count ? t("actuation.mixed") : "") : t("actuation.mm", { v: v.toFixed(1) }));
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
        <input
          class="h-[200px] [writing-mode:vertical-lr]"
          type="range"
          min="1.5"
          max="3.6"
          step="0.1"
          :value="value ?? 1.5"
          :disabled="!count"
          @input="emit('set', mm($event))"
        />
        <span class="text-xs text-muted">3.6</span>
        <span class="text-sm text-center min-w-[60px]">{{ label(value) }}</span>
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
            :disabled="!count"
            @change="emit('rapid', checked($event))"
          />
          {{ $t("rapid.title") }}
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
              :disabled="rapid.enabled !== true"
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
              :disabled="rapid.enabled !== true"
              @input="emit('release', mm($event))"
            />
            <span class="text-xs text-muted">1.0</span>
            <span class="text-sm text-center min-w-[60px]">{{ label(rapid.release) }}</span>
          </div>
        </div>
        <label class="text-xs flex gap-2 items-center">
          <input type="checkbox" :checked="split" :disabled="rapid.enabled !== true" @change="emit('split', checked($event))" />
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
