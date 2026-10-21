<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { KeyView } from "../types";
import type { GamepadBinding, GamepadConfig, GamepadControl } from "../gamepad";
import { ANALOG_BINDINGS, capturedKey } from "../gamepad";
import { keyName } from "../bindings";
import GamepadModel from "./GamepadModel.vue";
import AppIcon from "./AppIcon.vue";

const selected = defineModel<GamepadControl | null>("selected", { required: true });
const props = defineProps<{
  config: GamepadConfig;
  depth: number[];
  layout: KeyView[];
  editable: boolean;
  driver: boolean;
  running: boolean;
  error: string | null;
  driverInstalled: boolean | null;
  /** The installed driver's version when it is older than the one the app needs. */
  driverOutdated: string | null;
  driverOperation: "install" | "uninstall" | null;
  installError: string | null;
}>();
const emit = defineEmits<{ update: [config: GamepadConfig]; install: []; driver: [] }>();
const available = computed(() => props.driver && props.driverInstalled === true && !props.driverOperation);
const { t } = useI18n();
const notice = computed(() => {
  if (props.driverOperation) return t(props.driverOperation === "uninstall" ? "gamepad.uninstalling" : "gamepad.installing");
  if (props.driverInstalled === null) return t("gamepad.checkingDriver");
  if (props.driverOutdated) return t("gamepad.driverOutdated", { version: props.driverOutdated });
  return t(props.driverInstalled ? "gamepad.driverRequired" : "gamepad.driverMissing");
});
const canEdit = computed(() => props.editable && available.value);
const capture = ref<GamepadBinding | null>(null);
let previous: number[] = [];
const keys = computed(() => props.layout.filter((key) => key.editable));
const rows = computed<GamepadBinding[]>(() => {
  if (!selected.value) return [];
  const side = selected.value.startsWith("left") ? "left" : selected.value.startsWith("right") ? "right" : null;
  return side
    ? ["up", "down", "left", "right", "click"].map((direction) => `${side}_${direction}` as GamepadBinding)
    : [selected.value as GamepadBinding];
});
const title = computed(() =>
  selected.value?.startsWith("left")
    ? t("gamepad.leftStick")
    : selected.value?.startsWith("right")
      ? t("gamepad.rightStick")
      : t(`gamepad.${selected.value}`),
);
function setMaximum(binding: GamepadBinding, event: Event) {
  if (!canEdit.value) return;
  emit("update", {
    ...props.config,
    max_depth: { ...props.config.max_depth, [binding]: Number((event.target as HTMLInputElement).value) },
  });
}
const rowLabel = (binding: GamepadBinding) => t(`gamepad.${binding.replace(/^(?:left|right)_/, "")}`);
function bindingName(binding: GamepadBinding) {
  const key = keys.value.find((entry) => entry.key === props.config.bindings[binding]);
  return key ? keyName(key, t) : t("gamepad.capture");
}

function assign(binding: GamepadBinding, key: number | undefined) {
  if (!canEdit.value || (key !== undefined && !keys.value.some((entry) => entry.key === key))) return;
  const bindings = { ...props.config.bindings };
  const maxDepth = { ...props.config.max_depth };
  if (key === undefined) {
    delete bindings[binding];
    delete maxDepth[binding];
  } else bindings[binding] = key;
  capture.value = null;
  emit("update", { ...props.config, bindings, max_depth: maxDepth });
}

function beginCapture(binding: GamepadBinding) {
  if (!canEdit.value) return;
  previous = [...props.depth];
  capture.value = capture.value === binding ? null : binding;
}

function cancelCapture() {
  capture.value = null;
}

