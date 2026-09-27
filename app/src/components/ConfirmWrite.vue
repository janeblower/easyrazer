<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";

const emit = defineEmits<{ yes: [dontAsk: boolean]; no: [] }>();
const dontAsk = ref(false);
const no = ref<HTMLButtonElement | null>(null);
const onKey = (e: KeyboardEvent) => e.key === "Escape" && emit("no");

onMounted(() => {
  window.addEventListener("keydown", onKey);
  no.value?.focus();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="backdrop" @click.self="emit('no')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="cw-title">
      <h3 id="cw-title">Записать в память клавиатуры?</h3>
      <p>
        Эффект сохранится в клавиатуре и будет работать без EasyRazer. Ресурс перезаписи её флеш-памяти ограничен — не делайте этого слишком
        часто.
      </p>
      <label><input v-model="dontAsk" type="checkbox" /> Больше не спрашивать</label>
      <div class="btns">
        <button @click="emit('yes', dontAsk)">Да, записать</button>
        <button ref="no" class="primary" @click="emit('no')">Нет</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
}
.dlg {
  background: var(--panel);
  border: 1px solid #3a3a3a;
  border-radius: 10px;
  padding: 18px;
  max-width: 380px;
}
.dlg h3 {
  margin: 0 0 8px;
  font-size: 15px;
}
.dlg label {
  display: block;
  margin-top: 10px;
  color: var(--muted);
}
.btns {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
}
</style>
