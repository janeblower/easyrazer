<script setup lang="ts">
defineProps<{
  count: number;
  single?: boolean;
}>();
const emit = defineEmits<{
  selectAll: [];
  clear: [];
}>();
</script>

<template>
  <div class="px-4 py-3 card flex flex-col gap-3">
    <div class="flex gap-3 items-center">
      <button v-if="!single" @click="emit('selectAll')">{{ $t("common.selectAll") }}</button>
      <button :disabled="!count" @click="emit('clear')">{{ $t("common.clearSelection") }}</button>
      <span class="text-muted" :title="single ? undefined : $t('common.selectHow')">{{
        count ? $t("common.selected", { n: count }) : $t(single ? "bindings.selectHint" : "common.selectHint")
      }}</span>
    </div>
    <slot />
  </div>
</template>
