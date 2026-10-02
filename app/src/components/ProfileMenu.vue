<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, useTemplateRef } from "vue";
import type { ProfilesView } from "../types";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ view: ProfilesView | null; writable: boolean }>();
const emit = defineEmits<{
  load: [id: number];
  create: [];
  duplicate: [id: number];
  rename: [id: number, name: string];
  remove: [id: number];
  startup: [id: number];
  free: [id: number];
  write: [id: number];
}>();

const open = ref(false);
const actions = ref<number | null>(null); // row whose ⋯ menu is open
const editing = ref<number | null>(null);
const draft = ref("");
const root = useTemplateRef<HTMLElement>("root");
const input = useTemplateRef<HTMLInputElement[]>("input");

const loaded = () => props.view?.profiles.find((p) => p.id === props.view?.loaded) ?? null;

function close() {
  open.value = false;
  actions.value = null;
  editing.value = null;
}

function onDocClick(e: MouseEvent) {
  if (!root.value?.contains(e.target as Node)) close();
}

onMounted(() => {
  document.addEventListener("mousedown", onDocClick);
});
onUnmounted(() => {
  document.removeEventListener("mousedown", onDocClick);
});

function pick(id: number) {
  if (editing.value != null) return;
  close();
  if (id !== props.view?.loaded) emit("load", id);
}

async function startRename(id: number, name: string) {
  actions.value = null;
  editing.value = id;
  draft.value = name;
  await nextTick();
  input.value?.[0]?.select();
}

function commitRename() {
  const id = editing.value;
  editing.value = null;
  if (id != null && draft.value.trim()) emit("rename", id, draft.value);
}

function act(e: "duplicate" | "remove" | "startup" | "free" | "write", id: number) {
  actions.value = null;
  switch (e) {
    case "duplicate": {
      emit("duplicate", id);
      break;
    }
    case "remove": {
      emit("remove", id);
      break;
    }
    case "startup": {
      emit("startup", id);
      break;
    }
    case "free": {
      emit("free", id);
      break;
    }
    default: {
      emit("write", id);
    }
  }
}
</script>

<template>
  <div ref="root" class="relative">
    <button
      class="flex gap-2 min-w-[190px] items-center"
      :class="{ 'border-accent': open }"
      :disabled="!view"
      aria-haspopup="listbox"
      :aria-expanded="open"
      @click="open = !open"
    >
      <span class="max-w-[200px] truncate">{{ loaded()?.name ?? "…" }}</span>
      <span v-if="loaded()?.slot" class="slot slot-flash">{{ loaded()!.slot }}/5</span>
      <span class="text-muted ml-auto">▾</span>
    </button>
    <div v-if="open && view" class="menu w-[320px] right-0 top-[calc(100%+6px)]" role="listbox">
      <div
        v-for="p in view.profiles"
        :key="p.id"
        class="row group"
        :class="{ 'row-on': p.id === view.loaded, 'bg-key': actions === p.id }"
        role="option"
        :aria-selected="p.id === view.loaded"
        @click="pick(p.id)"
      >
        <span class="text-accent text-center w-4" :title="$t('profiles.isStartup')">{{
          p.slot != null && p.slot === view.startup ? "⏻" : ""
        }}</span>
        <input
          v-if="editing === p.id"
          ref="input"
          v-model="draft"
          class="py-0.5 flex-1 min-w-0"
          maxlength="32"
          @click.stop
          @keydown.enter="commitRename"
          @keydown.esc="editing = null"
          @blur="commitRename"
        />
        <span v-else class="flex-1 truncate">{{ p.name }}</span>
        <span v-if="p.unsaved" class="text-xs text-edited">{{ $t("profiles.unsaved") }}</span>
        <span v-if="p.slot" class="slot slot-flash">{{ p.slot }}/5</span>
        <span v-else class="slot">{{ $t("profiles.onlyHere") }}</span>
        <button
          class="dots"
          :class="{ 'dots-on': actions === p.id }"
          :title="$t('profiles.menu')"
          :aria-label="$t('profiles.menu')"
          @click.stop="actions = actions === p.id ? null : p.id"
        >
          ⋯
        </button>
        <div v-if="actions === p.id" class="menu w-[240px] right-[calc(100%+10px)] top-[-4px]" @click.stop>
          <template v-if="p.slot">
            <button class="item" :disabled="!writable || p.slot === view.startup" @click="act('startup', p.id)">
              <span class="ic">⏻</span>{{ $t(p.slot === view.startup ? "profiles.isStartup" : "profiles.startup") }}
            </button>
          </template>
          <button
            v-else
            class="item"
            :disabled="!writable || !view.free_slot"
            :title="view.free_slot ? '' : $t('profiles.noSlot')"
            @click="act('write', p.id)"
          >
            <AppIcon name="write" class="ic" />{{ $t("profiles.write") }}
          </button>
          <button class="item" @click="startRename(p.id, p.name)"><span class="ic">✎</span>{{ $t("profiles.rename") }}</button>
          <button class="item" @click="act('duplicate', p.id)"><span class="ic">⧉</span>{{ $t("profiles.duplicate") }}</button>
          <button
            v-if="p.slot"
            class="item"
            :disabled="!writable || view.profiles.filter((q) => q.slot != null).length <= 1"
            @click="act('free', p.id)"
          >
            <span class="ic">⊘</span>{{ $t("profiles.free", { slot: p.slot }) }}
          </button>
          <div class="sep" />
          <button class="item text-error" :disabled="view.profiles.length <= 1 || (!!p.slot && !writable)" @click="act('remove', p.id)">
            <AppIcon name="remove" class="ic" />{{ $t("profiles.remove") }}
          </button>
        </div>
      </div>
      <div class="sep" />
      <button class="item text-muted" @click="(close(), emit('create'))">
        <span class="ic text-accent">＋</span>{{ $t("profiles.new") }}
      </button>
      <div class="sep" />
      <div class="text-[11px] text-muted px-2 py-1">{{ $t("profiles.legend") }}</div>
    </div>
  </div>
</template>

<style scoped>
.menu {
  position: absolute;
  z-index: 10;
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgb(0 0 0 / 60%);
  padding: 4px;
}
.row {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 4px 0 8px;
  border-radius: 5px;
  cursor: pointer;
}
.row:hover {
  background: var(--key);
}
.row-on {
  box-shadow: inset 2px 0 0 var(--accent);
}
.slot {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 9px;
  border: 1px solid var(--line);
  color: var(--muted);
  white-space: nowrap;
}
.slot-flash {
  border-color: var(--write-line);
  color: var(--write);
}
.dots {
  width: 26px;
  height: 24px;
  padding: 0;
  background: transparent;
  border-color: transparent;
  color: var(--muted);
  visibility: hidden;
}
.row:hover .dots,
.dots-on {
  visibility: visible;
}
.dots:hover,
.dots-on {
  background: var(--bg);
  color: var(--text);
  border-color: var(--line);
}
.item {
  display: flex;
  width: 100%;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  background: transparent;
  border: none;
  text-align: left;
}
.item:hover:not(:disabled) {
  background: var(--key);
}
.ic {
  width: 16px;
  text-align: center;
  color: var(--muted);
}
.sep {
  height: 1px;
  background: var(--line);
  margin: 4px 0;
}
</style>
