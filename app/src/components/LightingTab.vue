<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { Effect, EffectInfo, KeyMap, KeyView, LightingState, Look, Rgb, Status } from "../types";
import ColorPicker from "./ColorPicker.vue";
import KeyboardMap from "./KeyboardMap.vue";
import ConfirmWrite from "./ConfirmWrite.vue";

interface Ui {
  group: string;
  type: string | null;
  colors: (Rgb | null)[];
  random: boolean;
  dir: string | null;
  speed: number | null;
  brightness: number;
  paint: KeyMap<Rgb>;
}
type Variant = "one" | "two" | "random";
interface Parsed {
  group: string;
  type: string | null;
  variant: Variant | null;
}
type Candidate = EffectInfo & Parsed;
interface Group {
  group: string;
  label: string;
  types: string[];
  effects: Candidate[];
}

const props = defineProps<{ status: Status | null }>();

const GROUPS: Record<string, string> = {
  off: "Выключено",
  static: "Статичный",
  breathing: "Дыхание",
  spectrum: "Спектр",
  wave: "Волна",
  reactive: "Отклик",
  starlight: "Звёздное небо",
  custom: "Своя раскладка",
};
const TYPES: Record<string, string> = { key: "Клавиша", ripple: "Рябь" };
const COLOR_VARIANTS = new Set(["one", "two", "random"]);
const DIRS: Record<string, string> = { left: "← влево", right: "вправо →" };
const DEFAULT_COLORS: Rgb[] = [
  [0x44, 0xd6, 0x2c],
  [0x00, 0x40, 0xff],
];
const WHITE: Rgb = [0xff, 0xff, 0xff];
const BLACK: Rgb = [0, 0, 0];

const effects = ref<EffectInfo[]>([]);
const applied = ref<Look | null>(null);
const saved = ref<Look | null>(null);
// What the controls show; the effect sent to the keyboard is derived from it.
const ui = ref<Ui | null>(null);
const dynamicLighting = ref(false);
const confirmWrite = ref(true);
const asking = ref(false);
const busy = ref(false);
const message = ref("");
const layout = ref<KeyView[]>([]);
// Last applied custom layout, offered again when the effect is picked.
const custom = ref<KeyMap<Rgb> | null>(null);
const selection = ref(new Set<number>());
let loading = false;

// Effect names are `group[_type][_one|_two|_random]`.
function parse(name: string): Parsed {
  const parts = name.split("_");
  const variant = COLOR_VARIANTS.has(parts.at(-1)!) ? (parts.pop() as Variant) : null;
  return { group: parts[0], type: parts[1] ?? null, variant };
}

const blocked = computed(() => !props.status || props.status.synapse);
const connected = computed(() => !!props.status?.device && !blocked.value);
const groups = computed(() => {
  const out: Group[] = [];
  for (const e of effects.value) {
    const p = parse(e.name);
    let g = out.find((g) => g.group === p.group);
    if (!g) out.push((g = { group: p.group, label: GROUPS[p.group] ?? p.group, types: [], effects: [] }));
    if (p.type && !g.types.includes(p.type)) g.types.push(p.type);
    g.effects.push({ ...e, ...p });
  }
  return out;
});

function candidates(s: Ui): Candidate[] {
  return groups.value.find((g) => g.group === s.group)?.effects.filter((e) => e.type === s.type) ?? [];
}

const currentGroup = computed(() => groups.value.find((g) => g.group === ui.value?.group));
const cands = computed(() => (ui.value ? candidates(ui.value) : []));
const slots = computed(() => Math.max(0, ...cands.value.map((e) => e.colors)));
const hasRandom = computed(() => cands.value.some((e) => e.variant === "random"));
const randomOn = computed(() => !!ui.value?.random && hasRandom.value);

