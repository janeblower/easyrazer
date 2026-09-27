<script setup lang="ts">
defineProps<{ count: number; value: number | null; dirty: number; canApply: boolean; busy: boolean }>();
const emit = defineEmits<{ set: [mm: number]; apply: []; revert: []; selectAll: []; clear: [] }>();

function onInput(e: Event) {
  emit("set", Math.round(Number((e.target as HTMLInputElement).value) * 10) / 10);
}
</script>

<template>
  <div class="px-4 py-3 card flex gap-3 items-center">
    <button @click="emit('selectAll')">Выделить все</button>
    <button :disabled="!count" @click="emit('clear')">Снять выделение</button>
    <span class="text-muted min-w-[260px]">{{ count ? `Выделено: ${count}` : "Выделите клавиши: клик, Ctrl+клик, рамка" }}</span>
    <input class="w-[220px]" type="range" min="1.5" max="3.6" step="0.1" :value="value ?? 1.5" :disabled="!count" @input="onInput" />
    <span class="min-w-[60px]">{{ value != null ? `${value.toFixed(1)} мм` : count ? "разные" : "" }}</span>
    <span class="flex-1"></span>
    <button :disabled="!dirty || busy" @click="emit('revert')">Отменить</button>
    <button class="primary" :disabled="!canApply" @click="emit('apply')">Применить{{ dirty ? ` (${dirty})` : "" }}</button>
  </div>
</template>
