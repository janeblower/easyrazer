<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import type { AppSettings, Effect, EffectInfo, KeyMap, KeyView, LightingState, Look, Rgb, Status } from "../types";
import ColorPicker from "./ColorPicker.vue";
import KeyboardMap from "./KeyboardMap.vue";
import ConfirmWrite from "./ConfirmWrite.vue";
import ActionBar from "./ActionBar.vue";
import AppIcon from "./AppIcon.vue";

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

const i18n = useI18n();
const { t } = i18n;
// Names the description adds later show as they are until translated.
const label = (section: string, name: string) => (i18n.te(`lighting.${section}.${name}`) ? t(`lighting.${section}.${name}`) : name);
const COLOR_VARIANTS = new Set(["one", "two", "random"]);
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
    if (!g)
      out.push(
        (g = {
          group: p.group,
          label: label("groups", p.group),
          types: [],
          effects: [],
        }),
      );
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
    message.value = t("lighting.applied");
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
    message.value = t("lighting.saved");
  } catch (error) {
    message.value = String(error);
  } finally {
    busy.value = false;
  }
}

async function save() {
  try {
    const settings = await invoke<AppSettings>("app_settings");
    if (settings.confirm_write) {
      asking.value = true;
      return;
    }
  } catch (error) {
    message.value = String(error);
    return;
  }
  await write();
}

async function onConfirm(dontAsk: boolean) {
  asking.value = false;
  if (dontAsk) {
    try {
      await invoke("set_confirm_write", { on: false });
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
      {{ $t("lighting.dlWarning") }}
      <button :disabled="blocked" @click="toggleDynamic">{{ $t("lighting.dlOff") }}</button>
    </div>
    <p v-if="message" class="msg">{{ message }}</p>
    <p v-if="!draft" class="msg">{{ $t("lighting.connect") }}</p>
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
      <div class="px-4 py-3 card flex-1 min-w-0">
        <div v-if="currentGroup?.types.length" class="field">
          <span class="field-label">{{ $t("lighting.type") }}</span>
          <span class="inline-flex">
            <button
              v-for="ty in currentGroup.types"
              :key="ty"
              class="seg-btn"
              :class="{ 'seg-on': ui.type === ty }"
              :disabled="blocked"
              @click="setUi('type', ty)"
            >
              {{ label("types", ty) }}
            </button>
          </span>
        </div>
        <div v-if="slots" class="field">
          <span class="field-label">{{ slots > 1 ? $t("lighting.colors") : $t("lighting.color") }}</span>
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
            :title="$t('lighting.random')"
            :aria-label="$t('lighting.random')"
            :disabled="blocked"
            @click="setUi('random', !ui.random)"
          >
            <AppIcon name="random" />
          </button>
        </div>
        <div v-if="dirInfo" class="field">
          <span class="field-label">{{ $t("lighting.direction") }}</span>
          <span class="inline-flex">
            <button
              v-for="d in dirInfo.dirs"
              :key="d"
              class="seg-btn"
              :class="{ 'seg-on': dirOf === d }"
              :disabled="blocked"
              @click="setUi('dir', d)"
            >
              {{ label("dirs", d) }}
            </button>
          </span>
        </div>
        <div v-if="speedInfo" class="field">
          <span class="field-label">{{ $t("lighting.speed") }}</span>
          <span class="hint">{{ $t("lighting.slow") }}</span>
          <input
            v-model.number="speedSlider"
            class="w-60"
            type="range"
            :min="speedInfo.speed![0]"
            :max="speedInfo.speed![1]"
            step="1"
            :disabled="blocked"
          />
          <span class="hint">{{ $t("lighting.fast") }}</span>
        </div>
        <template v-if="isCustom">
          <KeyboardMap v-model:selection="selection" :layout="layout" :colors="ui.paint" fit />
          <div class="field">
            <span class="field-label">{{ $t("lighting.color") }}</span>
            <ColorPicker :model-value="paintColor" :disabled="blocked || selection.size === 0" @update:model-value="paint" />
            <span class="hint" :title="$t('common.selectHow')">{{
              selection.size > 0 ? $t("common.selected", { n: selection.size }) : $t("common.selectHint")
            }}</span>
            <button @click="selectAll">{{ $t("common.selectAll") }}</button>
            <button :disabled="selection.size === 0" @click="selection = new Set()">{{ $t("common.clearSelection") }}</button>
          </div>
          <p class="text-warn m-0">
            {{ $t("lighting.customWarning") }}
          </p>
        </template>
        <p v-if="ui.group === 'off'" class="msg">{{ $t("lighting.isOff") }}</p>
        <div v-else class="field">
          <span class="field-label">{{ $t("lighting.brightness") }}</span>
          <input v-model.number="percent" class="w-60" type="range" min="0" max="100" step="1" :disabled="blocked" />
          <span>{{ percent }}%</span>
        </div>
      </div>
    </div>
    <div class="px-4 py-3 card">
      <label class="switch">
        <input type="checkbox" role="switch" :checked="dynamicLighting" :disabled="blocked" @change="toggleDynamic" />
        {{ $t("lighting.dynamic") }}
      </label>
    </div>
    <ActionBar
      :pending="dirty ? $t('common.notApplied') : ''"
      :can-revert="dirty && !busy && !blocked"
      :can-apply="dirty && !busy && !blocked"
      :can-write="canSave"
      :apply-only="isCustom"
      @revert="revert"
      @apply="apply"
      @write="save"
    />
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
.g-fire::before {
  background: linear-gradient(0deg, #f20, #f80, #fd0, transparent);
  animation: fx-breathe 1.2s ease-in-out infinite;
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
</style>
