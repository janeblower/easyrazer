<script setup lang="ts">
import { ref } from "vue";
import type { CloseAction } from "../types";
import { hasUnapplied } from "../unapplied";
import ModalDialog from "./ModalDialog.vue";

const emit = defineEmits<{ choose: [action: CloseAction, remember: boolean]; cancel: [] }>();
const remember = ref(false);
</script>

<template>
  <ModalDialog :title="$t('dialogs.close.title')" @cancel="emit('cancel')">
    <p>{{ $t("dialogs.close.text") }}</p>
    <p v-if="hasUnapplied()">{{ $t("dialogs.close.unapplied") }}</p>
    <label class="dlg-check"><input v-model="remember" type="checkbox" /> {{ $t("common.dontAsk") }}</label>
    <template #actions>
      <button @click="emit('choose', 'exit', remember)">{{ $t("dialogs.close.exit") }}</button>
      <button class="primary" autofocus @click="emit('choose', 'tray', remember)">{{ $t("dialogs.close.tray") }}</button>
    </template>
  </ModalDialog>
</template>
