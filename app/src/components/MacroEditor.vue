<script setup lang="ts">
import { computed, onActivated, onDeactivated, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { call } from "../keyboard";
import type { KeyView, MacroEvent, MacroState, Mouse } from "../types";
import { MOUSE, bodySize, codeName, footprint, keyName } from "../bindings";
import { dev } from "../dev";
import { hex } from "../devConsole";
import { useSection } from "../changes";
import DevBlock from "./DevBlock.vue";
import EmptyState from "./EmptyState.vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  state: MacroState | null;
  layout: KeyView[];
  /** Names of the keys each macro is bound to. */
  bound: Map<number, string[]>;
  writable: boolean;
}>();
const emit = defineEmits<{ update: [state: MacroState] }>();
const { t } = useI18n();

interface Draft {
  name: string;
  events: MacroEvent[];
}

// null while a new macro is not applied yet.
const selected = ref<number | null>(null);
const bytes = ref<number[] | null>(null);
watch(
  () => [dev.mode, selected.value, props.state] as const,
  async ([on, id]) => {
    bytes.value = on && id != null ? await call<number[] | null>("dev_macro", { macroId: id }) : null;
  },
  { immediate: true },
);
const draft = ref<Draft | null>(null);
const busy = ref(false);
const message = ref("");
const armed = ref(false); // the delete button asks for a second click
const recording = ref(false);
const withDelays = ref(true);
let last = 0;

const list = computed(() => Object.entries(props.state?.macros ?? {}).map(([id, m]) => ({ id: Number(id), ...m })));
const saved = computed(() => (selected.value == null ? null : (props.state?.macros[selected.value] ?? null)));
const dirty = computed(
  () =>
    !!draft.value &&
    (!saved.value || saved.value.name !== draft.value.name || JSON.stringify(saved.value.events) !== JSON.stringify(draft.value.events)),
);
const keys = computed(() => props.layout.filter((k) => k.editable).map((k) => ({ key: k.key, name: keyName(k, t) })));
const byName = computed(() => new Map(props.layout.map((k) => [k.name, k.key])));
const size = computed(() => footprint(bodySize(draft.value?.events ?? [])));
const boundTo = computed(() => (selected.value == null ? [] : (props.bound.get(selected.value) ?? [])));

watch(selected, () => (armed.value = false));

function add(...events: MacroEvent[]) {
  draft.value?.events.push(...events);
}

function onKey(e: KeyboardEvent) {
  e.preventDefault();
  e.stopPropagation();
  const key = byName.value.get(codeName(e.code));
  if (e.repeat || key == null || !draft.value) return;
  const now = performance.now();
  if (withDelays.value && last && draft.value.events.length > 0) add({ type: "delay", ms: Math.round(now - last) });
  last = now;
  add({ type: "key", key, down: e.type === "keydown" });
}

function startRecording() {
  recording.value = true;
  last = 0;
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("keyup", onKey, true);
}

function stopRecording() {
  recording.value = false;
  window.removeEventListener("keydown", onKey, true);
  window.removeEventListener("keyup", onKey, true);
}

function open(id: number | null) {
  stopRecording();
  selected.value = id;
  const m = id == null ? null : props.state?.macros[id];
  draft.value = { name: m?.name ?? t("macros.newName", { n: list.value.length + 1 }), events: (m?.events ?? []).map((e) => ({ ...e })) };
  message.value = "";
}

function recordNew() {
  open(null);
  startRecording();
}

function move(i: number, by: number) {
  const ev = draft.value?.events;
  if (!ev || i + by < 0 || i + by >= ev.length) return;
  [ev[i], ev[i + by]] = [ev[i + by], ev[i]];
}

// Kept alive behind the other tabs: recording must not swallow keys typed there.
const active = ref(true);
onActivated(() => (active.value = true));
onDeactivated(() => {
  active.value = false;
  stopRecording();
});
onUnmounted(stopRecording);

