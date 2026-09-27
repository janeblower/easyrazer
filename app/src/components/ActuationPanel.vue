<script setup lang="ts">
defineProps<{ count: number; value: number | null; dirty: number; canApply: boolean; canSave: boolean; busy: boolean }>();
const emit = defineEmits<{ set: [mm: number]; apply: []; save: []; revert: []; selectAll: []; clear: [] }>();

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
    <span v-if="dirty" class="text-xs text-edited">● не применено: {{ dirty }}</span>
    <button class="icon-btn" title="Отменить изменения" aria-label="Отменить изменения" :disabled="!dirty || busy" @click="emit('revert')">
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
    <button class="primary" :disabled="!canApply" @click="emit('apply')">Применить</button>
    <button
      class="text-[#ffb070] icon-btn border-[#8a5a20] bg-transparent"
      title="Записать в память клавиатуры"
      aria-label="Записать в память клавиатуры"
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
</template>
