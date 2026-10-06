<script setup lang="ts">
import { computed, ref } from "vue";
import type { KeyView, SnapTap } from "../types";
import { SNAP_RULES, addGroup, removeGroup, setRule, snapColor } from "../snap";

const props = defineProps<{
  snap: SnapTap;
  selection: Set<number>;
  layout: KeyView[];
  driver: boolean;
  /** Fn combination of the switch; null when none has it. */
  combo: string | null;
}>();
const emit = defineEmits<{ update: [snap: SnapTap] }>();

const picked = ref<number | null>(null);
const current = computed(() => (picked.value != null && picked.value < props.snap.groups.length ? picked.value : null));
const labels = computed(() => new Map(props.layout.map((k) => [k.key, k.label])));

function add() {
  const next = addGroup(props.snap, [...props.selection]);
  picked.value = next.groups.length - 1;
  emit("update", next);
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <label class="switch text-sm">
      <input
        type="checkbox"
        role="switch"
        :checked="snap.enabled"
        :disabled="!driver"
        @change="emit('update', { ...snap, enabled: ($event.target as HTMLInputElement).checked })"
      />
      {{ $t("snap.title") }}
      <span class="text-xs text-muted ml-auto">{{ combo ?? $t("snap.noCombo") }}</span>
    </label>
    <ul class="m-0 p-0 list-none flex flex-col gap-2">
      <li v-for="(g, i) in snap.groups" :key="g.keys.join()" class="flex gap-1 items-center">
        <button
          class="px-2 py-1 bg-key-off flex flex-1 gap-2 items-center"
          :class="i === current ? 'border-text' : 'border-line'"
          :aria-pressed="i === current"
          @click="picked = i"
        >
          <span class="rounded-sm h-2.5 w-2.5" :style="{ background: snapColor(i) }"></span>
          <span v-for="k in g.keys" :key="k" class="text-xs px-1.5 border border-line rounded border-solid bg-key">{{
            labels.get(k)
          }}</span>
          <span class="text-xs text-muted ml-auto">{{ $t(`snap.rules.${g.rule}`) }}</span>
        </button>
        <button class="icon-btn" :aria-label="$t('snap.remove')" :title="$t('snap.remove')" @click="emit('update', removeGroup(snap, i))">
          ×
        </button>
      </li>
    </ul>
    <span v-if="snap.groups.length === 0" class="hint">{{ $t("snap.empty") }}</span>
    <button class="self-start" :disabled="!driver || selection.size < 2" @click="add">{{ $t("snap.add", { n: selection.size }) }}</button>
    <template v-if="current != null">
      <div class="field">
        <span class="field-label">{{ $t("snap.rule") }}</span>
        <span class="inline-flex">
          <button
            v-for="r in SNAP_RULES"
            :key="r"
            class="seg-btn"
            :class="{ 'seg-on': snap.groups[current].rule === r }"
            @click="emit('update', setRule(snap, current, r))"
          >
            {{ $t(`snap.rules.${r}`) }}
          </button>
        </span>
      </div>
      <span class="hint">{{ $t(`snap.hints.${snap.groups[current].rule}`) }}</span>
    </template>
    <span class="text-xs text-muted" :class="{ invisible: driver }">{{ $t("rapid.hwOnly") }}</span>
  </div>
</template>
