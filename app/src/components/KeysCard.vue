<script setup lang="ts">
import ActionBar from "./ActionBar.vue";

defineProps<{
  count: number;
  dirty: number;
  canApply: boolean;
  canSave: boolean;
  busy: boolean;
  single?: boolean;
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
      <button v-if="!single" @click="emit('selectAll')">{{ $t("common.selectAll") }}</button>
      <button :disabled="!count" @click="emit('clear')">{{ $t("common.clearSelection") }}</button>
      <span class="text-muted">{{
        count ? $t("common.selected", { n: count }) : $t(single ? "bindings.selectHint" : "common.selectHint")
      }}</span>
    </div>
    <slot />
    <ActionBar
      :pending="dirty ? $t('common.notAppliedN', { n: dirty }) : ''"
      :can-revert="dirty > 0 && !busy"
      :can-apply="canApply"
      :can-write="canSave"
      @revert="emit('revert')"
      @apply="emit('apply')"
      @write="emit('save')"
    />
  </div>
</template>