// `state` picks the macro list out of the reply.
async function run<T>(command: string, args: Record<string, unknown>, done: string, state: (r: T) => MacroState) {
  busy.value = true;
  try {
    const r = await call<T>(command, args);
    emit("update", state(r));
    message.value = t(done);
    return r;
  } catch (error) {
    message.value = String(error);
    return;
  } finally {
    busy.value = false;
  }
}

async function apply(): Promise<boolean> {
  const d = draft.value;
  if (!d) return false;
  stopRecording();
  const args = { macroId: selected.value, name: d.name.trim() || t("macros.untitled"), events: d.events };
  const r = await run<[number, MacroState]>("set_macro", args, "macros.applied", ([, s]) => s);
  if (!r) return false;
  const [id, state] = r;
  selected.value = id;
  draft.value = { name: state.macros[id].name, events: state.macros[id].events.map((e) => ({ ...e })) };
  return true;
}

async function doWrite() {
  if ((dirty.value && !(await apply())) || selected.value == null) return;
  await run<MacroState>("write_macro", { macroId: selected.value }, "macros.written", (s) => s);
}

useSection({
  id: "macros",
  pending: () => (dirty.value ? 1 : 0),
  parts: () => [{ label: t("keys.macros"), place: "macros" }],
  canApply: () => dirty.value && !busy.value,
  canRevert: () => dirty.value && !busy.value,
  // An applied macro not yet in the flash is written from its own tab only: elsewhere it is not in view.
  canWrite: () => props.writable && !busy.value && (dirty.value || (active.value && !!saved.value && !saved.value.written)),
  apply: async () => {
    await apply();
  },
  revert: () => {
    open(selected.value);
  },
  write: doWrite,
});

async function remove() {
  if (!armed.value) {
    armed.value = true;
    return;
  }
  if (selected.value != null && (await run<MacroState>("delete_macro", { macroId: selected.value }, "macros.deleted", (s) => s))) {
    selected.value = null;
    draft.value = null;
  }
}

const value = (e: Event) => (e.target as HTMLSelectElement).value;
</script>

