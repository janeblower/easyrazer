<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { log, nameOf } from "../dev";
import { hex, isWrite } from "../devConsole";
import type { LogEntry } from "../types";

const filter = ref("");
const hideBackground = ref(true);
const frozen = ref<LogEntry[] | null>(null); // the log as it was when paused
const clearedAt = ref(-1); // last seq hidden by Clear
const open = ref<number | null>(null);
const box = ref<HTMLElement | null>(null);
let stick = true; // follows new lines until scrolled up

const rows = computed(() => {
  const q = filter.value.trim().toLowerCase();
  return (frozen.value ?? log.value).filter(
    (e) =>
      e.seq > clearedAt.value &&
      !(hideBackground.value && e.background) &&
      (!q || `${e.cmd} ${nameOf(e.cmd) ?? ""} ${hex(e.args)} ${hex(e.reply)}`.toLowerCase().includes(q)),
  );
});

const time = (ms: number) => `${new Date(ms).toLocaleTimeString(undefined, { hour12: false })}.${String(ms % 1000).padStart(3, "0")}`;
const tone = (e: LogEntry) => (e.error || (e.status && e.status !== "Ok") ? "text-error" : isWrite(e.cmd) ? "text-write" : "text-hs");
const copy = async (e: LogEntry) => {
  await navigator.clipboard.writeText(`→ ${hex(e.tx)}\n← ${hex(e.rx)}`);
};

function onScroll() {
  const el = box.value;
  if (el) stick = el.scrollTop + el.clientHeight >= el.scrollHeight - 4;
}

watch(
  () => rows.value.length,
  async () => {
    await nextTick();
    if (stick && box.value) box.value.scrollTop = box.value.scrollHeight;
  },
  { immediate: true },
);
</script>

<template>
  <div class="flex flex-1 flex-col min-h-0">
    <div class="p-2 border-b border-line flex gap-2 items-center">
      <input v-model="filter" class="flex-1 min-w-0" :placeholder="$t('dev.filter')" />
      <label class="text-xs text-muted whitespace-nowrap"
        ><input v-model="hideBackground" type="checkbox" /> {{ $t("dev.hideBackground") }}</label
      >
      <button @click="frozen = frozen ? null : [...log]">{{ frozen ? $t("dev.resume") : $t("dev.pause") }}</button>
      <button @click="clearedAt = log.at(-1)?.seq ?? -1">{{ $t("dev.clear") }}</button>
    </div>
    <div ref="box" class="text-xs font-mono p-2 flex-1 overflow-auto" @scroll="onScroll">
      <div v-for="e in rows" :key="e.seq" class="cursor-pointer" @click="open = open === e.seq ? null : e.seq">
        <span :class="tone(e)">
          {{ time(e.time) }} → {{ nameOf(e.cmd) ?? "" }} {{ e.cmd }} [{{ e.size }}] {{ hex(e.args) }} ← {{ e.error ?? e.status ?? "—" }}
          {{ hex(e.reply) }} · {{ e.ms }} ms<template v-if="e.polls"> · busy×{{ e.polls }}</template>
        </span>
        <div v-if="open === e.seq" class="text-muted pl-4 break-all" @click.stop>
          → {{ hex(e.tx) }}<br />← {{ hex(e.rx) }}
          <button class="ml-2" @click="copy(e)">{{ $t("dev.copy") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>
