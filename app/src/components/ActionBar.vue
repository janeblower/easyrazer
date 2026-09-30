<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

// Revert and Apply work on the draft; Write puts the applied state into the flash.
defineProps<{
  /** What is not applied yet; empty hides it. */
  pending: string;
  canRevert: boolean;
  canApply: boolean;
  canWrite: boolean;
  /** Nothing of it can go to the flash: no Write button. */
  applyOnly?: boolean;
  writeTitle?: string;
}>();
const emit = defineEmits<{ revert: []; apply: []; write: [] }>();
</script>

<template>
  <!-- Every tab's buttons sit in the status bar; `defer` waits for it to be in the page. -->
  <Teleport defer to="#status-actions">
    <div class="flex gap-3 items-center">
      <span v-if="pending" class="text-xs text-edited">{{ pending }}</span>
      <button
        class="icon-btn"
        :title="$t('common.revert')"
        :aria-label="$t('common.revert')"
        :disabled="!canRevert"
        @click="emit('revert')"
      >
        <AppIcon name="revert" />
      </button>
      <button class="primary" :disabled="!canApply" @click="emit('apply')">{{ $t("common.apply") }}</button>
      <button
        v-if="!applyOnly"
        class="write-btn"
        :title="writeTitle ?? $t('common.write')"
        :aria-label="writeTitle ?? $t('common.write')"
        :disabled="!canWrite"
        @click="emit('write')"
      >
        <AppIcon name="write" />
      </button>
    </div>
  </Teleport>
</template>
