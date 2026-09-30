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
  <!-- The hint goes first: its width changes, the buttons stay put. -->
  <div class="flex gap-3 items-center">
    <span class="text-muted" :title="single ? undefined : $t('common.selectHow')">{{
      count ? $t("common.selected", { n: count }) : $t(single ? "bindings.selectHint" : "common.selectHint")
    }}</span>
    <button v-if="!single" @click="emit('selectAll')">{{ $t("common.selectAll") }}</button>
    <button :disabled="!count" @click="emit('clear')">{{ $t("common.clearSelection") }}</button>
  </div>
</template>
