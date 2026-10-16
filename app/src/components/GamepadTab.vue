<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyView } from "../types";
import type { GamepadBinding, GamepadConfig } from "../gamepad";
import { capturedKey } from "../gamepad";
import { keyName } from "../bindings";
import GamepadModel from "./GamepadModel.vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{
  config: GamepadConfig;
  depth: number[];
  layout: KeyView[];
  editable: boolean;
  driver: boolean;
  running: boolean;
  error: string | null;
}>();
const emit = defineEmits<{ update: [config: GamepadConfig] }>();
const { t } = useI18n();
const selected = ref<GamepadBinding | "left" | "right">("left");
const capture = ref<GamepadBinding | null>(null);
let previous: number[] = [];
const keys = computed(() => props.layout.filter((key) => key.editable));
const rows = computed<GamepadBinding[]>(() => {
  const side = selected.value.startsWith("left") ? "left" : selected.value.startsWith("right") ? "right" : null;
  return side
    ? ["up", "down", "left", "right", "click"].map((direction) => `${side}_${direction}` as GamepadBinding)
    : [selected.value as GamepadBinding];
});
const title = computed(() =>
  selected.value.startsWith("left")
    ? t("gamepad.leftStick")
    : selected.value.startsWith("right")
      ? t("gamepad.rightStick")
      : t(`gamepad.${selected.value}`),
);
const rowLabel = (binding: GamepadBinding) => t(`gamepad.${binding.replace(/^(?:left|right)_/, "")}`);

function assign(binding: GamepadBinding, key: number | undefined) {
  if (!props.editable || (key !== undefined && !keys.value.some((entry) => entry.key === key))) return;
  const bindings = { ...props.config.bindings };
  if (key === undefined) delete bindings[binding];
  else bindings[binding] = key;
  capture.value = null;
  emit("update", { ...props.config, bindings });
}

function beginCapture(binding: GamepadBinding) {
  previous = [...props.depth];
  capture.value = capture.value === binding ? null : binding;
}

function cancelCapture() {
  capture.value = null;
}

function escape(event: KeyboardEvent) {
  if (capture.value && event.key === "Escape") {
    event.preventDefault();
    capture.value = null;
  }
}

watch(
  () => props.depth,
  (depth) => {
    if (!capture.value) return;
    const key = capturedKey(
      previous,
      depth,
      keys.value.map((entry) => entry.key),
    );
    previous = [...depth];
    if (key !== null) assign(capture.value, key);
  },
);
watch([() => props.config, () => props.layout, () => props.driver, () => props.editable, selected], () => {
  capture.value = null;
});
onMounted(() => {
  window.addEventListener("keydown", escape);
  window.addEventListener("blur", cancelCapture);
});
onUnmounted(() => {
  window.removeEventListener("keydown", escape);
  window.removeEventListener("blur", cancelCapture);
});
</script>

