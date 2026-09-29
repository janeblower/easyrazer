<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Action, KeyView, Macro, MacroMode, Media, Mouse } from "../types";
import { MEDIA, MODS, MOUSE, factory, keyName } from "../bindings";

const props = defineProps<{
  /** Selected key, and what it does now; `action` is null for a binding the app does not edit. */
  keyId: number | null;
  action: Action | null;
  layout: KeyView[];
  macros: Record<number, Macro>;
}>();
const emit = defineEmits<{ set: [action: Action] }>();
const { t } = useI18n();

const TYPES = ["key", "mouse", "media", "macro", "disabled"] as const;
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

function setType(ty: (typeof TYPES)[number]) {
  if (props.keyId == null || type.value === ty) return;
  switch (ty) {
    case "key": {
      emit("set", factory(props.keyId));
      break;
    }
    case "mouse": {
      emit("set", { type: "mouse", button: "left" });
      break;
    }
    case "media": {
      emit("set", { type: "media", media: "play" });
      break;
    }
    case "macro": {
      const first = macroList.value[0];
      if (first) emit("set", { type: "macro", id: first.id, mode: "times", count: 1 });
      break;
    }
    default: {
      emit("set", { type: "disabled" });
    }
  }
}

function pickTarget(e: Event) {
  const input = e.target as HTMLInputElement;
  const name = input.value.trim().toLowerCase();
  const found = targets.value.find((o) => o.name.toLowerCase() === name);
  if (found && keyAction.value) emit("set", { ...keyAction.value, key: found.key });
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
    <div class="field">
      <span class="field-label">{{ $t("bindings.action") }}</span>
      <span class="inline-flex">
        <button
          v-for="ty in TYPES"
          :key="ty"
          class="seg-btn"
          :class="{ 'seg-on': keyId != null && type === ty }"
          :disabled="keyId == null || (ty === 'macro' && macroList.length === 0)"
          :title="ty === 'macro' && macroList.length === 0 ? $t('bindings.noMacros') : ''"
          @click="setType(ty)"
        >
          {{ $t(`bindings.types.${ty}`) }}
        </button>
      </span>
      <button :disabled="keyId == null" @click="emit('set', factory(keyId!))">{{ $t("bindings.factory") }}</button>
    </div>
    <p v-if="keyId != null && type === 'other'" class="hint">{{ $t("bindings.other") }}</p>
    <template v-if="keyAction">
      <div class="field">
        <span class="field-label">{{ $t("bindings.key") }}</span>
        <input
          :value="targetName"
          list="bind-targets"
          :placeholder="targetName || $t('bindings.search')"
          @focus="($event.target as HTMLInputElement).value = ''"
          @change="pickTarget"
          @blur="($event.target as HTMLInputElement).value = targetName"
        />
        <datalist id="bind-targets">
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
  </div>
</template>
