<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { registry } from "../dev";
import { type Parsed, hex, isWrite, parse, suggest } from "../devConsole";
import type { LogEntry } from "../types";
import ModalDialog from "./ModalDialog.vue";

const RECENT_KEY = "dev.recent";
const RECENT_MAX = 20;

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

const line = ref("");
const listOpen = ref(false);
const active = ref(0);
const recent = ref(loadRecent());
const result = ref<LogEntry | string | null>(null);
const confirming = ref<{ p: Parsed; text: string } | null>(null);
const list = computed(() => suggest(line.value, registry.value, recent.value));
const shown = computed(() => list.value[active.value]);
watch(list, () => (active.value = 0));

function complete() {
  const s = shown.value;
  if (!s) return;
  const rest = line.value.trim().split(/\s+/).slice(1);
  line.value = s.recent ? s.text : `${[s.text, ...rest].join(" ")}${rest.length > 0 ? "" : " "}`;
}

async function send(p: Parsed, text: string) {
  confirming.value = null;
  recent.value = [text, ...recent.value.filter((r) => r !== text)].slice(0, RECENT_MAX);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent.value));
  } catch {
    // Without storage the history lasts the session.
  }
  try {
    result.value = await invoke<LogEntry>("dev_exchange", {
      class: Number.parseInt(p.cmd.slice(0, 2), 16),
      id: Number.parseInt(p.cmd.slice(3), 16),
      size: p.size,
      args: p.args,
    });
  } catch (error) {
    result.value = String(error);
  }
}

async function submit() {
  listOpen.value = false;
  const p = parse(line.value, registry.value);
  if (typeof p === "string") result.value = p;
  else if (isWrite(p.cmd)) confirming.value = { p, text: line.value.trim() };
  else await send(p, line.value.trim());
}

function onKey(e: KeyboardEvent) {
  const keys: Record<string, () => void> = {
    ArrowDown: () => (active.value = Math.min(active.value + 1, list.value.length - 1)),
    ArrowUp: () => (active.value = Math.max(active.value - 1, 0)),
    Tab: complete,
    Escape: () => (listOpen.value = false),
    Enter: () => {
      void submit();
    },
  };
  const f = keys[e.key];
  if (!f) {
    listOpen.value = true;
    return;
  }
  e.preventDefault();
  f();
}
</script>

<template>
  <div class="text-xs font-mono p-2 flex flex-col gap-2 min-h-0 overflow-auto">
    <div class="relative">
      <input
        v-model="line"
        class="font-mono w-full"
        spellcheck="false"
        @focus="listOpen = true"
        @blur="listOpen = false"
        @keydown="onKey"
      />
      <div v-if="listOpen && list.length > 0" class="border border-line rounded-b-md bg-key max-h-[200px] overflow-auto">
        <div
          v-for="(s, i) in list"
          :key="`${s.recent ? 'r' : 'c'}${s.text}`"
          class="px-2 py-0.5 gap-2 grid grid-cols-[170px_50px_20px_1fr] cursor-pointer"
          :class="i === active ? 'bg-panel text-text' : 'text-muted'"
          @mousedown.prevent="
            active = i;
            complete();
          "
        >
          <span class="truncate">{{ s.text }}</span>
          <span>{{ s.info?.cmd ?? $t("dev.recent") }}</span>
          <span :class="s.info && isWrite(s.info.cmd) ? 'text-write' : 'text-hs'">{{
            s.info ? (isWrite(s.info.cmd) ? "W" : "R") : ""
          }}</span>
          <span class="truncate">{{ s.info?.help ?? "" }}</span>
        </div>
      </div>
      <p class="hint m-0 mt-1">{{ $t("dev.consoleHint") }}</p>
    </div>
    <div v-if="shown?.info" class="text-muted p-2 rounded-md bg-key">
      <b class="text-text">{{ shown.text }}</b> · {{ shown.info.cmd }} · size {{ shown.info.size ?? "args" }} ·
      {{ isWrite(shown.info.cmd) ? "W" : "R" }}
      <p class="font-sans m-0 my-1">{{ shown.info.help }}</p>
      <div v-for="([name, desc], i) in shown.info.args" :key="i">
        args[{{ i }}] {{ name }} <span class="text-muted">{{ desc }}</span>
      </div>
      <div v-if="shown.info.example">{{ shown.text }} {{ shown.info.example }}</div>
    </div>
    <div v-if="result" :class="typeof result === 'string' ? 'text-error' : 'text-text'">
      <template v-if="typeof result === 'string'">{{ result }}</template>
      <template v-else>← {{ result.error ?? result.status }} · {{ result.ms }} ms · {{ hex(result.reply) }}</template>
    </div>
    <ModalDialog v-if="confirming" :title="$t('dev.confirmTitle')" @cancel="confirming = null">
      <p>{{ $t("dev.confirmText", { cmd: `${confirming.p.cmd} ${hex(confirming.p.args)}` }) }}</p>
      <template #actions>
        <button autofocus @click="confirming = null">{{ $t("profiles.cancel") }}</button>
        <button class="primary" @click="send(confirming.p, confirming.text)">{{ $t("dev.send") }}</button>
      </template>
    </ModalDialog>
  </div>
</template>
