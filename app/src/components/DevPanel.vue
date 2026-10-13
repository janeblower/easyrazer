<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { call } from "../keyboard";
import type { DeviceInfo, Dock } from "../types";
import { setLanguage, systemLanguage } from "../i18n";
import { dev, selected, setPanel, startDev } from "../dev";
import { trackDetached } from "../devWindow";
import DevConsole from "./DevConsole.vue";
import DevTraffic from "./DevTraffic.vue";

const props = defineProps<{ dock?: Dock; detached?: boolean }>();

const TABS = ["traffic", "device"] as const;
const DOCKS: Dock[] = ["window", "left", "bottom", "right"];
const GLYPH: Record<Dock, string> = { window: "⧉", left: "◧", bottom: "⬓", right: "◨" };
// Each axis starts from its own size: a side panel's width is far too tall for the bottom.
const DEFAULT_SIZE: Record<Dock, number> = { left: 420, right: 420, bottom: 260, window: 420 };
const MIN_SIZE = 200;
const DRAWER_KEY = "dev.drawer";
const DRAWER_MIN = 80;
const tab = ref<(typeof TABS)[number]>("traffic");
const menu = ref(false);

const { t: translate } = useI18n();
const info = ref<DeviceInfo | null>(null);
const uncensored = ref(false);
const note = ref("");
const h = (n: number | null | undefined, width = 2) => (n == null ? "—" : n.toString(16).toUpperCase().padStart(width, "0"));

function readDrawer() {
  const fallback = { open: true, height: 160 };
  try {
    const d = JSON.parse(localStorage.getItem(DRAWER_KEY) ?? "{}") as Partial<typeof fallback>;
    return {
      open: typeof d.open === "boolean" ? d.open : fallback.open,
      height: typeof d.height === "number" ? d.height : fallback.height,
    };
  } catch {
    return fallback;
  }
}
const drawer = reactive(readDrawer());
watch(drawer, () => {
  try {
    localStorage.setItem(DRAWER_KEY, JSON.stringify(drawer));
  } catch {
    // Without storage the drawer state lasts the session.
  }
});

async function loadInfo() {
  note.value = "";
  try {
    info.value = await call<DeviceInfo | null>("dev_device");
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
    await navigator.clipboard.writeText(await call<string>("dev_diagnostics", { uncensored: uncensored.value }));
    note.value = translate("dev.copied");
  } catch (error) {
    note.value = String(error);
  }
}

const root = ref<HTMLElement | null>(null);

// The console swallows the Esc that closes its own list or confirmation.
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape" || e.defaultPrevented) return;
  if (menu.value) menu.value = false;
  else drawer.open = !drawer.open;
}

onMounted(async () => {
  if (!props.detached) return;
  window.addEventListener("keydown", onKey);
  root.value?.focus();
  await startDev();
  setLanguage(dev.language ?? systemLanguage());
  await trackDetached();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
});

function onAsideKey(e: KeyboardEvent) {
  if (!props.detached) onKey(e);
}

function pickDock(d: Dock) {
  menu.value = false;
  const p = dev.panel;
  if (!p) return;
  const sameAxis = (p.dock === "bottom") === (d === "bottom");
  void setPanel({ dock: d, size: sameAxis ? p.size : DEFAULT_SIZE[d] });
}

function openEntry(seq: number) {
  tab.value = "traffic";
  selected.value = seq;
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
    // The window moves under the pointer on a left or bottom dock, so client coordinates would feed back.
    const delta = props.dock === "bottom" ? e.screenY - down.screenY : e.screenX - down.screenX;
    if (dev.panel) dev.panel.size = Math.max(MIN_SIZE, Math.round(start + sign * delta));
  };
  const up = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    el.removeEventListener("pointercancel", up);
    void setPanel({});
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
}

