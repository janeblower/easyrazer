<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import type { DeviceInfo, Dock } from "../types";
import { setLanguage, systemLanguage } from "../i18n";
import { dev, setPanel, startDev } from "../dev";
import { trackDetached } from "../devWindow";
import DevConsole from "./DevConsole.vue";
import DevLog from "./DevLog.vue";

const props = defineProps<{ dock?: Dock; detached?: boolean }>();

const TABS = ["log", "console", "device"] as const;
const DOCKS: Dock[] = ["left", "bottom", "right", "window"];
// Each axis starts from its own size: a side panel's width is far too tall for the bottom.
const DEFAULT_SIZE: Record<Dock, number> = { left: 420, right: 420, bottom: 260, window: 420 };
const MIN_SIZE = 200;
const tab = ref<(typeof TABS)[number]>("log");

const { t: translate } = useI18n();
const info = ref<DeviceInfo | null>(null);
const uncensored = ref(false);
const note = ref("");
const h = (n: number | null | undefined, width = 2) => (n == null ? "—" : n.toString(16).toUpperCase().padStart(width, "0"));

async function loadInfo() {
  note.value = "";
  try {
    info.value = await invoke<DeviceInfo | null>("dev_device");
  } catch (error) {
    note.value = String(error);
  }
}

const rows = computed(() => {
  const i = info.value;
  if (!i) return [];
  return [
    ["model", i.model],
    ["ids", `${h(i.vid, 4)}:${h(i.pid, 4)}`],
    ["interface", String(i.interface)],
    ["tid", h(i.tid)],
    ["lighting", `${i.lighting_protocol}, LED ${h(i.led)}`],
    ["lampArray", String(i.lamp_array)],
    ["mode", h(i.mode)],
    ["startup", String(i.startup_slot ?? "—")],
    ["loaded", String(i.loaded_profile ?? "—")],
    ["firmware", i.firmware ?? "—"],
    ["serial", i.serial ?? "—"],
  ] as const;
});

watch(tab, (v) => {
  if (v === "device") void loadInfo();
});

async function copyDiagnostics() {
  try {
    await navigator.clipboard.writeText(await invoke<string>("dev_diagnostics", { uncensored: uncensored.value }));
    note.value = translate("dev.copied");
  } catch (error) {
    note.value = String(error);
  }
}

onMounted(async () => {
  if (!props.detached) return;
  await startDev();
  setLanguage(dev.language ?? systemLanguage());
  await trackDetached();
});

function pickDock(d: Dock) {
  const p = dev.panel;
  if (!p) return;
  const sameAxis = (p.dock === "bottom") === (d === "bottom");
  void setPanel({ dock: d, size: sameAxis ? p.size : DEFAULT_SIZE[d] });
}

const edge = computed(
  () =>
    ({
      left: "right-0 top-0 bottom-0 w-1 cursor-ew-resize",
      right: "left-0 top-0 bottom-0 w-1 cursor-ew-resize",
      bottom: "left-0 right-0 top-0 h-1 cursor-ns-resize",
      window: "hidden",
    })[props.dock ?? "window"],
);

function drag(down: PointerEvent) {
  const p = dev.panel;
  if (!p || !props.dock) return;
  const el = down.currentTarget as HTMLElement;
  el.setPointerCapture(down.pointerId);
  const start = p.size;
  const sign = props.dock === "left" ? 1 : -1;
  const move = (e: PointerEvent) => {
    const delta = props.dock === "bottom" ? e.clientY - down.clientY : e.clientX - down.clientX;
    if (dev.panel) dev.panel.size = Math.max(MIN_SIZE, Math.round(start + sign * delta));
  };
  const up = () => {
    el.removeEventListener("pointermove", move);
    void setPanel({});
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up, { once: true });
}
</script>

<template>
  <aside
    class="text-text border-dev border-solid bg-panel flex flex-col min-h-0 relative"
    :class="
      detached
        ? 'h-screen border-0'
        : { left: 'border-0 border-r-2', right: 'border-0 border-l-2', bottom: 'border-0 border-t-2', window: 'border-0' }[dock ?? 'window']
    "
  >
    <div class="absolute z-10" :class="edge" @pointerdown="drag" />
    <header class="px-2 py-1 border-b border-line flex gap-1 items-center">
      <button v-for="t in TABS" :key="t" class="seg-btn" :class="{ 'seg-on': tab === t }" @click="tab = t">
        {{ $t(`dev.tabs.${t}`) }}
      </button>
      <button class="ml-2" @click="copyDiagnostics">{{ $t("dev.copyDiagnostics") }}</button>
      <label class="text-xs text-muted" :title="$t('dev.uncensoredHint')"
        ><input v-model="uncensored" type="checkbox" /> {{ $t("dev.uncensored") }}</label
      >
      <span class="text-xs text-muted">{{ note }}</span>
      <span class="ml-auto inline-flex">
        <button
          v-for="d in DOCKS"
          :key="d"
          class="px-2 seg-btn"
          :class="dev.panel?.dock === d ? 'text-dev' : 'text-muted'"
          :title="$t(`dev.dock.${d}`)"
          :aria-pressed="dev.panel?.dock === d"
          @click="pickDock(d)"
        >
          {{ { left: "◧", bottom: "⬓", right: "◨", window: "⧉" }[d] }}
        </button>
      </span>
      <button class="text-muted" :title="$t('dev.close')" @click="setPanel({ open: false })">✕</button>
    </header>
    <DevLog v-if="tab === 'log'" />
    <DevConsole v-else-if="tab === 'console'" />
    <div v-else class="text-xs p-2 overflow-auto">
      <p v-if="!info" class="msg">{{ $t("dev.noDevice") }}</p>
      <table v-else class="font-mono">
        <tr v-for="[k, v] in rows" :key="k">
          <td class="text-muted pr-4">{{ $t(`dev.info.${k}`) }}</td>
          <td>{{ v }}</td>
        </tr>
      </table>
      <button class="mt-2" @click="loadInfo">{{ $t("dev.refresh") }}</button>
    </div>
  </aside>
</template>
