<script setup lang="ts">
import { onMounted, ref, useTemplateRef } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { Group, Notices } from "../../notices";
import ModalDialog from "./ModalDialog.vue";

defineProps<{ version: string }>();
const emit = defineEmits<{ close: [] }>();

const licenses = ref<Notices | null>(null);
const error = ref("");
const scroller = useTemplateRef<HTMLElement>("scroller");

onMounted(async () => {
  try {
    const res = await fetch("/licenses.json");
    // The dev server answers unknown paths with index.html.
    if (!res.ok || !res.headers.get("content-type")?.startsWith("application/json")) throw new Error(`licenses.json: ${res.status}`);
    licenses.value = (await res.json()) as Notices;
  } catch (error_) {
    error.value = String(error_);
  }
});

const names = (g: Group) => [...new Set(g.users.flatMap((u) => u.components.map((c) => c.name)))].join(", ");
const holders = (g: Group) =>
  g.users
    .map((u) => [u.components.map((c) => `${c.name} ${c.version}`).join(", "), ...u.holders.map((h) => `  ${h}`)].join("\n"))
    .join("\n");

// Scrolls the list alone; the element's own scrollIntoView would move the dialog too.
function jump(id: string) {
  const target = scroller.value?.querySelector<HTMLElement>(`[data-anchor="${id}"]`);
  if (scroller.value && target) scroller.value.scrollTop = target.offsetTop;
}

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
    <details v-if="licenses" class="text-muted mt-2">
      <summary class="cursor-pointer">{{ $t("about.thirdParty") }}</summary>
      <div ref="scroller" class="text-[11px] mt-2 p-2 rounded bg-bg max-h-[40vh] relative overflow-auto">
        <p v-for="g in licenses.groups" :key="g.id" class="m-0 mb-1.5">
          <a href="#" class="text-accent" @click.prevent="jump(g.id)">{{ g.name }}</a> — {{ names(g) }}
        </p>
        <p v-for="(n, i) in licenses.notices" :key="n.title" class="m-0 mb-1.5">
          <a href="#" class="text-accent" @click.prevent="jump(`notice-${i}`)">{{ $t("about.notice") }}</a> — {{ n.title }}
        </p>
        <section v-for="g in licenses.groups" :key="g.id" :data-anchor="g.id">
          <h4 class="text-text mb-1 mt-4">{{ g.name }}</h4>
          <pre class="m-0 whitespace-pre-wrap">{{ holders(g) }}</pre>
          <pre class="m-0 mt-2 whitespace-pre-wrap">{{ g.text }}</pre>
        </section>
        <section v-for="(n, i) in licenses.notices" :key="n.title" :data-anchor="`notice-${i}`">
          <h4 class="text-text mb-1 mt-4">{{ $t("about.notice") }}: {{ n.title }}</h4>
          <pre class="m-0 whitespace-pre-wrap">{{ n.text }}</pre>
        </section>
      </div>
    </details>
    <p v-if="error" class="text-xs text-error mb-0 mt-2">{{ error }}</p>
    <template #actions>
      <button class="primary" autofocus @click="emit('close')">{{ $t("about.close") }}</button>
    </template>
  </ModalDialog>
</template>
