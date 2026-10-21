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

// The webview would navigate itself to an href, so the browser opens the page instead.
async function open(link: "repo" | "license") {
  await invoke("open_link", { link }).catch((error_: unknown) => (error.value = String(error_)));
}
</script>

<template>
  <ModalDialog class="max-w-[600px]" :title="$t('about.title')" @cancel="emit('close')">
    <p class="mb-1 mt-0"><b>EasyRazer</b> {{ version }}</p>
    <p class="msg">{{ $t("about.description") }}</p>
    <p class="msg mt-2">{{ $t("about.unofficial") }}</p>
    <i18n-t keypath="about.license" tag="p" class="msg mt-2">
      <template #gpl>
        <a href="#" class="text-accent" @click.prevent="open('license')">GNU GPL</a>
      </template>
    </i18n-t>
    <i18n-t keypath="about.source" tag="p" class="msg mt-2">
      <template #github>
        <a href="#" class="text-accent" @click.prevent="open('repo')">GitHub</a>
      </template>
    </i18n-t>
    <details class="text-muted mt-2">
      <summary class="cursor-pointer">{{ $t("about.thirdParty") }}</summary>
      <pre class="text-[11px] m-0 mt-2 p-2 rounded bg-bg max-h-[40vh] whitespace-pre-wrap overflow-auto">{{ licenses }}</pre>
    </details>
    <p v-if="error" class="text-xs text-error mb-0 mt-2">{{ error }}</p>
    <template #actions>
      <button class="primary" autofocus @click="emit('close')">{{ $t("about.close") }}</button>
    </template>
  </ModalDialog>
</template>
