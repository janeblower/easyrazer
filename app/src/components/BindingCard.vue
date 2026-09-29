<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Action, KeyView, Media, Mouse } from "../types";
import { MEDIA, MODS, MOUSE, factory, keyName } from "../bindings";

const props = defineProps<{
  /** Selected key, and what it does now; `action` is null for a binding the app does not edit. */
  keyId: number | null;
  action: Action | null;
  layout: KeyView[];
}>();
const emit = defineEmits<{ set: [action: Action] }>();
const { t } = useI18n();

const TYPES = ["key", "mouse", "media", "disabled"] as const;
const type = computed(() => props.action?.type ?? "other");
const keyAction = computed(() => (props.action?.type === "key" ? props.action : null));
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
  // Not a key from the list: show the current one again.
  else input.value = targetName.value;
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
          :disabled="keyId == null"
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
          :placeholder="$t('bindings.search')"
          @focus="($event.target as HTMLInputElement).select()"
          @change="pickTarget"
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
    <div v-else-if="action?.type === 'media'" class="field">
      <span class="field-label">{{ $t("bindings.media") }}</span>
      <select :value="action.media" @change="emit('set', { type: 'media', media: value($event) as Media })">
        <option v-for="m in MEDIA" :key="m" :value="m">{{ $t(`bindings.mediaNames.${m}`) }}</option>
      </select>
    </div>
  </div>
</template>
