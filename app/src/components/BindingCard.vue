<script setup lang="ts">
import { computed, useId } from "vue";
import { useI18n } from "vue-i18n";
import type { Action, KeyView, Macro, MacroMode, Media, Mouse, System } from "../types";
import { MEDIA, MODS, MOUSE, SYSTEM, factory, keyName } from "../bindings";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  title: string;
  /** Selected key, and what it does now; `action` is null for a binding the app does not edit. */
  keyId: number | null;
  action: Action | null;
  layout: KeyView[];
  macros: Record<number, Macro>;
  /** What the reset button restores. */
  factoryAction: Action | null;
  /** The Hypershift layer: offers system actions. */
  hypershift?: boolean;
  /** Other keys holding a unique system action, as "Fn+LShift". */
  taken?: Partial<Record<System, string>>;
}>();
const emit = defineEmits<{ set: [action: Action] }>();
const { t } = useI18n();
const listId = useId();

const ICONS = { key: "keyboard", mouse: "mouse", media: "music", macro: "macro", system: "cog", disabled: "ban" } as const;
type Type = keyof typeof ICONS;
const types = computed(() =>
  props.hypershift
    ? (["key", "mouse", "media", "macro", "system", "disabled"] as const)
    : (["key", "mouse", "media", "macro", "disabled"] as const),
);
const MODES: MacroMode[] = ["times", "hold", "toggle"];
const type = computed(() => props.action?.type ?? "other");
const keyAction = computed(() => (props.action?.type === "key" ? props.action : null));
const macroAction = computed(() => (props.action?.type === "macro" ? props.action : null));
const macroList = computed(() => Object.entries(props.macros).map(([id, m]) => ({ id: Number(id), name: m.name })));
const targets = computed(() => props.layout.filter((k) => k.editable).map((k) => ({ key: k.key, name: keyName(k, t) })));
const targetName = computed(() => targets.value.find((o) => o.key === keyAction.value?.key)?.name ?? "");

// A modifier counts on either side; the UI sets the left one.
const sides = (bit: number) => (1 << bit) | (1 << (bit + 4));
const value = (e: Event) => (e.target as HTMLSelectElement).value;

const DEFAULTS: Record<Exclude<Type, "key" | "macro">, Action> = {
  mouse: { type: "mouse", button: "left" },
  media: { type: "media", media: "play" },
  system: { type: "system", action: "brightness_down" },
  disabled: { type: "disabled" },
};

function setType(ty: Type) {
  if (props.keyId == null || type.value === ty) return;
  const first = macroList.value[0];
  if (ty === "key") emit("set", factory(props.keyId));
  else if (ty !== "macro") emit("set", DEFAULTS[ty]);
  else if (first) emit("set", { type: "macro", id: first.id, mode: "times", count: 1 });
}

function pickTarget(e: Event) {
  const input = e.target as HTMLInputElement;
  const name = input.value.trim().toLowerCase();
  const found = targets.value.find((o) => o.name.toLowerCase() === name);
  if (found && keyAction.value) emit("set", { ...keyAction.value, key: found.key });
}

// A refused pick leaves the prop as it was, so the element is put back by hand.
function pickSystem(e: Event) {
  const el = e.target as HTMLSelectElement;
  const picked = el.value as System;
  if (props.action?.type === "system") el.value = props.action.action;
  emit("set", { type: "system", action: picked });
}

function setMacro(change: Partial<{ id: number; mode: MacroMode; count: number }>) {
  const a = macroAction.value;
  if (a) emit("set", { ...a, ...change });
}

function setMod(bit: number, on: boolean) {
  const a = keyAction.value;
  if (a) emit("set", { ...a, mods: on ? a.mods | (1 << bit) : a.mods & ~sides(bit) });
}
</script>

