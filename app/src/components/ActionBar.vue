<script setup lang="ts">
import type { Place } from "../changes";
import AppIcon from "./AppIcon.vue";

// Revert and Apply work on the drafts of every tab; Write puts the applied state into the flash.
defineProps<{
  /** What is not applied yet; empty hides it. */
  pending: string;
  /** The parts with edits, to open them. */
  parts: { label: string; place: Place }[];
  canRevert: boolean;
  canApply: boolean;
  canWrite: boolean;
  writeTitle?: string;
}>();
const emit = defineEmits<{ revert: []; apply: []; write: []; open: [place: Place] }>();
</script>

<template>
  <!-- The buttons sit in the status bar; `defer` waits for it to be in the page. -->
  <Teleport defer to="#status-actions">
    <div class="flex gap-3 items-center">
      <span v-if="pending" class="text-xs text-edited whitespace-nowrap">{{ pending }}</span>
      <span v-if="parts.length > 0" class="text-xs text-muted flex gap-1 whitespace-nowrap">
        <template v-for="(p, i) in parts" :key="p.place">
          <span v-if="i > 0" aria-hidden="true">·</span>
          <button
            class="text-xs text-muted p-0 border-0 bg-transparent h-auto hover:text-text hover:underline"
            @click="emit('open', p.place)"
          >
            {{ p.label }}
          </button>
        </template>
      </span>
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