// One set color picks the one-color effect, two pick the two-color one, none turn the lighting off.
function resolve(s: Ui): Look | null {
  const cs = candidates(s);
  if (cs.length === 0) return null;
  if (s.group === "custom") return { effect: { name: "custom", colors: s.paint }, brightness: s.brightness };
  const set = s.colors.slice(0, Math.max(0, ...cs.map((e) => e.colors))).filter((c): c is Rgb => !!c);
  let e: Candidate = cs[0];
  const random = cs.find((c) => c.variant === "random");
  if (s.random && random) e = random;
  else if (cs.some((c) => c.colors)) {
    if (set.length === 0) return { effect: { name: "off" }, brightness: s.brightness };
    e = cs.find((c) => c.colors === set.length) ?? cs.find((c) => c.colors === 1) ?? e;
  }
  const effect: Effect = { name: e.name };
  if (e.colors >= 1) effect.rgb1 = set[0];
  if (e.colors >= 2) effect.rgb2 = set[1];
  if (e.dirs.length > 0) effect.dir = s.dir && e.dirs.includes(s.dir) ? s.dir : e.dirs[0];
  if (e.speed) {
    const [lo, hi] = e.speed;
    effect.speed = s.speed != null && s.speed >= lo && s.speed <= hi ? s.speed : Math.round((lo + hi) / 2);
  }
  return { effect, brightness: s.brightness };
}

function blank(): KeyMap<Rgb> {
  return Object.fromEntries(layout.value.map((k) => [k.key, WHITE]));
}

function stateFrom(stored: Look | null): Ui | null {
  const look = stored && effects.value.some((e) => e.name === stored.effect.name) ? stored : null;
  const paint = look?.effect.colors ?? custom.value ?? blank();
  const s: Ui = { group: "", type: null, colors: [...DEFAULT_COLORS], random: false, dir: null, speed: null, brightness: 255, paint };
  const name = look?.effect.name ?? effects.value[0]?.name;
  if (!name) return null;
  const p = parse(name);
  Object.assign(s, { group: p.group, type: p.type, random: p.variant === "random" });
  if (look) {
    const e = look.effect;
    if (e.rgb1) s.colors = [e.rgb1, e.rgb2 ?? (p.variant === "one" ? null : DEFAULT_COLORS[1])];
    Object.assign(s, { dir: e.dir ?? null, speed: e.speed ?? null, brightness: look.brightness });
  }
  return s;
}

// The backend omits unset effect fields; compare looks by the fields that are set.
function norm(look?: Look | null): Look | null {
  if (!look) return null;
  const { name, rgb1, rgb2, dir, speed, colors } = look.effect;
  const effect: Effect = { name };
  if (colors != null) effect.colors = colors;
  if (rgb1 != null) effect.rgb1 = rgb1;
  if (rgb2 != null) effect.rgb2 = rgb2;
  if (dir != null) effect.dir = dir;
  if (speed != null) effect.speed = speed;
  return { effect, brightness: look.brightness };
}

function same(a?: Look | null, b?: Look | null) {
  return JSON.stringify(norm(a)) === JSON.stringify(norm(b));
}

const draft = computed(() => ui.value && resolve(ui.value));
const dirty = computed(() => !!draft.value && !same(draft.value, applied.value ?? saved.value));
const isCustom = computed(() => ui.value?.group === "custom");
const canSave = computed(() => connected.value && !busy.value && !!draft.value && !isCustom.value && !same(draft.value, saved.value));

function setUi<K extends keyof Ui>(k: K, v: Ui[K]) {
  ui.value = { ...ui.value!, [k]: v };
}

// The color all selected keys share, or none when they differ.
const paintColor = computed(() => {
  const cs = Array.from(selection.value, (k) => ui.value?.paint[k] ?? BLACK);
  return cs.length > 0 && cs.every((c) => c.join(",") === cs[0].join(",")) ? cs[0] : null;
});

function paint(rgb: Rgb | null) {
  const next = { ...ui.value!.paint };
  for (const k of selection.value) next[k] = rgb ?? BLACK;
  setUi("paint", next);
}

function selectAll() {
  selection.value = new Set(layout.value.map((k) => k.key));
}

function pickGroup(g: Group) {
  setUi("group", g.group);
  const type = ui.value!.type;
  setUi("type", type && g.types.includes(type) ? type : (g.types[0] ?? null));
}

function setColor(i: number, rgb: Rgb | null) {
  const colors = [...ui.value!.colors];
  colors[i] = rgb;
  setUi("colors", colors);
}

const percent = computed({
  get: () => Math.round((ui.value!.brightness / 255) * 100),
  set: (p: number) => {
    setUi("brightness", Math.round((p * 255) / 100));
  },
});

// Taken from the group, not the resolved effect: with no colors the effect is "off" but the controls stay.
const dirInfo = computed(() => cands.value.find((e) => e.dirs.length));
const dirOf = computed(() => {
  const dirs = dirInfo.value?.dirs ?? [];
  const dir = ui.value?.dir;
  return dir && dirs.includes(dir) ? dir : dirs[0];
});
const speedInfo = computed(() => cands.value.find((e) => e.speed));

