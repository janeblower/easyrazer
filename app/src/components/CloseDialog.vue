<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import type { CloseAction } from "../types";

const emit = defineEmits<{ choose: [action: CloseAction, remember: boolean]; cancel: [] }>();
const remember = ref(false);
const tray = ref<HTMLButtonElement | null>(null);
const onKey = (e: KeyboardEvent) => {
  if (e.key === "Escape") emit("cancel");
};

onMounted(() => {
  window.addEventListener("keydown", onKey);
  tray.value?.focus();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="dlg-backdrop" @click.self="emit('cancel')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="cd-title">
      <h3 id="cd-title" class="dlg-title">Закрыть окно</h3>
      <p>В трее EasyRazer продолжит следить за клавиатурой и вернёт подсветку после переподключения.</p>
      <label class="dlg-check"><input v-model="remember" type="checkbox" /> Больше не спрашивать</label>
      <div class="dlg-btns">
        <button @click="emit('choose', 'exit', remember)">Закрыть программу</button>
        <button ref="tray" class="primary" @click="emit('choose', 'tray', remember)">Свернуть в трей</button>
      </div>
    </div>
  </div>
</template>
