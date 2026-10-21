<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import ModalDialog from "./ModalDialog.vue";

defineProps<{ version: string }>();
const emit = defineEmits<{ close: [] }>();

const licenses = ref("");
const error = ref("");

onMounted(async () => {
  try {
    const res = await fetch("/LICENSES.txt");
    // The dev server answers unknown paths with index.html.
    if (!res.ok || !res.headers.get("content-type")?.startsWith("text/plain")) throw new Error(`LICENSES.txt: ${res.status}`);
    licenses.value = await res.text();
  } catch (error_) {
    error.value = String(error_);
  }
});

async function openRepo() {
  await invoke("open_repo").catch((error_: unknown) => (error.value = String(error_)));
}
</script>

<template>
  <ModalDialog class="max-w-[600px]" :title="$t('about.title')" @cancel="emit('close')">
    <p class="mb-1 mt-0"><b>EasyRazer</b> {{ version }}</p>
    <p class="msg">{{ $t("about.description") }}</p>
    <p class="msg mt-2">{{ $t("about.unofficial") }}</p>
    <p class="msg mt-2">{{ $t("about.license") }}</p>
    <p class="msg mt-2">{{ $t("about.thirdParty") }}</p>
    <pre class="text-[11px] m-0 mt-2 p-2 rounded bg-bg max-h-[40vh] whitespace-pre-wrap overflow-auto">{{ licenses }}</pre>
    <p v-if="error" class="text-xs text-error mb-0 mt-2">{{ error }}</p>
    <template #actions>
      <button @click="openRepo">{{ $t("about.github") }}</button>
      <button class="primary" autofocus @click="emit('close')">{{ $t("about.close") }}</button>
    </template>
  </ModalDialog>
</template>
