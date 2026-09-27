<script setup lang="ts">
defineProps<{ count: number; value: number | null; dirty: number; canApply: boolean; busy: boolean }>();
const emit = defineEmits<{ set: [mm: number]; apply: []; revert: []; selectAll: []; clear: [] }>();

function onInput(e: Event) {
  emit("set", Math.round(Number((e.target as HTMLInputElement).value) * 10) / 10);
}
</script>

<template>
  <div class="panel">
    <button @click="emit('selectAll')">Выделить все</button>
    <button :disabled="!count" @click="emit('clear')">Снять выделение</button>
    <span class="count">{{ count ? `Выделено: ${count}` : "Выделите клавиши: клик, Ctrl+клик, рамка" }}</span>
    <input type="range" min="1.5" max="3.6" step="0.1" :value="value ?? 1.5" :disabled="!count" @input="onInput" />
    <span class="val">{{ value != null ? `${value.toFixed(1)} мм` : count ? "разные" : "" }}</span>
    <span class="spacer"></span>
    <button :disabled="!dirty || busy" @click="emit('revert')">Отменить</button>
    <button class="primary" :disabled="!canApply" @click="emit('apply')">Применить{{ dirty ? ` (${dirty})` : "" }}</button>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  background: var(--panel);
  border-radius: 6px;
}
.count {
  color: var(--muted);
  min-width: 260px;
}
input[type="range"] {
  width: 220px;
  accent-color: var(--accent);
}
.val {
  min-width: 60px;
}
.spacer {
  flex: 1;
}
</style>
