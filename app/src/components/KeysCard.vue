<script setup lang="ts">
defineProps<{
  count: number;
  dirty: number;
  canApply: boolean;
  canSave: boolean;
  busy: boolean;
}>();
const emit = defineEmits<{
  apply: [];
  save: [];
  revert: [];
  selectAll: [];
  clear: [];
}>();
</script>

<template>
  <div class="px-4 py-3 card flex flex-col gap-3">
    <div class="flex gap-3 items-center">
      <button @click="emit('selectAll')">{{ $t("common.selectAll") }}</button>
      <button :disabled="!count" @click="emit('clear')">{{ $t("common.clearSelection") }}</button>
      <span class="text-muted">{{ count ? $t("common.selected", { n: count }) : $t("common.selectHint") }}</span>
    </div>
    <slot />
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
