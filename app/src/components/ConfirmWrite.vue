<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";

const emit = defineEmits<{ yes: [dontAsk: boolean]; no: [] }>();
const dontAsk = ref(false);
const no = ref<HTMLButtonElement | null>(null);
const onKey = (e: KeyboardEvent) => {
  if (e.key === "Escape") emit("no");
};

onMounted(() => {
  window.addEventListener("keydown", onKey);
  no.value?.focus();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="dlg-backdrop" @click.self="emit('no')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="cw-title">
      <h3 id="cw-title" class="dlg-title">Записать в память клавиатуры?</h3>
      <p>
        Эффект сохранится в клавиатуре и будет работать без EasyRazer. Ресурс перезаписи её флеш-памяти ограничен — не делайте этого слишком
        часто.
      </p>
      <label class="dlg-check"><input v-model="dontAsk" type="checkbox" /> Больше не спрашивать</label>
      <div class="dlg-btns">
        <button @click="emit('yes', dontAsk)">Да, записать</button>
        <button ref="no" class="primary" @click="emit('no')">Нет</button>
      </div>
    </div>
  </div>
</template>
