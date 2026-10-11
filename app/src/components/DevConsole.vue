<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import { fromConsole, registry } from "../dev";
import { type Parsed, confirmation, hex, isWrite, parse, suggest } from "../devConsole";
import { isError } from "../devTraffic";
import type { LogEntry } from "../types";

const emit = defineEmits<{ open: [seq: number]; close: [] }>();

const RECENT_KEY = "dev.recent";
const RECENT_MAX = 20;

interface Line {
  kind: "in" | "out" | "error" | "warn" | "note";
  text: string;
  seq?: number;
}

function loadRecent(): string[] {
  try {
    const x: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(x) ? x.filter((s): s is string => typeof s === "string") : [];
  } catch {
    return [];
  }
}

const { t } = useI18n();
const line = ref("");
const listOpen = ref(false);
const active = ref(0);
const recent = ref(loadRecent());
const out = ref<Line[]>([]);
const pending = ref<{ p: Parsed; text: string } | null>(null);
const box = ref<HTMLElement | null>(null);
const list = computed(() => suggest(line.value, registry.value, recent.value));
const shown = computed(() => list.value[active.value]);
watch(list, () => (active.value = 0));

function print(...lines: Line[]) {
  out.value.push(...lines);
  void nextTick(() => {
    if (box.value) box.value.scrollTop = box.value.scrollHeight;
  });
}

function complete() {
  const s = shown.value;
  if (!s) return;
  const rest = line.value.trim().split(/\s+/).slice(1);
  line.value = s.recent ? s.text : `${[s.text, ...rest].join(" ")}${rest.length > 0 ? "" : " "}`;
}

async function send(p: Parsed, text: string) {
  recent.value = [text, ...recent.value.filter((r) => r !== text)].slice(0, RECENT_MAX);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent.value));
  } catch {
    // Without storage the history lasts the session.
  }
  line.value = "";
  try {
    const e = await invoke<LogEntry>("dev_exchange", {
      class: Number.parseInt(p.cmd.slice(0, 2), 16),
      id: Number.parseInt(p.cmd.slice(3), 16),
      size: p.size,
      args: p.args,
    });
    fromConsole.add(e.seq);
    print({ kind: isError(e) ? "error" : "out", text: `${e.error ?? e.status ?? "—"} · ${hex(e.reply) || "—"} · ${e.ms} ms`, seq: e.seq });
  } catch (error) {
    print({ kind: "error", text: String(error) });
  }
}

async function submit() {
  listOpen.value = false;
  const text = line.value.trim();
  if (!text) return;
  const p = parse(text, registry.value);
  print({ kind: "in", text });
  if (typeof p === "string") {
    print({ kind: "error", text: p });
    return;
  }
  const ask = confirmation(p, registry.value);
  if (!ask) {
    await send(p, text);
    return;
  }
  pending.value = { p, text };
  let warning: string;
  if (ask.kind === "slot") warning = t("dev.drawer.confirmSlot", { slot: ask.slot });
  else if (ask.kind === "unusual") warning = t("dev.drawer.confirmUnusual", { value: hex([ask.value]), help: ask.help });
  else warning = t(ask.kind === "flash" ? "dev.drawer.confirmFlash" : "dev.drawer.confirmUnknown");
  print({ kind: "warn", text: `⚠ ${warning}` });
}

function cancel() {
  if (!pending.value) return;
  pending.value = null;
  print({ kind: "note", text: t("dev.drawer.cancelled") });
}

