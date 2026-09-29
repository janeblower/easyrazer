<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Action, KeyView, Media, Mouse } from "../types";
import { MEDIA, MODS, MOUSE, common, factory, keyName } from "../bindings";

type KeyAction = Extract<Action, { type: "key" }>;
type Change = (key: number, current: Action | null) => Action;

const props = defineProps<{
  /** Bindings of the selected keys. */
  actions: (Action | null)[];
  layout: KeyView[];
}>();
const emit = defineEmits<{ change: [change: Change] }>();
const { t } = useI18n();

const TYPES = ["key", "mouse", "media", "disabled"] as const;
const count = computed(() => props.actions.length);
const type = computed(() => common(props.actions.map((a) => a?.type ?? "other")));
const target = computed(() => common(props.actions.map((a) => (a?.type === "key" ? a.key : null))));
const button = computed(() => common(props.actions.map((a) => (a?.type === "mouse" ? a.button : null))));
const media = computed(() => common(props.actions.map((a) => (a?.type === "media" ? a.media : null))));
const targets = computed(() => props.layout.filter((k) => k.editable).map((k) => ({ key: k.key, name: keyName(k, t) })));

// A modifier counts on either side; the UI sets the left one.
const sides = (bit: number) => (1 << bit) | (1 << (bit + 4));
const mod = (bit: number) => common(props.actions.map((a) => (a?.type === "key" ? (a.mods & sides(bit)) !== 0 : null)));
const asKey = (k: number, a: Action | null): KeyAction => (a?.type === "key" ? a : { type: "key", key: k, mods: 0 });
const value = (e: Event) => (e.target as HTMLSelectElement).value;

function setType(ty: (typeof TYPES)[number]) {
  emit("change", (k, a) => {
    if (a?.type === ty) return a;
    if (ty === "key") return factory(k);
    if (ty === "mouse") return { type: "mouse", button: "left" };
    if (ty === "media") return { type: "media", media: "play" };
    return { type: "disabled" };
  });
}

function setMod(bit: number, on: boolean) {
  emit("change", (k, a) => {
    const x = asKey(k, a);
    return { ...x, mods: on ? x.mods | (1 << bit) : x.mods & ~sides(bit) };
  });
}
</script>

<template>
  <div class="flex flex-col">
    <div class="field">
      <span class="field-label">{{ $t("bindings.action") }}</span>
      <span class="inline-flex">
        <button v-for="ty in TYPES" :key="ty" class="seg-btn" :class="{ 'seg-on': type === ty }" :disabled="!count" @click="setType(ty)">
          {{ $t(`bindings.types.${ty}`) }}
        </button>
      </span>
      <button :disabled="!count" @click="emit('change', factory)">{{ $t("bindings.factory") }}</button>
    </div>
    <p v-if="type === 'other'" class="hint">{{ $t("bindings.other") }}</p>
    <template v-if="type === 'key'">
      <div class="field">
        <span class="field-label">{{ $t("bindings.key") }}</span>
        <select :value="target ?? ''" @change="emit('change', (k, a) => ({ ...asKey(k, a), key: Number(value($event)) }))">
          <option v-if="target == null" value="" disabled>{{ $t("actuation.mixed") }}</option>
          <option v-for="o in targets" :key="o.key" :value="o.key">{{ o.name }}</option>
        </select>
      </div>
      <div class="field">
        <span class="field-label">{{ $t("bindings.mods") }}</span>
        <label v-for="(m, bit) in MODS" :key="m" class="flex gap-1 items-center">
          <input
            type="checkbox"
            :checked="mod(bit) === true"
            :indeterminate="mod(bit) === null"
            @change="setMod(bit, ($event.target as HTMLInputElement).checked)"
          />
          {{ $t(`bindings.modNames.${m}`) }}
        </label>
      </div>
    </template>
    <div v-else-if="type === 'mouse'" class="field">
      <span class="field-label">{{ $t("bindings.button") }}</span>
      <select :value="button ?? ''" @change="emit('change', () => ({ type: 'mouse', button: value($event) as Mouse }))">
        <option v-if="button == null" value="" disabled>{{ $t("actuation.mixed") }}</option>
        <option v-for="b in MOUSE" :key="b" :value="b">{{ $t(`bindings.mouse.${b}`) }}</option>
      </select>
    </div>
    <div v-else-if="type === 'media'" class="field">
      <span class="field-label">{{ $t("bindings.media") }}</span>
      <select :value="media ?? ''" @change="emit('change', () => ({ type: 'media', media: value($event) as Media }))">
        <option v-if="media == null" value="" disabled>{{ $t("actuation.mixed") }}</option>
        <option v-for="m in MEDIA" :key="m" :value="m">{{ $t(`bindings.mediaNames.${m}`) }}</option>
      </select>
    </div>
  </div>
</template>
