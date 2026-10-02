<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useI18n } from "vue-i18n";
import type { KeyView, MacroEvent, MacroState, Mouse } from "../types";
import { MOUSE, bodySize, codeName, footprint, keyName } from "../bindings";
import { useConfirmWrite } from "../confirmWrite";
import { trackUnapplied } from "../unapplied";
import ConfirmWrite from "./ConfirmWrite.vue";
import ActionBar from "./ActionBar.vue";
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
trackUnapplied(() => dirty.value);
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

function move(i: number, by: number) {
  const ev = draft.value?.events;
  if (!ev || i + by < 0 || i + by >= ev.length) return;
  [ev[i], ev[i + by]] = [ev[i + by], ev[i]];
}

onUnmounted(stopRecording);

async function run(command: string, args: Record<string, unknown>, done: string) {
  busy.value = true;
  try {
    emit("update", await invoke<MacroState>(command, args));
    message.value = t(done);
    return true;
  } catch (error) {
    message.value = String(error);
    return false;
  } finally {
    busy.value = false;
  }
}

async function apply(): Promise<boolean> {
  const d = draft.value;
  if (!d) return false;
  stopRecording();
  busy.value = true;
  try {
    const [id, state] = await invoke<[number, MacroState]>("set_macro", {
      id: selected.value,
      name: d.name.trim() || t("macros.untitled"),
      events: d.events,
    });
    emit("update", state);
    selected.value = id;
    draft.value = { name: state.macros[id].name, events: state.macros[id].events.map((e) => ({ ...e })) };
    message.value = t("macros.applied");
    return true;
  } catch (error) {
    message.value = String(error);
    return false;
  } finally {
    busy.value = false;
  }
}

async function doWrite() {
  if ((dirty.value && !(await apply())) || selected.value == null) return;
  await run("write_macro", { id: selected.value }, "macros.written");
}

const { asking, write, onConfirm } = useConfirmWrite(doWrite, (error) => (message.value = String(error)));

async function remove() {
  if (!armed.value) {
    armed.value = true;
    return;
  }
  if (selected.value != null && (await run("delete_macro", { id: selected.value }, "macros.deleted"))) {
    selected.value = null;
    draft.value = null;
  }
}

const value = (e: Event) => (e.target as HTMLSelectElement).value;
</script>

<template>
  <div class="px-4 py-3 card flex gap-4 items-start">
    <div class="flex flex-col gap-1 w-[180px]">
      <button
        v-for="m in list"
        :key="m.id"
        class="text-left truncate"
        :class="{ 'seg-on': selected === m.id }"
        :title="m.written ? '' : $t('macros.notWritten')"
        @click="open(m.id)"
      >
        {{ m.name }}<span v-if="!m.written" class="text-edited"> ●</span>
      </button>
      <button @click="open(null)">{{ $t("macros.new") }}</button>
    </div>
    <div v-if="draft" class="flex flex-1 flex-col gap-2 min-w-0">
      <div class="field">
        <span class="field-label">{{ $t("macros.name") }}</span>
        <input v-model="draft.name" class="flex-1" maxlength="40" />
      </div>
      <ol class="m-0 pl-6 flex flex-col gap-1 max-h-[320px] overflow-y-auto">
        <li v-for="(e, i) in draft.events" :key="i">
          <span class="flex gap-2 items-center">
            <template v-if="e.type === 'delay'">
              {{ $t("macros.delay") }}
              <input v-model.number="e.ms" type="number" min="0" max="600000" class="w-[80px]" />
              {{ $t("macros.ms") }}
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
            <span class="ml-auto flex gap-1">
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
          </span>
        </li>
      </ol>
      <p v-if="draft.events.length === 0" class="hint m-0">{{ $t("macros.empty") }}</p>
      <div class="flex flex-wrap gap-2 items-center">
        <button :class="{ 'seg-on': recording }" @click="recording ? stopRecording() : startRecording()">
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
      <ActionBar
        :pending="dirty ? $t('common.notApplied') : ''"
        :can-revert="dirty && !busy"
        :can-apply="dirty && !busy"
        :can-write="writable && !busy && (dirty || !saved?.written)"
        :write-title="$t('macros.write')"
        @revert="open(selected)"
        @apply="apply"
        @write="write"
      />
    </div>
    <p v-else class="hint flex-1">{{ $t("macros.pick") }}</p>
    <ConfirmWrite v-if="asking" @yes="onConfirm" @no="asking = false" />
  </div>
</template>