<template>
  <div class="flex flex-col">
    <div class="flex gap-2 items-center">
      <span class="text-sm flex-1">{{ title }}</span>
      <button
        class="icon-btn"
        :disabled="keyId == null || !factoryAction"
        :title="$t('bindings.factory')"
        :aria-label="$t('bindings.factory')"
        @click="emit('set', factoryAction!)"
      >
        <AppIcon name="revert" />
      </button>
    </div>
    <div class="flex gap-2 items-center">
      <span class="inline-flex">
        <button
          v-for="ty in types"
          :key="ty"
          class="icon-btn seg-btn"
          :class="keyId != null && type === ty ? (hypershift ? 'border-hs bg-hs text-ink' : 'seg-on') : ''"
          :disabled="keyId == null || (ty === 'macro' && macroList.length === 0)"
          :title="ty === 'macro' && macroList.length === 0 ? $t('bindings.noMacros') : $t(`bindings.types.${ty}`)"
          :aria-label="$t(`bindings.types.${ty}`)"
          :aria-pressed="keyId != null && type === ty"
          @click="setType(ty)"
        >
          <AppIcon :name="ICONS[ty]" />
        </button>
      </span>
      <span class="hint">{{ keyId != null && type !== "other" ? $t(`bindings.types.${type}`) : "" }}</span>
    </div>
    <p v-if="keyId != null && type === 'other'" class="hint">{{ $t("bindings.other") }}</p>
    <template v-if="keyAction">
      <div class="field">
        <span class="field-label">{{ $t("bindings.key") }}</span>
        <input
          :value="targetName"
          :list="listId"
          :placeholder="targetName || $t('bindings.search')"
          @focus="($event.target as HTMLInputElement).value = ''"
          @change="pickTarget"
          @blur="($event.target as HTMLInputElement).value = targetName"
        />
        <datalist :id="listId">
          <option v-for="o in targets" :key="o.key" :value="o.name" />
        </datalist>
      </div>
      <div class="field">
        <span class="field-label">{{ $t("bindings.mods") }}</span>
        <label v-for="(m, bit) in MODS" :key="m" class="flex gap-1 items-center">
          <input
            type="checkbox"
            :checked="(keyAction.mods & sides(bit)) !== 0"
            @change="setMod(bit, ($event.target as HTMLInputElement).checked)"
          />
          {{ $t(`bindings.modNames.${m}`) }}
        </label>
      </div>
    </template>
    <div v-else-if="action?.type === 'mouse'" class="field">
      <span class="field-label">{{ $t("bindings.button") }}</span>
      <select :value="action.button" @change="emit('set', { type: 'mouse', button: value($event) as Mouse })">
        <option v-for="b in MOUSE" :key="b" :value="b">{{ $t(`bindings.mouse.${b}`) }}</option>
      </select>
    </div>
    <template v-else-if="macroAction">
      <div class="field">
        <span class="field-label">{{ $t("bindings.macro") }}</span>
        <select :value="macroAction.id" @change="setMacro({ id: Number(value($event)) })">
          <option v-if="!(macroAction.id in macros)" :value="macroAction.id">#{{ macroAction.id }}</option>
          <option v-for="m in macroList" :key="m.id" :value="m.id">{{ m.name }}</option>
        </select>
      </div>
      <div class="field">
        <span class="field-label">{{ $t("bindings.play") }}</span>
        <select :value="macroAction.mode" @change="setMacro({ mode: value($event) as MacroMode, count: macroAction.count || 1 })">
          <option v-for="m in MODES" :key="m" :value="m">{{ $t(`bindings.modes.${m}`) }}</option>
        </select>
        <input
          v-if="macroAction.mode === 'times'"
          type="number"
          min="1"
          max="255"
          class="w-[60px]"
          :value="macroAction.count"
          @change="setMacro({ count: Math.min(255, Math.max(1, Math.round(Number(value($event)) || 1))) })"
        />
      </div>
    </template>
    <div v-else-if="action?.type === 'media'" class="field">
      <span class="field-label">{{ $t("bindings.media") }}</span>
      <select :value="action.media" @change="emit('set', { type: 'media', media: value($event) as Media })">
        <option v-for="m in MEDIA" :key="m" :value="m">{{ $t(`bindings.mediaNames.${m}`) }}</option>
      </select>
    </div>
    <div v-else-if="action?.type === 'system'" class="field">
      <span class="field-label">{{ $t("bindings.function") }}</span>
      <select class="sys" :value="action.action" @change="pickSystem">
        <button><selectedcontent /></button>
        <option v-for="s in SYSTEM" :key="s" :value="s">
          <AppIcon class="text-text h-4 w-4" :name="s" />{{ $t(`bindings.system.${s}`)
          }}<span v-if="taken?.[s]" class="taken">— {{ taken[s] }}</span>
        </option>
      </select>
    </div>
  </div>
</template>

<style scoped>
/* WebView2 renders rich options only in a customizable select. */
.sys,
.sys::picker(select) {
  appearance: base-select;
}
.sys {
  display: flex;
  align-items: center;
  gap: 8px;
}
.sys::picker(select) {
  margin-top: 4px;
  padding: 4px;
  background: var(--key);
  border: 1px solid var(--line);
  border-radius: 6px;
}
.sys option {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 5px 8px;
  border-radius: 4px;
  color: var(--muted);
}
.sys option:hover {
  background: var(--line);
}
.sys option:checked {
  color: var(--text);
}
.sys option::checkmark {
  order: 1;
  margin-left: auto;
  color: var(--accent);
}
.sys selectedcontent {
  display: flex;
  align-items: center;
  gap: 8px;
}
.taken {
  font-size: 12px;
  opacity: 0.6;
}
.sys selectedcontent .taken {
  display: none;
}
</style>