// Some firmware speeds are "lower is faster"; the slider always reads slow → fast.
const speedSlider = computed({
  get: () => {
    const [lo, hi] = speedInfo.value!.speed!;
    const fastLow = speedInfo.value!.fast_low;
    const cur = ui.value!.speed;
    const s = cur != null && cur >= lo && cur <= hi ? cur : Math.round((lo + hi) / 2);
    return fastLow ? lo + hi - s : s;
  },
  set: (v: number) => {
    const [lo, hi] = speedInfo.value!.speed!;
    const fastLow = speedInfo.value!.fast_low;
    setUi("speed", fastLow ? lo + hi - v : v);
  },
});

let pending: Look | null = null;
let sending = false;

// One preview in flight, at most one per 50 ms; the newest draft wins. Parallel calls
// would race for the device lock and could land out of order.
async function preview() {
  pending = norm(draft.value);
  if (sending) return;
  sending = true;
  while (pending) {
    await new Promise((done) => setTimeout(done, 50));
    const look = pending;
    pending = null;
    try {
      await invoke("lighting_preview", { look });
    } catch (error) {
      message.value = String(error);
    }
  }
  sending = false;
}

watch(draft, (now, before) => {
  if (!loading && connected.value && now && !same(now, before)) void preview();
});

async function load() {
  try {
    if (layout.value.length === 0) layout.value = await invoke<KeyView[]>("lighting_layout");
    const s = await invoke<LightingState>("lighting_state");
    effects.value = s.effects;
    custom.value = s.custom;
    applied.value = s.applied;
    saved.value = s.saved;
    dynamicLighting.value = s.dynamic_lighting;
    confirmWrite.value = s.confirm_write;
    loading = true;
    ui.value = stateFrom(s.applied ?? s.saved);
    await nextTick();
    loading = false;
    message.value = "";
  } catch (error) {
    message.value = String(error);
  }
}

function revert() {
  ui.value = stateFrom(applied.value ?? saved.value);
}

async function apply() {
  busy.value = true;
  try {
    const look = norm(draft.value);
    await invoke("lighting_apply", { look });
    applied.value = look;
    if (look?.effect.colors) custom.value = look.effect.colors;
    message.value = "Применено";
  } catch (error) {
    message.value = String(error);
  } finally {
    busy.value = false;
  }
}

async function write() {
  busy.value = true;
  try {
    const look = norm(draft.value);
    await invoke("lighting_write", { look });
    applied.value = look;
    saved.value = look;
    message.value = "Записано в память клавиатуры";
  } catch (error) {
    message.value = String(error);
  } finally {
    busy.value = false;
  }
}

async function save() {
  if (confirmWrite.value) asking.value = true;
  else await write();
}

async function onConfirm(dontAsk: boolean) {
  asking.value = false;
  if (dontAsk) {
    try {
      await invoke("set_confirm_write", { on: false });
      confirmWrite.value = false;
    } catch (error) {
      message.value = String(error);
    }
  }
  await write();
}

async function toggleDynamic() {
  const on = !dynamicLighting.value;
  try {
    await invoke("set_dynamic_lighting", { on });
    dynamicLighting.value = on;
  } catch (error) {
    message.value = String(error);
  }
}

watch(connected, async (ok) => ok && load());
onMounted(load);
</script>

