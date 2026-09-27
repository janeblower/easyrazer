<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";

const emit = defineEmits<{ answer: [on: boolean]; later: [] }>();
const yes = ref<HTMLButtonElement | null>(null);
const onKey = (e: KeyboardEvent) => {
  if (e.key === "Escape") emit("later");
};

onMounted(() => {
  window.addEventListener("keydown", onKey);
  yes.value?.focus();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="dlg-backdrop" @click.self="emit('later')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="ao-title">
      <h3 id="ao-title" class="dlg-title">Запускать EasyRazer вместе с Windows?</h3>
      <p>Программа будет в трее и сама вернёт подсветку.</p>
      <div class="dlg-btns">
        <button @click="emit('answer', false)">Не надо</button>
        <button ref="yes" class="primary" @click="emit('answer', true)">Включить</button>
      </div>
    </div>
  </div>
</template>
