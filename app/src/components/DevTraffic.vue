<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from "vue";
import { fromConsole, log, nameOf, selected } from "../dev";
import { hex } from "../devConsole";
import { COLUMNS, type Column, SHOWS, clock, isError, kindOf, matches, parsePrefs, selectedEntry, visibleColumns } from "../devTraffic";
import type { LogEntry } from "../types";
import DevDetail from "./DevDetail.vue";

const PREFS_KEY = "dev.traffic";
// Narrower than this, the details take the whole tab; the type chips fold into a list sooner.
const SPLIT_MIN = 560;
const CHIPS_MIN = 440;

function readPrefs() {
  try {
    return parsePrefs(localStorage.getItem(PREFS_KEY));
  } catch {
    return parsePrefs(null);
  }
}
const prefs = reactive(readPrefs());
watch(
  prefs,
  () => {
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
    } catch {
      // Without storage the view settings last the session.
    }
  },
  { deep: true },
);

const root = ref<HTMLElement | null>(null);
const box = ref<HTMLElement | null>(null);
const width = ref(1000);
let observer: ResizeObserver | undefined;
onMounted(() => {
  observer = new ResizeObserver((entries) => (width.value = entries[0]?.contentRect.width ?? width.value));
  if (root.value) observer.observe(root.value);
});
onBeforeUnmount(() => observer?.disconnect());

const filter = ref("");
const frozen = shallowRef<LogEntry[] | null>(null); // the log as it was when paused
const clearedAt = ref(-1); // last seq hidden by Clear
const popup = ref<null | { kind: "columns" | "show"; x: number; y: number }>(null);
let stick = true; // follows new lines until scrolled up

const rows = computed(() =>
  (frozen.value ?? log.value).filter(
    (e) => e.seq > clearedAt.value && matches(e, prefs.show, prefs.background, filter.value, nameOf(e.cmd)),
  ),
);
const errors = computed(() => log.value.filter((e) => e.seq > clearedAt.value && (prefs.background || !e.background) && isError(e)).length);
const entry = computed(() => selectedEntry(log.value, selected.value, clearedAt.value));
watch(entry, (e, old) => {
  if (!e && old && selected.value === old.seq) selected.value = null;
});
const split = computed(() => width.value >= SPLIT_MIN);
const columns = computed<Column[]>(() => (entry.value ? ["name"] : visibleColumns(prefs.columns, width.value - 12)));

const tone = (e: LogEntry) => ({ r: "text-hs", w: "text-write", e: "text-error" })[kindOf(e)];
function cell(e: LogEntry, c: Column) {
  switch (c) {
    case "time": {
      return clock(e.time);
    }
    case "name": {
      return nameOf(e.cmd) ?? e.cmd;
    }
    case "code": {
      return e.cmd;
    }
    case "args": {
      return hex(e.args) || "—";
    }
    case "status": {
      return e.error ?? e.status ?? "—";
    }
    case "reply": {
      return hex(e.reply) || "—";
    }
    case "ms": {
      return e.polls ? `${e.ms} ·${e.polls}` : String(e.ms);
    }
  }
}

function openPopup(kind: "columns" | "show", ev: MouseEvent) {
  const r = root.value?.getBoundingClientRect();
  const x = ev.clientX - (r?.left ?? 0);
  popup.value = { kind, x: Math.max(0, Math.min(x, width.value - 160)), y: ev.clientY - (r?.top ?? 0) };
  root.value?.focus();
}
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || !popup.value) return;
  e.preventDefault();
  popup.value = null;
}
function toggleColumn(c: Column) {
  prefs.columns = prefs.columns.includes(c)
    ? prefs.columns.filter((x) => x !== c)
    : COLUMNS.filter((x) => x === c || prefs.columns.includes(x));
}
function pick(seq: number) {
  selected.value = selected.value === seq ? null : seq;
}

function onScroll() {
  const el = box.value;
  if (el) stick = el.scrollTop + el.clientHeight >= el.scrollHeight - 4;
}
watch(
  () => rows.value.at(-1)?.seq,
  async () => {
    await nextTick();
    if (stick && box.value) box.value.scrollTop = box.value.scrollHeight;
  },
  { immediate: true },
);
</script>

