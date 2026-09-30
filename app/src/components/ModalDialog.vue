<script setup lang="ts">
import { onMounted, useId, useTemplateRef } from "vue";

defineProps<{ title: string }>();
const emit = defineEmits<{ cancel: [] }>();
const dialog = useTemplateRef<HTMLDialogElement>("dialog");
const titleId = useId();

// showModal() gives the backdrop, focus trap, Escape and the `autofocus` button for free.
onMounted(() => dialog.value?.showModal());
</script>

<template>
  <dialog ref="dialog" :aria-labelledby="titleId" @cancel.prevent="emit('cancel')" @click.self="emit('cancel')">
    <div class="p-[18px]">
      <h3 :id="titleId" class="dlg-title">{{ title }}</h3>
      <slot />
      <div class="dlg-btns">
        <slot name="actions" />
      </div>
    </div>
  </dialog>
</template>