// The app clears the selection itself; a capture takes the first Esc, so this
// listens in the capture phase, ahead of the app's listener added earlier.
function escape(event: KeyboardEvent) {
  if (event.key === "Escape" && capture.value) {
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
watch([() => props.config, () => props.layout, canEdit, selected], () => {
  capture.value = null;
});
onMounted(() => {
  window.addEventListener("keydown", escape, true);
  window.addEventListener("blur", cancelCapture);
});
onUnmounted(() => {
  window.removeEventListener("keydown", escape, true);
  window.removeEventListener("blur", cancelCapture);
});
</script>

<template>
  <section class="gamepad-tab">
    <div v-if="driverInstalled !== true || !driver || driverOperation" class="gamepad-notice" role="status" :aria-busy="!!driverOperation">
      <div class="gamepad-warning">
        <span class="gamepad-warning-icon" aria-hidden="true">!</span>
        <p>{{ notice }}</p>
        <button
          v-if="driverInstalled === false || driverOperation === 'install'"
          :disabled="!!driverOperation"
          class="primary"
          @click="emit('install')"
        >
          <span v-if="driverOperation" class="install-spinner" aria-hidden="true" />
          <AppIcon v-else name="shield" />
          {{ t(driverOutdated ? "gamepad.update" : "gamepad.install") }}
        </button>
        <button v-else-if="driverInstalled && !driver && !driverOperation" class="primary" @click="emit('driver')">
          {{ t("gamepad.switchDriver") }}
        </button>
        <p v-if="installError" class="gamepad-error" role="alert">{{ installError }}</p>
      </div>
    </div>
    <div class="gamepad-content" :class="{ unavailable: !available }" :inert="!available">
      <header class="gamepad-heading">
        <p class="hint">{{ t("gamepad.hint") }}</p>
        <label class="switch"
          ><input
            type="checkbox"
            :checked="config.enabled"
            :disabled="!canEdit"
            @change="emit('update', { ...config, enabled: ($event.target as HTMLInputElement).checked })"
          />{{ t("gamepad.enabled") }}</label
        >
      </header>
      <p v-if="error" class="gamepad-error" role="alert">{{ error }}</p>
      <div class="gamepad-workspace">
        <div class="gamepad-stage">
          <GamepadModel :config="config" :depth="available ? depth : []" :selected="selected" @select="available && (selected = $event)" />
        </div>
        <aside class="gamepad-editor">
          <template v-if="selected">
            <h3>{{ title }}</h3>
            <p class="hint">{{ t("gamepad.assignHint") }}</p>
            <div v-for="binding in rows" :key="binding" class="assignment">
              <label :for="`gamepad-${binding}`">{{ rows.length === 1 ? t("gamepad.keyboardKey") : rowLabel(binding) }}</label>
              <div class="assignment-input">
                <button
                  :id="`gamepad-${binding}`"
                  class="record-key"
                  :disabled="!canEdit"
                  :class="{ capturing: capture === binding }"
                  :aria-pressed="capture === binding"
                  :title="t(!driver ? 'gamepad.driverRequired' : 'gamepad.capture')"
                  @click="beginCapture(binding)"
                >
                  {{ capture === binding ? t("gamepad.capturing") : bindingName(binding) }}
                </button>
                <button
                  class="icon-btn"
                  :disabled="!canEdit || config.bindings[binding] === undefined"
                  :title="t('gamepad.clear')"
                  :aria-label="t('gamepad.clear')"
                  @click="assign(binding, undefined)"
                >
                  <AppIcon name="remove" />
                </button>
              </div>
              <label v-if="ANALOG_BINDINGS.includes(binding)" class="maximum-depth">
                <span
                  >{{ t("gamepad.maxDepth") }} <output>{{ config.max_depth?.[binding] ?? 100 }}%</output></span
                >
                <input
                  type="range"
                  min="1"
                  max="100"
                  step="1"
                  :value="config.max_depth?.[binding] ?? 100"
                  :disabled="!canEdit"
                  :aria-label="`${rowLabel(binding)}: ${t('gamepad.maxDepth')}`"
                  @input="setMaximum(binding, $event)"
                />
              </label>
            </div>
          </template>
          <p v-else class="hint">{{ t("gamepad.selectHint") }}</p>
          <div class="input-help">
            <h3>{{ t("gamepad.inputTitle") }}</h3>
            <p class="hint">{{ t("gamepad.inputHint") }}</p>
            <p class="input-status" :class="{ running }" role="status"><span />{{ t(running ? "gamepad.running" : "gamepad.stopped") }}</p>
          </div>
        </aside>
      </div>
    </div>
  </section>
</template>

<style scoped>
.gamepad-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
  margin: 4px 0 12px;
  flex-shrink: 0;
}
.gamepad-tab {
  position: relative;
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}
.gamepad-content {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}
.unavailable {
  opacity: 0.4;
  pointer-events: none;
}
.gamepad-notice {
  position: absolute;
  inset: 0;
  z-index: 1;
  display: grid;
  place-items: center;
  padding: 16px;
  border-radius: 7px;
  background: rgb(0 0 0 / 62%);
}
.gamepad-warning {
  width: min(100%, 380px);
  padding: 22px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
  box-shadow: 0 12px 36px rgb(0 0 0 / 45%);
  text-align: center;
}
.gamepad-warning-icon {
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  margin: 0 auto 12px;
  border: 1px solid var(--warn);
  border-radius: 50%;
  color: var(--warn);
  font-size: 18px;
}
.gamepad-warning p {
  margin: 0 auto 16px;
  max-width: 300px;
}
.gamepad-warning button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}
.gamepad-warning .gamepad-error {
  margin-top: 12px;
  margin-bottom: 0;
  color: var(--error);
}
.install-spinner {
  width: 16px;
  height: 16px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: install-spin 0.8s linear infinite;
}
@keyframes install-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .install-spinner {
    animation: none;
  }
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
  grid-template-columns: minmax(0, 1fr) 300px;
  grid-template-rows: minmax(0, 1fr);
  gap: 20px;
  min-height: 0;
  flex: 1;
  align-items: stretch;
}
.gamepad-stage,
.gamepad-editor {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 7px;
}
.gamepad-stage {
  padding: 12px;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}
.gamepad-editor {
  padding: 18px;
  min-height: 0;
  overflow-y: auto;
}
.assignment {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr);
  gap: 6px;
  align-items: center;
  margin: 12px 0;
  font-size: 13px;
}
.assignment-input {
  display: flex;
  align-items: center;
  gap: 6px;
}
.maximum-depth {
  grid-column: 1 / -1;
  display: grid;
  gap: 3px;
  color: var(--muted);
  font-size: 11px;
}
.maximum-depth span {
  display: flex;
  justify-content: space-between;
}
.maximum-depth input {
  width: 100%;
  margin: 0;
}
.gamepad-editor:has(.maximum-depth) .assignment {
  margin: 6px 0;
}
.gamepad-editor:has(.maximum-depth) .input-help {
  margin-top: 8px;
  padding-top: 8px;
}
.gamepad-editor:has(.maximum-depth) p {
  margin-bottom: 6px;
}
.record-key {
  width: 100%;
  min-width: 0;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.assignment-input .capturing {
  border-color: var(--accent);
  color: var(--accent);
}
.input-help {
  border-top: 1px solid var(--line);
  margin-top: 16px;
  padding-top: 14px;
}
.input-help p {
  line-height: 1.5;
  margin-bottom: 10px;
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