function onKey(e: KeyboardEvent) {
  if (pending.value) {
    if (e.repeat) {
      e.preventDefault();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const { p, text } = pending.value;
      pending.value = null;
      void send(p, text);
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
    return;
  }
  const keys: Record<string, () => void> = {
    ArrowDown: () => (active.value = Math.min(active.value + 1, list.value.length - 1)),
    ArrowUp: () => (active.value = Math.max(active.value - 1, 0)),
    Tab: complete,
    Enter: () => {
      void submit();
    },
  };
  if (e.key === "Escape") {
    // An open list takes this Esc; otherwise it reaches the panel and hides the drawer.
    if (listOpen.value) {
      e.preventDefault();
      listOpen.value = false;
    }
    return;
  }
  const f = keys[e.key];
  if (!f) {
    listOpen.value = true;
    return;
  }
  e.preventDefault();
  f();
}

function onBlur() {
  listOpen.value = false;
  cancel();
}

const tone: Record<Line["kind"], string> = {
  in: "text-text",
  out: "text-muted cursor-pointer hover:bg-key",
  error: "text-error",
  warn: "text-warn",
  note: "text-muted",
};
</script>

<template>
  <div class="font-mono flex flex-1 flex-col min-h-0">
    <div class="font-sans px-1 border-b border-0 border-line border-solid flex shrink-0 h-6 whitespace-nowrap items-center">
      <span class="dev-tab dev-tab-on flex items-center">{{ $t("dev.drawer.title") }}</span>
      <span class="flex-1" />
      <button class="dev-btn" :title="$t('dev.drawer.clear')" @click="out = []">⊘</button>
      <button class="dev-btn" :title="$t('dev.drawer.close')" @click="emit('close')">✕</button>
    </div>
    <div ref="box" class="flex-1 min-h-0 select-text overflow-auto">
      <div v-if="out.length === 0" class="text-muted font-sans px-2 py-0.5">{{ $t("dev.drawer.empty") }}</div>
      <div
        v-for="(l, i) in out"
        :key="i"
        class="px-2 py-px border-b border-[#262626] border-0 border-solid flex gap-1.5 break-all"
        :class="tone[l.kind]"
        @click="l.seq !== undefined && emit('open', l.seq)"
      >
        <span class="shrink-0" :class="l.kind === 'in' ? 'text-dev' : 'text-muted'">{{ l.kind === "in" ? "›" : "‹" }}</span>
        <span>{{ l.text }}</span>
      </div>
    </div>
    <div class="px-2 py-1 border-t border-0 border-line border-solid flex shrink-0 gap-1.5 items-center relative">
      <div
        v-if="listOpen && !pending && list.length > 0"
        class="border border-line rounded-t-md border-solid bg-key max-h-[160px] bottom-full left-5 right-1 absolute z-10 overflow-auto"
      >
        <div
          v-for="(s, i) in list"
          :key="`${s.recent ? 'r' : 'c'}${s.text}`"
          class="px-2 py-px flex gap-2 cursor-pointer whitespace-nowrap"
          :class="i === active ? 'bg-line text-text' : 'text-muted'"
          @mousedown.prevent="
            active = i;
            complete();
          "
        >
          <span>{{ s.text }}</span>
          <span>{{ s.info?.cmd ?? $t("dev.recent") }}</span>
          <span v-if="s.info" :class="isWrite(s.info.cmd) ? 'text-write' : 'text-hs'">{{ isWrite(s.info.cmd) ? "W" : "R" }}</span>
          <span class="truncate">{{ s.info?.help ?? "" }}</span>
        </div>
      </div>
      <span class="text-dev">›</span>
      <input
        v-model="line"
        class="text-xs font-mono flex-1 min-w-0 !p-0 !outline-none !border-0 !bg-transparent"
        :readonly="!!pending"
        :placeholder="$t('dev.drawer.placeholder')"
        spellcheck="false"
        autocomplete="off"
        @focus="listOpen = !pending"
        @blur="onBlur"
        @keydown="onKey"
      />
    </div>
    <div
      v-if="listOpen && !pending && shown?.info"
      class="text-muted font-sans px-2 py-0.5 border-t border-0 border-line border-solid shrink-0 truncate"
    >
      <b class="text-text">{{ shown.text }}</b> · {{ shown.info.cmd }} · {{ isWrite(shown.info.cmd) ? "W" : "R" }}
      <template v-if="shown.info.flash != null"> · flash</template> — {{ shown.info.help }}
    </div>
  </div>
</template>
