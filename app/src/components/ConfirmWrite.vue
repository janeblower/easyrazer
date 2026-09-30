<script setup lang="ts">
import { ref } from "vue";
import ModalDialog from "./ModalDialog.vue";

// `text` replaces the general warning; such a question is asked every time.
defineProps<{ text?: string }>();
const emit = defineEmits<{ yes: [dontAsk: boolean]; no: [] }>();
const dontAsk = ref(false);
</script>

<template>
  <ModalDialog :title="$t('dialogs.write.title')" @cancel="emit('no')">
    <p>{{ text ?? $t("dialogs.write.text") }}</p>
    <label v-if="!text" class="dlg-check"><input v-model="dontAsk" type="checkbox" /> {{ $t("common.dontAsk") }}</label>
    <template #actions>
      <button @click="emit('yes', dontAsk)">{{ $t("dialogs.write.yes") }}</button>
      <button class="primary" autofocus @click="emit('no')">{{ $t("dialogs.write.no") }}</button>
    </template>
  </ModalDialog>
</template>
