<script setup lang="ts">
import ModalDialog from "./ModalDialog.vue";

const emit = defineEmits<{ answer: [on: boolean]; later: [] }>();
</script>

<template>
  <ModalDialog :title="$t('dialogs.autostart.title')" @cancel="emit('later')">
    <p>{{ $t("dialogs.autostart.text") }}</p>
    <template #actions>
      <button @click="emit('answer', false)">{{ $t("dialogs.autostart.no") }}</button>
      <button class="primary" autofocus @click="emit('answer', true)">{{ $t("dialogs.autostart.yes") }}</button>
    </template>
  </ModalDialog>
</template>
