<script setup lang="ts">
import { nextTick, ref, useId, useTemplateRef } from "vue";
import type { DeviceEntry } from "../types";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ devices: DeviceEntry[]; selected: string | null }>();
const emit = defineEmits<{ pick: [id: string]; rename: [id: string, name: string]; forget: [id: string] }>();

const open = ref(false);
const editing = ref<string | null>(null);
const draft = ref("");
const id = useId();
const menu = useTemplateRef<HTMLElement>("menu");
const input = useTemplateRef<HTMLInputElement[]>("input");

const current = () => props.devices.find((d) => d.id === props.selected) ?? null;

// The blur that commits a rename comes before the click on another row; that click is not a pick.
let renamedAt = 0;

function commitRename() {
  const id = editing.value;
  editing.value = null;
  if (id == null) return;
  renamedAt = Date.now();
  emit("rename", id, draft.value);
}

function onToggle(e: ToggleEvent) {
  open.value = e.newState === "open";
  if (open.value) return;
  if (editing.value != null) commitRename();
  editing.value = null;
}

function pick(id: string) {
  if (editing.value != null || Date.now() - renamedAt < 300) return;
  menu.value?.hidePopover();
  emit("pick", id);
}

async function startRename() {
  const d = current();
  if (!d) return;
  editing.value = d.id;
  draft.value = d.name;
  await nextTick();
  input.value?.[0]?.select();
}

function forget() {
  const d = current();
  menu.value?.hidePopover();
  if (d) emit("forget", d.id);
}
</script>

<template>
  <div>
    <button
      class="flex gap-2 min-w-[190px] items-center"
      :class="{ 'border-accent': open }"
      :style="{ anchorName: `--${id}` }"
      :popovertarget="id"
      aria-haspopup="listbox"
      :aria-expanded="open"
    >
      <AppIcon :name="current()?.connected ? 'plug' : 'plug_off'" :class="current()?.connected ? 'text-accent' : 'text-muted'" />
      <span class="max-w-[200px] truncate">{{ current()?.name ?? "…" }}</span>
      <span class="text-muted ml-auto">▾</span>
    </button>
    <div :id="id" ref="menu" popover class="menu popup w-[320px]" :style="{ positionAnchor: `--${id}` }" role="listbox" @toggle="onToggle">
      <div
        v-for="d in devices"
        :key="d.id"
        class="row"
        :class="{ 'row-on': d.id === selected }"
        role="option"
        :aria-selected="d.id === selected"
        @click="pick(d.id)"
      >
        <AppIcon :name="d.connected ? 'plug' : 'plug_off'" :class="d.connected ? 'text-accent' : 'text-muted'" />
        <input
          v-if="editing === d.id"
          ref="input"
          v-model="draft"
          class="py-0.5 flex-1 min-w-0"
          maxlength="40"
          @click.stop
          @keydown.enter="commitRename"
          @keydown.esc.prevent="editing = null"
          @blur="commitRename"
        />
        <span v-else class="flex-1 truncate">{{ d.name }}</span>
        <span class="text-xs text-muted whitespace-nowrap">{{
          d.connected ? $t(d.driver_mode ? "mode.driver" : "mode.hw") : $t("devices.offline")
        }}</span>
      </div>
      <div class="sep" />
      <!-- mousedown would blur the rename field first and redraw the list under the click. -->
      <button class="item" :disabled="!current()" @mousedown.prevent @click="startRename">
        <span class="ic">✎</span>{{ $t("devices.rename") }}
      </button>
      <button v-if="current() && !current()!.connected" class="item text-error" @click="forget">
        <AppIcon name="remove" class="ic" />{{ $t("devices.forget") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.popup {
  inset: auto;
  margin: 6px 0 0;
  top: anchor(bottom);
  right: anchor(right);
  overflow: visible;
  color: inherit;
}
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
  padding: 0 8px;
  border-radius: 5px;
  cursor: pointer;
}
.row:hover {
  background: var(--key);
}
.row-on {
  box-shadow: inset 2px 0 0 var(--accent);
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
