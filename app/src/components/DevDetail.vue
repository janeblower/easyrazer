<script setup lang="ts">
import { computed, ref } from "vue";
import { fromConsole, infoOf, nameOf } from "../dev";
import { hex, isWrite } from "../devConsole";
import { HEADER, argRows, clock, dump, isError } from "../devTraffic";
import type { LogEntry } from "../types";

const props = defineProps<{ entry: LogEntry }>();
defineEmits<{ close: [] }>();

const TABS = ["request", "response", "timing"] as const;
const tab = ref<(typeof TABS)[number]>("request");
const args = computed(() => argRows(infoOf(props.entry.cmd), props.entry.args));
const copy = async () => {
  await navigator.clipboard.writeText(`→ ${hex(props.entry.tx)}\n← ${hex(props.entry.rx)}`);
};
</script>

<template>
  <div class="flex flex-1 flex-col min-h-0 min-w-0">
    <div class="px-1 border-b border-0 border-line border-solid flex shrink-0 h-[26px] whitespace-nowrap items-center overflow-hidden">
      <button class="dev-btn" :title="$t('dev.detail.close')" @click="$emit('close')">✕</button>
      <button v-for="t in TABS" :key="t" class="dev-tab !px-1.5" :class="{ 'dev-tab-on': tab === t }" @click="tab = t">
        {{ $t(`dev.detail.tabs.${t}`) }}
      </button>
      <span class="flex-1" />
      <button class="dev-btn" :title="$t('dev.detail.copy')" @click="copy">⎘</button>
    </div>
    <div class="text-xs leading-relaxed font-mono p-2 select-text break-all overflow-auto">
      <template v-if="tab === 'request'">
        <div>
          {{ nameOf(entry.cmd) ?? entry.cmd }} · {{ entry.cmd }} · {{ isWrite(entry.cmd) ? $t("dev.detail.write") : $t("dev.detail.read")
          }}<template v-if="fromConsole.has(entry.seq)"> · {{ $t("dev.detail.fromConsole") }}</template>
        </div>
        <h4 class="text-[11px] text-muted font-sans mb-0.5 mt-2 uppercase">{{ $t("dev.detail.args") }}</h4>
        <table v-if="args.length > 0">
          <tbody>
            <tr v-for="([name, byte], i) in args" :key="i">
              <td class="text-muted pr-3">{{ name }}</td>
              <td>{{ byte }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else>—</div>
        <h4 class="text-[11px] text-muted font-sans mb-0.5 mt-2 uppercase">{{ $t("dev.detail.bytes") }}</h4>
        <div v-for="row in dump(entry.tx)" :key="row.offset" class="whitespace-pre">
          <span class="text-muted">{{ row.offset }} </span>
          <span v-for="(c, j) in row.cells" :key="j" :class="{ 'text-dev': HEADER.includes(Number.parseInt(row.offset, 16) + j) }">
            {{ c }}</span
          >
        </div>
      </template>
      <template v-else-if="tab === 'response'">
        <div>
          {{ $t("dev.detail.status") }}
          <span :class="isError(entry) ? 'text-error' : 'text-accent'">{{ entry.error ?? entry.status ?? "—" }}</span>
        </div>
        <h4 class="text-[11px] text-muted font-sans mb-0.5 mt-2 uppercase">{{ $t("dev.detail.data") }}</h4>
        <div>{{ hex(entry.reply) || "—" }}</div>
        <h4 class="text-[11px] text-muted font-sans mb-0.5 mt-2 uppercase">{{ $t("dev.detail.bytes") }}</h4>
        <div v-for="row in dump(entry.rx)" :key="row.offset" class="whitespace-pre">
          <span class="text-muted">{{ row.offset }} </span>
          <span v-for="(c, j) in row.cells" :key="j" :class="{ 'text-dev': HEADER.includes(Number.parseInt(row.offset, 16) + j) }">
            {{ c }}</span
          >
        </div>
      </template>
      <table v-else>
        <tbody>
          <tr>
            <td class="text-muted pr-3">{{ $t("dev.detail.sent") }}</td>
            <td>{{ clock(entry.time) }}</td>
          </tr>
          <tr>
            <td class="text-muted pr-3">{{ $t("dev.detail.duration") }}</td>
            <td>{{ entry.ms }}</td>
          </tr>
          <tr>
            <td class="text-muted pr-3">{{ $t("dev.detail.polls") }}</td>
            <td>{{ entry.polls }}</td>
          </tr>
          <tr>
            <td class="text-muted pr-3">{{ $t("dev.detail.background") }}</td>
            <td>{{ entry.background ? $t("dev.detail.yes") : $t("dev.detail.no") }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