<template>
  <section class="flex flex-col gap-3">
    <div v-if="dynamicLighting" class="text-warn px-3 py-2 rounded-md bg-warn-bg flex gap-3 items-center">
      ⚠ Подсветкой управляет динамическое освещение Windows — эффекты клавиатуры не видны.
      <button :disabled="blocked" @click="toggleDynamic">Отключить</button>
    </div>
    <p v-if="message" class="msg">{{ message }}</p>
    <p v-if="!draft" class="msg">Подключите клавиатуру, чтобы увидеть её эффекты.</p>
    <div v-else-if="ui" class="flex gap-3" :class="{ 'pointer-events-none opacity-40': blocked }">
      <ul class="m-0 p-2.5 list-none card w-[210px]">
        <li
          v-for="g in groups"
          :key="g.group"
          :class="[`fx g-${g.group}`, { on: currentGroup === g }]"
          class="mb-1 px-2.5 py-2 border-2 rounded-md border-solid bg-key cursor-pointer relative overflow-hidden"
          @click="!blocked && pickGroup(g)"
        >
          <span>{{ g.label }}</span>
        </li>
      </ul>
      <div class="px-3.5 py-2.5 card flex-1">
        <div v-if="currentGroup?.types.length" class="field">
          <span class="field-label">Тип</span>
          <span class="inline-flex">
            <button
              v-for="t in currentGroup.types"
              :key="t"
              class="seg-btn"
              :class="{ 'seg-on': ui.type === t }"
              :disabled="blocked"
              @click="setUi('type', t)"
            >
              {{ TYPES[t] ?? t }}
            </button>
          </span>
        </div>
        <div v-if="slots" class="field">
          <span class="field-label">{{ slots > 1 ? "Цвета" : "Цвет" }}</span>
          <ColorPicker
            v-for="i in slots"
            :key="i"
            :model-value="ui.colors[i - 1]"
            :disabled="blocked || randomOn"
            @update:model-value="setColor(i - 1, $event)"
          />
          <button
            v-if="hasRandom"
            class="ml-1.5 icon-btn"
            :class="{ 'border-accent text-accent': ui.random }"
            :aria-pressed="ui.random"
            title="Случайные цвета"
            aria-label="Случайные цвета"
            :disabled="blocked"
            @click="setUi('random', !ui.random)"
          >
            <svg
              viewBox="0 0 24 24"
              width="18"
              height="18"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M2 18h1.4c1.3 0 2.5-.6 3.3-1.7l6.1-8.6c.7-1.1 2-1.7 3.3-1.7H22" />
              <path d="m18 2 4 4-4 4" />
              <path d="M2 6h1.9c1.5 0 2.9.9 3.6 2.2" />
              <path d="M22 18h-5.9c-1.3 0-2.6-.7-3.3-1.8l-.5-.8" />
              <path d="m18 14 4 4-4 4" />
            </svg>
          </button>
        </div>
        <div v-if="dirInfo" class="field">
          <span class="field-label">Направление</span>
          <span class="inline-flex">
            <button
              v-for="d in dirInfo.dirs"
              :key="d"
              class="seg-btn"
              :class="{ 'seg-on': dirOf === d }"
              :disabled="blocked"
              @click="setUi('dir', d)"
            >
              {{ DIRS[d] ?? d }}
            </button>
          </span>
        </div>
        <div v-if="speedInfo" class="field">
          <span class="field-label">Скорость</span>
          <span class="hint">медленно</span>
          <input
            v-model.number="speedSlider"
            class="w-60"
            type="range"
            :min="speedInfo.speed![0]"
            :max="speedInfo.speed![1]"
            step="1"
            :disabled="blocked"
          />
          <span class="hint">быстро</span>
        </div>
        <template v-if="isCustom">
          <div class="field">
            <span class="field-label">Цвет</span>
            <ColorPicker :model-value="paintColor" :disabled="blocked || selection.size === 0" @update:model-value="paint" />
            <span class="hint">{{ selection.size > 0 ? `Выделено: ${selection.size}` : "Выделите клавиши: клик, Ctrl+клик, рамка" }}</span>
            <button @click="selectAll">Выделить все</button>
            <button :disabled="selection.size === 0" @click="selection = new Set()">Снять выделение</button>
          </div>
          <p class="text-warn m-0">
            ⚠ Своя раскладка не записывается в память клавиатуры: она работает, пока запущен EasyRazer (в том числе свёрнутый в трей).
          </p>
        </template>
        <p v-if="ui.group === 'off'" class="msg">Подсветка выключена.</p>
        <div v-else class="field">
          <span class="field-label">Яркость</span>
          <input v-model.number="percent" class="w-60" type="range" min="0" max="100" step="1" :disabled="blocked" />
          <span>{{ percent }}%</span>
        </div>
      </div>
    </div>
    <div v-if="isCustom && ui" class="p-3 card overflow-x-auto" :class="{ 'pointer-events-none opacity-40': blocked }">
      <KeyboardMap v-model:selection="selection" :layout="layout" :colors="ui.paint" />
    </div>
    <div class="px-4 py-3 card flex gap-2 items-center">
      <label class="switch">
        <input type="checkbox" role="switch" :checked="dynamicLighting" :disabled="blocked" @change="toggleDynamic" />
        Динамическое освещение Windows
      </label>
      <span class="flex-1"></span>
      <span v-if="dirty" class="text-xs text-edited">● не применено</span>
      <button
        class="icon-btn"
        title="Отменить изменения"
        aria-label="Отменить изменения"
        :disabled="!dirty || busy || blocked"
        @click="revert"
      >
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M9 14 4 9l5-5" />
          <path d="M4 9h11a5 5 0 0 1 0 10h-3" />
        </svg>
      </button>
      <button class="primary" :disabled="!dirty || busy || blocked" @click="apply">Применить</button>
      <button
        v-if="!isCustom"
        class="text-[#ffb070] icon-btn border-[#8a5a20] bg-transparent"
        title="Записать в память клавиатуры"
        aria-label="Записать в память клавиатуры"
        :disabled="!canSave"
        @click="save"
      >
        <svg
          viewBox="0 0 24 24"
          width="18"
          height="18"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" />
          <path d="M7 3v5h8V3" />
          <path d="M7 21v-7h10v7" />
        </svg>
      </button>
    </div>
    <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
  </section>