function dragDrawer(down: PointerEvent) {
  const el = down.currentTarget as HTMLElement;
  el.setPointerCapture(down.pointerId);
  const start = drawer.height;
  const max = (root.value?.clientHeight ?? 400) * 0.7;
  const move = (e: PointerEvent) => {
    drawer.height = Math.round(Math.min(max, Math.max(DRAWER_MIN, start - (e.clientY - down.clientY))));
  };
  const up = () => {
    el.removeEventListener("pointermove", move);
    el.removeEventListener("pointerup", up);
    el.removeEventListener("pointercancel", up);
  };
  el.addEventListener("pointermove", move);
  el.addEventListener("pointerup", up);
  el.addEventListener("pointercancel", up);
}
</script>

<template>
  <aside
    ref="root"
    tabindex="-1"
    class="text-xs text-text outline-none border-dev border-solid bg-panel flex flex-col min-h-0 min-w-0 relative"
    :class="
      detached
        ? 'h-screen border-0'
        : { left: 'border-0 border-r-2', right: 'border-0 border-l-2', bottom: 'border-0 border-t-2', window: 'border-0' }[dock ?? 'window']
    "
    @keydown="onAsideKey"
  >
    <div class="absolute z-10" :class="edge" @pointerdown="drag" />
    <header class="px-1 border-b border-0 border-line border-solid flex shrink-0 h-[30px] min-w-0 whitespace-nowrap items-center">
      <button v-for="t in TABS" :key="t" class="dev-tab" :class="{ 'dev-tab-on': tab === t }" @click="tab = t">
        {{ $t(`dev.tabs.${t}`) }}
      </button>
      <span class="flex-1" />
      <button
        class="dev-btn font-mono"
        :class="{ 'text-dev': drawer.open }"
        :title="$t('dev.drawer.toggle')"
        @click="drawer.open = !drawer.open"
      >
        ›_
      </button>
      <button class="dev-btn" :title="$t('dev.menu.open')" @click="menu = !menu">⋮</button>
      <button class="dev-btn" :title="$t('dev.close')" @click="setPanel({ open: false })">✕</button>
    </header>
    <template v-if="menu">
      <div class="inset-0 fixed z-20" @click="menu = false" @contextmenu.prevent="menu = false" />
      <div class="dev-pop min-w-[220px] right-1 top-[28px]">
        <div class="text-muted px-3 py-1">{{ $t("dev.menu.dock") }}</div>
        <div class="px-3 py-1 flex justify-between">
          <button
            v-for="d in DOCKS"
            :key="d"
            class="text-sm dev-btn"
            :class="{ '!text-dev outline outline-1 outline-dev': dev.panel?.dock === d }"
            :title="$t(`dev.dock.${d}`)"
            :aria-pressed="dev.panel?.dock === d"
            @click="pickDock(d)"
          >
            {{ GLYPH[d] }}
          </button>
        </div>
        <hr class="my-1 border-t border-0 border-line border-solid" />
        <button class="dev-btn text-left w-full !text-text !px-3" @click="copyDiagnostics">⎘ {{ $t("dev.copyDiagnostics") }}</button>
        <label class="px-3 py-1 flex gap-2 items-center" :title="$t('dev.uncensoredHint')">
          <input v-model="uncensored" type="checkbox" /> {{ $t("dev.uncensored") }}
        </label>
        <div v-if="note" class="text-muted px-3 py-1 whitespace-normal">{{ note }}</div>
      </div>
    </template>
    <DevTraffic v-if="tab === 'traffic'" />
    <div v-else class="p-2 flex-1 min-h-0 overflow-auto">
      <p v-if="!info" class="msg">{{ $t("dev.noDevice") }}</p>
      <table v-else class="font-mono">
        <tr v-for="[k, v] in rows" :key="k">
          <td class="text-muted pr-4">{{ $t(`dev.info.${k}`) }}</td>
          <td>{{ v }}</td>
        </tr>
      </table>
      <button class="mt-2" @click="loadInfo">{{ $t("dev.refresh") }}</button>
    </div>
    <div
      v-show="drawer.open"
      class="border-t border-0 border-line border-solid bg-bg shrink-0 h-[5px] cursor-ns-resize"
      @pointerdown="dragDrawer"
    />
    <div v-show="drawer.open" class="flex shrink-0 flex-col max-h-[70%] min-h-0" :style="{ height: `${drawer.height}px` }">
      <DevConsole @open="openEntry" @close="drawer.open = false" />
    </div>
  </aside>
</template>