<template>
  <EmptyState v-if="list.length === 0 && !draft" class="card" icon="macro" :title="$t('macros.noneTitle')" :text="$t('macros.noneText')">
    <span class="mt-1 flex gap-2">
      <button class="primary" @click="recordNew">{{ $t("macros.recordNew") }}</button>
      <button @click="open(null)">{{ $t("macros.createEmpty") }}</button>
    </span>
    <span v-if="state?.free != null" class="hint">{{ $t("macros.free", { n: state.free }) }}</span>
  </EmptyState>
  <div v-else class="px-4 py-3 card flex gap-4 items-start">
    <div class="flex flex-col gap-1 w-[180px]">
      <button
        v-for="m in list"
        :key="m.id"
        class="text-left flex gap-1.5 items-baseline"
        :class="{ 'border-accent': selected === m.id }"
        :title="m.written ? '' : $t('macros.notWritten')"
        @click="open(m.id)"
      >
        <span class="truncate">{{ m.id }}. {{ m.name }}</span
        ><span v-if="!m.written" class="text-edited">●</span>
        <span class="text-xs text-muted ml-auto truncate">{{ bound.get(m.id)?.join(", ") }}</span>
      </button>
      <button @click="open(null)">{{ $t("macros.new") }}</button>
    </div>
    <div v-if="draft" class="flex flex-1 flex-col gap-2 min-w-0">
      <div class="field">
        <span class="field-label">{{ $t("macros.name") }}</span>
        <input v-model="draft.name" class="flex-1" maxlength="40" />
      </div>
      <ol class="m-0 p-0 list-none flex flex-col gap-1 max-h-[320px] overflow-y-auto">
        <li v-for="(e, i) in draft.events" :key="i" class="step">
          <span class="text-xs text-muted text-right">{{ i + 1 }}</span>
          <template v-if="e.type === 'delay'">
            <span class="px-2">{{ $t("macros.delay") }}</span>
            <span class="flex gap-2 items-center">
              <input v-model.number="e.ms" type="number" min="0" max="600000" class="w-[90px]" />
              {{ $t("macros.ms") }}
            </span>
          </template>
          <template v-else>
            <select :value="String(e.down)" @change="e.down = value($event) === 'true'">
              <option value="true">{{ $t("macros.down") }}</option>
              <option value="false">{{ $t("macros.up") }}</option>
            </select>
            <select v-if="e.type === 'key'" v-model.number="e.key">
              <option v-for="k in keys" :key="k.key" :value="k.key">{{ k.name }}</option>
            </select>
            <select v-else :value="e.button" @change="e.button = value($event) as Mouse">
              <option v-for="b in MOUSE" :key="b" :value="b">{{ $t(`bindings.mouse.${b}`) }}</option>
            </select>
          </template>
          <span class="step-actions flex gap-1">
            <button class="icon-btn" :aria-label="$t('macros.moveUp')" :disabled="i === 0" @click="move(i, -1)">
              <AppIcon name="up" />
            </button>
            <button class="icon-btn" :aria-label="$t('macros.moveDown')" :disabled="i === draft.events.length - 1" @click="move(i, 1)">
              <AppIcon name="down" />
            </button>
            <button class="icon-btn" :aria-label="$t('macros.remove')" @click="draft.events.splice(i, 1)">
              <AppIcon name="remove" />
            </button>
          </span>
        </li>
      </ol>
      <p v-if="draft.events.length === 0" class="hint m-0">{{ $t("macros.empty") }}</p>
      <div class="flex flex-wrap gap-2 items-center">
        <button :class="{ 'border-error text-error': recording }" @click="recording ? stopRecording() : startRecording()">
          {{ $t(recording ? "macros.stop" : "macros.record") }}
        </button>
        <label class="flex gap-1 items-center"><input v-model="withDelays" type="checkbox" /> {{ $t("macros.withDelays") }}</label>
        <button @click="add({ type: 'key', key: keys[0]?.key ?? 1, down: true }, { type: 'key', key: keys[0]?.key ?? 1, down: false })">
          {{ $t("macros.addKey") }}
        </button>
        <button @click="add({ type: 'mouse', button: 'left', down: true }, { type: 'mouse', button: 'left', down: false })">
          {{ $t("macros.addMouse") }}
        </button>
        <button @click="add({ type: 'delay', ms: 50 })">{{ $t("macros.addDelay") }}</button>
        <button :disabled="draft.events.length === 0" @click="draft.events = []">{{ $t("macros.clear") }}</button>
      </div>
      <p v-if="recording" class="hint m-0">{{ $t("macros.recordingHint") }}</p>
      <p class="hint m-0">
        {{ state?.free == null ? $t("macros.size", { n: size }) : $t("macros.sizeFree", { n: size, free: state.free }) }}
      </p>
      <div class="flex gap-3 items-center justify-end">
        <span v-if="message" class="text-xs text-muted mr-auto">{{ message }}</span>
        <button
          v-if="selected != null"
          :disabled="busy || boundTo.length > 0"
          :title="boundTo.length > 0 ? $t('macros.boundTo', { keys: boundTo.join(', ') }) : ''"
          @click="remove"
        >
          {{ $t(armed ? "macros.confirmDelete" : "macros.delete") }}
        </button>
      </div>
      <DevBlock v-if="dev.mode && selected != null">
        <div>id {{ selected }} · {{ bytes?.length ?? 0 }} B · free {{ state?.free ?? "—" }}</div>
        <div>{{ hex(bytes ?? []) }}</div>
      </DevBlock>
    </div>
    <EmptyState v-else class="flex-1" icon="macro" :title="$t('macros.pickTitle')" :text="$t('macros.pickText')" />
  </div>
</template>

<style scoped>
/* Fixed columns keep a pause in line with the key steps; the row's buttons sit right after it. */
.step {
  display: grid;
  grid-template-columns: 24px 130px 180px auto;
  gap: 8px;
  align-items: center;
  justify-content: start;
}
.step-actions {
  opacity: 0;
  transition: opacity 0.1s;
}
.step:hover .step-actions,
.step:focus-within .step-actions {
  opacity: 1;
}
</style>