<template>
  <div ref="root" tabindex="-1" class="outline-none flex flex-1 flex-col min-h-0 min-w-0 relative" @keydown="onKey">
    <div class="px-1 border-b border-0 border-line border-solid flex shrink-0 gap-1 h-7 min-w-0 whitespace-nowrap items-center">
      <button class="dev-btn" :title="$t('dev.traffic.clear')" @click="clearedAt = log.at(-1)?.seq ?? -1">⊘</button>
      <button
        class="dev-btn"
        :class="{ 'text-dev': frozen }"
        :title="frozen ? $t('dev.traffic.resume') : $t('dev.traffic.pause')"
        @click="frozen = frozen ? null : log"
      >
        {{ frozen ? "▶" : "⏸" }}
      </button>
      <input v-model="filter" class="text-xs flex-1 min-w-[50px] !px-1.5 !py-0.5" :placeholder="$t('dev.traffic.filter')" />
      <template v-if="width >= CHIPS_MIN">
        <button
          v-for="s in SHOWS"
          :key="s"
          class="dev-btn !rounded-full"
          :class="{ 'bg-key text-text': prefs.show === s }"
          @click="prefs.show = s"
        >
          {{ $t(`dev.traffic.show.${s}`) }}
        </button>
      </template>
      <button v-else class="dev-btn text-text" @click="openPopup('show', $event)">{{ $t(`dev.traffic.show.${prefs.show}`) }} ▾</button>
      <button
        class="dev-btn"
        :class="{ 'text-dev': prefs.background }"
        :title="prefs.background ? $t('dev.traffic.hideBackground') : $t('dev.traffic.showBackground')"
        @click="prefs.background = !prefs.background"
      >
        ◌
      </button>
      <button v-if="errors > 0" class="dev-btn !text-error" :title="$t('dev.traffic.errors')" @click="prefs.show = 'e'">
        ● {{ errors }}
      </button>
    </div>
    <div class="flex flex-1 min-h-0 min-w-0">
      <div v-show="!entry || split" ref="box" class="min-w-0 overflow-auto" :class="entry ? 'flex-[0.8]' : 'flex-1'" @scroll="onScroll">
        <table class="text-xs font-mono w-full border-collapse table-fixed">
          <thead>
            <tr @contextmenu.prevent="openPopup('columns', $event)">
              <th
                v-for="c in columns"
                :key="c"
                class="text-muted font-normal px-1.5 py-0.5 text-left border-b border-0 border-line border-solid bg-panel whitespace-nowrap top-0 sticky overflow-hidden"
                :class="{ 'w-[150px]': c === 'name' && columns.length > 1 }"
              >
                {{ $t(`dev.columns.${c}`) }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="e in rows"
              :key="e.seq"
              class="cursor-pointer hover:bg-key"
              :class="[selected === e.seq ? 'bg-dev/25' : kindOf(e) === 'e' ? 'bg-error/15' : '', { 'opacity-50': e.background }]"
              @click="pick(e.seq)"
            >
              <td
                v-for="c in columns"
                :key="c"
                class="px-1.5 py-px whitespace-nowrap text-ellipsis overflow-hidden"
                :class="c === 'name' ? tone(e) : ''"
              >
                <span v-if="c === 'name' && fromConsole.has(e.seq)" class="text-dev">› </span>{{ cell(e, c) }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <DevDetail v-if="entry" :entry="entry" :class="split ? 'border-0 border-l border-solid border-line' : ''" @close="selected = null" />
    </div>
    <template v-if="popup">
      <div class="inset-0 fixed z-20" @click="popup = null" @contextmenu.prevent="popup = null" />
      <div class="dev-pop px-3 min-w-[140px]" :style="{ left: `${popup.x}px`, top: `${popup.y}px` }">
        <template v-if="popup.kind === 'columns'">
          <label v-for="c in COLUMNS" :key="c" class="py-0.5 flex gap-2 items-center">
            <input
              type="checkbox"
              :checked="c === 'name' || prefs.columns.includes(c)"
              :disabled="c === 'name'"
              @change="toggleColumn(c)"
            />
            {{ $t(`dev.columns.${c}`) }}
          </label>
        </template>
        <template v-else>
          <div
            v-for="s in SHOWS"
            :key="s"
            class="py-0.5 cursor-pointer"
            :class="{ 'text-dev': prefs.show === s }"
            @click="
              prefs.show = s;
              popup = null;
            "
          >
            {{ $t(`dev.traffic.show.${s}`) }}
          </div>
        </template>
      </div>
    </template>
  </div>
</template>
