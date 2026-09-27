<script setup lang="ts">
import { computed } from "vue";
import type { Status } from "../types";

const props = defineProps<{ status: Status | null; progress: [number, number] | null; message: string; error: string }>();

const view = computed(() => {
  const s = props.status;
  if (props.error) return { kind: "warn", text: props.error };
  if (!s) return { kind: "info", text: "Поиск клавиатуры…" };
  if (s.synapse)
    return {
      kind: "warn",
      text: "Запущен Synapse (RazerAppEngine). Закройте его, включая значок в трее, — иначе он перезапишет настройки.",
    };
  if (!s.device && s.unsupported != null) {
    const pid = s.unsupported.toString(16).toUpperCase().padStart(4, "0");
    return { kind: "warn", text: `Модель клавиатуры Razer не поддерживается (PID ${pid}).` };
  }
  if (!s.device) return { kind: "warn", text: "Поддерживаемая клавиатура Razer не найдена." };
  if (props.progress) return { kind: "info", text: `Чтение клавиатуры: ${props.progress[0]} из ${props.progress[1]}` };
  if (props.message) return { kind: "info", text: props.message };
  return { kind: "ok", text: `Подключено: ${s.model}, профиль ${s.profile ?? "?"}` };
});
</script>

<template>
  <div class="status" :class="view.kind">{{ view.text }}</div>
</template>

<style scoped>
.status {
  padding: 8px 16px;
  border-radius: 6px;
  background: var(--panel);
}
.warn {
  background: #3a2a10;
  color: #ffcf7a;
}
.ok {
  color: var(--accent);
}
</style>
