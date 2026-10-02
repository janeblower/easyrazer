<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Status } from "../types";

const props = defineProps<{ status: Status | null; progress: [number, number] | null; message: string; error: string }>();

const { t } = useI18n();

const KIND: Record<string, string> = { info: "bg-panel", warn: "bg-warn-bg text-warn", ok: "bg-panel text-accent" };

const view = computed(() => {
  const s = props.status;
  if (props.error) return { kind: "warn", text: props.error };
  if (props.progress)
    return { kind: "info", text: t("status.reading", { percent: Math.floor((props.progress[0] / props.progress[1]) * 100) }) };
  if (!s) return { kind: "info", text: t("status.searching") };
  if (s.synapse)
    return {
      kind: "warn",
      text: t("status.synapse"),
    };
  if (!s.device && s.unsupported != null) {
    const pid = s.unsupported.toString(16).toUpperCase().padStart(4, "0");
    return { kind: "warn", text: t("status.unsupported", { pid }) };
  }
  if (!s.device) return { kind: "warn", text: t("status.notFound") };
  if (props.message) return { kind: "info", text: props.message };
  return { kind: "ok", text: t("status.connected", { model: s.model ?? "", profile: s.profile_name ?? "?" }) };
});
</script>

<template>
  <div class="px-4 py-2 rounded-md flex gap-3 min-h-[50px] items-center" :class="KIND[view.kind]">
    <span class="mr-auto">{{ view.text }}</span>
    <div id="status-actions" class="text-text"></div>
  </div>
</template>