<template>
  <section class="gamepad-tab">
    <header class="gamepad-heading">
      <div>
        <h2>{{ t("gamepad.title") }}</h2>
        <p class="hint">{{ t("gamepad.hint") }}</p>
      </div>
      <label class="switch"
        ><input
          type="checkbox"
          :checked="config.enabled"
          :disabled="!editable"
          @change="emit('update', { ...config, enabled: ($event.target as HTMLInputElement).checked })"
        />{{ t("gamepad.enabled") }}</label
      >
    </header>
    <p v-if="error" class="gamepad-error" role="alert">{{ error }}</p>
    <div class="gamepad-workspace">
      <div class="gamepad-stage"><GamepadModel :config="config" :depth="depth" :selected="selected" @select="selected = $event" /></div>
      <aside class="gamepad-editor">
        <h3>{{ title }}</h3>
        <p class="hint">{{ t("gamepad.assignHint") }}</p>
        <div v-for="binding in rows" :key="binding" class="assignment">
          <label :for="`gamepad-${binding}`">{{ rows.length === 1 ? t("gamepad.keyboardKey") : rowLabel(binding) }}</label>
          <div class="assignment-input">
            <select
              :id="`gamepad-${binding}`"
              :value="config.bindings[binding] ?? ''"
              :disabled="!editable"
              @change="
                assign(
                  binding,
                  ($event.target as HTMLSelectElement).value === '' ? undefined : Number(($event.target as HTMLSelectElement).value),
                )
              "
            >
              <option value="">{{ t("gamepad.unassigned") }}</option>
              <option v-for="key in keys" :key="key.key" :value="key.key">{{ keyName(key, t) }}</option>
            </select>
            <button
              class="icon-btn"
              :disabled="!editable || !driver"
              :class="{ capturing: capture === binding }"
              :aria-pressed="capture === binding"
              :title="t(capture === binding ? 'gamepad.capturing' : 'gamepad.capture')"
              :aria-label="t(capture === binding ? 'gamepad.capturing' : 'gamepad.capture')"
              @click="beginCapture(binding)"
            >
              <AppIcon name="keyboard" />
            </button>
            <button
              class="icon-btn"
              :disabled="!editable || config.bindings[binding] === undefined"
              :title="t('gamepad.clear')"
              :aria-label="t('gamepad.clear')"
              @click="assign(binding, undefined)"
            >
              <AppIcon name="remove" />
            </button>
          </div>
        </div>
        <p v-if="capture" class="hint" role="status">{{ t("gamepad.capturing") }}</p>
        <div class="input-help">
          <h3>{{ t("gamepad.inputTitle") }}</h3>
          <p class="hint">{{ t("gamepad.inputHint") }}</p>
          <p class="hint">{{ t("gamepad.selectedHint") }}</p>
          <p class="input-status" :class="{ running }" role="status">
            <span />{{ t(!driver ? "gamepad.driverRequired" : running ? "gamepad.running" : "gamepad.stopped") }}
          </p>
        </div>
      </aside>
    </div>
  </section>
</template>

<style scoped>
.gamepad-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
  margin: 24px 0 20px;
}
h2 {
  font-size: 18px;
  margin: 0 0 8px;
}
h3 {
  font-size: 15px;
  margin: 0 0 12px;
}
p {
  margin: 0 0 20px;
}
.gamepad-heading p {
  margin: 0;
}
.gamepad-heading .switch {
  flex-shrink: 0;
}
.gamepad-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 280px;
  gap: 28px;
  align-items: start;
}
.gamepad-stage,
.gamepad-editor {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 7px;
}
.gamepad-stage {
  padding: 10px 12px 14px;
}
.gamepad-editor {
  padding: 24px 20px;
}
.assignment {
  display: grid;
  gap: 6px;
  align-items: center;
  margin: 16px 0;
}
.assignment-input {
  display: flex;
  align-items: center;
  gap: 6px;
}
.assignment select {
  width: 100%;
  min-width: 0;
  flex: 1;
}
.assignment-input .capturing {
  border-color: var(--accent);
  color: var(--accent);
}
.input-help {
  border-top: 1px solid var(--line);
  margin-top: 24px;
  padding-top: 20px;
}
.input-help p {
  line-height: 1.7;
  margin-bottom: 12px;
}
.input-status {
  display: flex;
  gap: 7px;
  align-items: center;
  color: var(--muted);
  font-size: 12px;
}
.input-status span {
  height: 6px;
  width: 6px;
  border-radius: 50%;
  background: var(--muted);
  flex-shrink: 0;
}
.input-status.running span {
  background: var(--accent);
}
.gamepad-error {
  color: var(--error);
}
@media (max-width: 850px) {
  .gamepad-workspace {
    grid-template-columns: minmax(0, 1fr);
    gap: 22px;
  }
  .gamepad-stage {
    padding: 4px;
  }
}
@media (max-width: 450px) {
  .gamepad-heading {
    align-items: start;
  }
  .gamepad-heading .switch {
    align-items: start;
  }
  .gamepad-editor {
    padding: 20px 16px;
  }
}
</style>
