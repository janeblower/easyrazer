<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";

// `text` replaces the general warning; such a question is asked every time.
const props = defineProps<{ text?: string }>();
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
      <h3 id="cw-title" class="dlg-title">{{ $t("dialogs.write.title") }}</h3>
      <p>{{ props.text ?? $t("dialogs.write.text") }}</p>
      <label v-if="!props.text" class="dlg-check"><input v-model="dontAsk" type="checkbox" /> {{ $t("common.dontAsk") }}</label>
      <div class="dlg-btns">
        <button @click="emit('yes', dontAsk)">{{ $t("dialogs.write.yes") }}</button>
        <button ref="no" class="primary" @click="emit('no')">{{ $t("dialogs.write.no") }}</button>
      </div>
    </div>
  </div>
</template>
