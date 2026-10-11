<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { log } from "../dev";
import { hex } from "../devConsole";
import type { DevKey, RawKey } from "../types";
import DevBlock from "./DevBlock.vue";

const props = defineProps<{ keys: number[]; depth: boolean; driver: boolean }>();
const MAX = 8;
const raw = ref<Record<number, DevKey>>({});
const lead = ref<[number, number, boolean]>([0, 0, false]);
let unlisten: UnlistenFn | undefined;

async function load() {
  raw.value = props.keys.length > 0 ? await invoke<Record<number, DevKey>>("dev_keys", { keys: props.keys.slice(0, MAX) }) : {};
}
// A write shows up in the log; the profile it changed is read again.
// The parent passes a fresh array each render: watch its contents, not the array.
watch([() => props.keys.join(","), () => log.value.at(-1)?.seq], load, { immediate: true });

const fmt = (r: RawKey | null) => (r ? `lo ${hex([r.thr_low])} hi ${hex([r.thr_high])} fn ${hex([r.fn_id])} [${hex(r.fn_data)}]` : "—");
const lines = computed(() =>
  Object.entries(raw.value).map(([k, v]) => `#${k} ${v.name}  normal ${fmt(v.normal)}  hs ${fmt(v.hypershift)}`),
);

onMounted(async () => (unlisten = await listen<[number, number, boolean]>("key-depth", (e) => (lead.value = e.payload))));
onUnmounted(() => unlisten?.());
</script>

<template>
  <DevBlock v-if="keys.length > 0 || depth">
    <div v-for="l in lines" :key="l">{{ l }}</div>
    <div v-if="keys.length > MAX">{{ $t("dev.more", { n: keys.length - MAX }) }}</div>
    <div v-if="depth">{{ driver ? `depth ${lead[0]} travel ${lead[1]} ${lead[2] ? "down" : "up"}` : $t("dev.needDriver") }}</div>
  </DevBlock>
</template>