</template>

<style scoped>
.fx {
  border-color: transparent;
}
.fx.on {
  border-color: var(--accent);
}
.fx > span {
  position: relative;
  z-index: 1;
}
.fx::before,
.fx::after {
  content: "";
  position: absolute;
  inset: 0;
  opacity: 0;
  transition: opacity 0.2s;
  pointer-events: none;
}
.fx:hover::before,
.fx.on::before,
.fx:hover::after,
.fx.on::after {
  opacity: 0.45;
}
.g-custom::before {
  background: linear-gradient(90deg, #f00 0 25%, #0f0 25% 50%, #00f 50% 75%, #ff0 75%);
}
.g-static::before {
  background: var(--accent);
}
.g-breathing::before {
  animation: fx-breathe 3s ease-in-out infinite;
}
.g-spectrum::before {
  background: #ff0000;
  animation: fx-hue 4s linear infinite;
}
.g-wave::before {
  background: linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
  background-size: 200% 100%;
  animation: fx-wave 3s linear infinite;
}
.g-reactive::before {
  background: radial-gradient(
    circle at 20% 50%,
    transparent var(--r),
    var(--accent) calc(var(--r) + 3px),
    transparent calc(var(--r) + 14px)
  );
  animation: fx-ripple 1.6s ease-out infinite;
}
.g-starlight::before,
.g-starlight::after {
  animation: fx-twinkle 1.8s ease-in-out infinite;
}
.g-starlight::before {
  background:
    radial-gradient(circle at 12% 30%, rgb(255 255 255 / var(--a)) 0 2px, transparent 3px),
    radial-gradient(circle at 48% 70%, rgb(68 214 44 / var(--a)) 0 2px, transparent 3px),
    radial-gradient(circle at 80% 35%, rgb(255 255 255 / var(--a)) 0 2px, transparent 3px);
}
.g-starlight::after {
  animation-delay: -0.9s;
  background:
    radial-gradient(circle at 30% 60%, rgb(0 128 255 / var(--a)) 0 2px, transparent 3px),
    radial-gradient(circle at 66% 25%, rgb(255 255 255 / var(--a)) 0 2px, transparent 3px),
    radial-gradient(circle at 92% 70%, rgb(68 214 44 / var(--a)) 0 2px, transparent 3px);
}
@property --r {
  syntax: "<length>";
  inherits: false;
  initial-value: 0px;
}
@property --a {
  syntax: "<number>";
  inherits: false;
  initial-value: 0;
}
@keyframes fx-breathe {
  0%,
  100% {
    background-color: transparent;
  }
  50% {
    background-color: var(--accent);
  }
}
@keyframes fx-hue {
  to {
    filter: hue-rotate(360deg);
  }
}
@keyframes fx-wave {
  to {
    background-position: -100% 0;
  }
}
@keyframes fx-ripple {
  from {
    --r: 0px;
  }
  to {
    --r: 220px;
  }
}
@keyframes fx-twinkle {
  0%,
  100% {
    --a: 0;
  }
  50% {
    --a: 1;
  }
}
@media (prefers-reduced-motion: reduce) {
  .fx::before,
  .fx::after {
    animation: none;
  }
}
.switch {
  display: flex;
  align-items: center;
  gap: 8px;
}
.switch input {
  appearance: none;
  width: 34px;
  height: 18px;
  border-radius: 9px;
  background: #3a3a3a;
  position: relative;
  cursor: pointer;
}
.switch input::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #fff;
  transition: left 0.15s;
}
.switch input:checked {
  background: var(--accent);
}
.switch input:checked::after {
  left: 18px;
}
</style>
