<script setup lang="ts">
import { computed, useId } from "vue";
import { useI18n } from "vue-i18n";
import { gamepadState, stickDisplay } from "../gamepad";
import type { GamepadBinding, GamepadConfig } from "../gamepad";

const props = defineProps<{ config: GamepadConfig; depth: number[]; selected: GamepadBinding | "left" | "right" | null }>();
const emit = defineEmits<{ select: [control: GamepadBinding | "left" | "right"] }>();
const { t } = useI18n();
const uid = useId().replace(/:/g, "");
const state = computed(() => gamepadState(props.config.bindings, props.depth));
const sticks = computed(() => [
  { id: "left" as const, cx: 249, cy: 350, label: "L", axes: state.value.left, ...stickDisplay(state.value.left) },
  { id: "right" as const, cx: 478, cy: 474, label: "R", axes: state.value.right, ...stickDisplay(state.value.right) },
]);
const faces = [
  { id: "y", x: 600, y: 314 },
  { id: "x", x: 552, y: 362 },
  { id: "b", x: 648, y: 362 },
  { id: "a", x: 600, y: 410 },
] as const;
const dpad = [
  { id: "dpad_up", x: 319, y: 467, glyph: "⌃" },
  { id: "dpad_down", x: 319, y: 535, glyph: "⌄" },
  { id: "dpad_left", x: 285, y: 501, glyph: "‹" },
  { id: "dpad_right", x: 353, y: 501, glyph: "›" },
] as const;
const shoulders = [
  { id: "lb", trigger: "lt", x: 180, mirror: false },
  { id: "rb", trigger: "rt", x: 620, mirror: true },
] as const;
const ticks = Array.from({ length: 41 }, (_, i) => i - 20);
const active = (id: GamepadBinding) => ({ selected: props.selected === id, pressed: state.value.buttons[id] });
const choose = (id: GamepadBinding | "left" | "right", e: KeyboardEvent) => {
  if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    emit("select", id);
  }
};
</script>

<template>
  <svg class="controller" viewBox="0 0 800 690" :aria-label="t('gamepad.title')" role="group">
    <defs>
      <linearGradient :id="`${uid}-shell`" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="var(--key)" />
        <stop offset="1" stop-color="var(--panel)" />
      </linearGradient>
    </defs>
    <g v-for="s in shoulders" :key="s.id" :transform="s.mirror ? 'translate(800 0) scale(-1 1)' : ''">
      <g
        class="control shoulder"
        :class="active(s.id)"
        role="button"
        tabindex="0"
        :aria-label="t(`gamepad.${s.id}`)"
        :aria-pressed="selected === s.id"
        @click="emit('select', s.id)"
        @keydown="choose(s.id, $event)"
      >
        <path d="M158 142 Q206 139 253 127 Q276 146 300 146 L300 211 L263 211 Q176 211 158 142 Z" />
        <path d="M163 275 Q165 243 231 235 Q258 230 272 242 L300 252 L275 280 Z" />
        <text x="229" y="181" :transform="s.mirror ? 'translate(458 0) scale(-1 1)' : ''">{{ s.id.toUpperCase() }}</text>
      </g>
      <g
        class="control trigger"
        :class="active(s.trigger)"
        role="button"
        tabindex="0"
        :aria-label="t(`gamepad.${s.trigger}`)"
        :aria-pressed="selected === s.trigger"
        @click="emit('select', s.trigger)"
        @keydown="choose(s.trigger, $event)"
      >
        <clipPath :id="`${uid}-${s.trigger}`"><path d="M159 136 L159 92 Q160 47 193 44 Q239 40 249 124 Z" /></clipPath>
        <path d="M159 136 L159 92 Q160 47 193 44 Q239 40 249 124 Z" />
        <rect
          class="trigger-fill"
          x="153"
          :y="140 - state[s.trigger] * 100"
          width="104"
          :height="state[s.trigger] * 100"
          :clip-path="`url(#${uid}-${s.trigger})`"
        />
        <text x="202" y="99" :transform="s.mirror ? 'translate(404 0) scale(-1 1)' : ''">{{ s.trigger.toUpperCase() }}</text>
      </g>
      <g class="trigger-ruler" aria-hidden="true">
        <path d="M137 61 V133 M132 61 H141 M134 79 H141 M132 97 H141 M134 115 H141 M132 133 H141" />
        <rect x="145" y="61" width="4" height="72" class="track" />
        <rect x="145" :y="133 - state[s.trigger] * 72" width="4" :height="state[s.trigger] * 72" class="meter" />
        <text x="139" y="47" :transform="s.mirror ? 'translate(278 0) scale(-1 1)' : ''">{{ Math.round(state[s.trigger] * 100) }}%</text>
        <text
          v-for="(value, i) in [100, 50, 0]"
          :key="value"
          x="118"
          :y="61 + i * 36"
          :transform="s.mirror ? 'translate(236 0) scale(-1 1)' : ''"
        >
          {{ value }}
        </text>
      </g>
    </g>
    <path
      class="shell"
      :fill="`url(#${uid}-shell)`"
      d="M267 252 Q207 256 170 294 Q132 333 108 409 Q73 518 92 613 Q99 655 126 654 Q142 654 165 632 L251 585 Q270 567 303 567 L497 567 Q530 567 549 585 L635 632 Q658 654 674 654 Q701 655 708 613 Q727 518 692 409 Q668 333 630 294 Q593 256 533 252 Q400 240 267 252 Z"
    />
    <g class="grip" aria-hidden="true">
      <path v-for="n in 4" :key="n" :d="`M${109 + n * 5} ${554 + n * 8} Q120 584 ${144 + n * 5} ${606 + n * 4}`" />
      <path v-for="n in 4" :key="n" :d="`M${691 - n * 5} ${554 + n * 8} Q680 584 ${656 - n * 5} ${606 + n * 4}`" />
    </g>
    <g class="emblem" aria-hidden="true">
      <path d="M400 278 L421 290 V315 L400 328 L379 315 V290 Z" />
      <path d="M390 295 H410 L398 303 H409 L391 314" />
    </g>
    <g
      v-for="f in faces"
      :key="f.id"
      class="control face"
      :class="active(f.id)"
      role="button"
      tabindex="0"
      :aria-label="t(`gamepad.${f.id}`)"
      :aria-pressed="selected === f.id"
      @click="emit('select', f.id)"
      @keydown="choose(f.id, $event)"
    >
      <circle :cx="f.x" :cy="f.y" r="22" />
      <text :x="f.x" :y="f.y + 1">{{ f.id.toUpperCase() }}</text>
    </g>
    <g
      v-for="(id, i) in ['back', 'start'] as const"
      :key="id"
      class="control"
      :class="active(id)"
      role="button"
      tabindex="0"
      :aria-label="t(`gamepad.${id}`)"
      :aria-pressed="selected === id"
      @click="emit('select', id)"
      @keydown="choose(id, $event)"
    >
      <circle :cx="350 + i * 100" cy="364" r="16" />
      <path v-if="id === 'back'" d="M343 359 H353 V367 H343 Z M347 355 H357 V363" class="glyph" />
      <path v-else d="M443 359 H457 M443 364 H457 M443 369 H457" class="glyph" />
    </g>
    <g
      v-for="d in dpad"
      :key="d.id"
      class="control dpad"
      :class="active(d.id)"
      role="button"
      tabindex="0"
      :aria-label="t(`gamepad.${d.id}`)"
      :aria-pressed="selected === d.id"
      @click="emit('select', d.id)"
      @keydown="choose(d.id, $event)"
    >
      <rect :x="d.x - 15" :y="d.y - 15" width="30" height="30" rx="3" />
      <text :x="d.x" :y="d.y">{{ d.glyph }}</text>
    </g>
    <g
      v-for="s in sticks"
      :key="s.id"
      :transform="`translate(${s.cx} ${s.cy})`"
      class="stick control"
      :class="{ selected: selected === s.id || selected?.startsWith(`${s.id}_`), pressed: state.buttons[`${s.id}_click`] }"
      role="button"
      tabindex="0"
      :aria-label="t(`gamepad.${s.id}Stick`)"
      :aria-pressed="selected === s.id || selected?.startsWith(`${s.id}_`)"
      @click="emit('select', s.id)"
      @keydown="choose(s.id, $event)"
    >
      <circle r="46" class="socket" />
      <circle r="37" class="rim" />
      <g class="telemetry" :opacity="s.opacity" aria-hidden="true">
        <circle r="73" class="dial" />
        <path v-for="n in 48" :key="n" :d="`M0 -${n % 4 === 0 ? 65 : 69} V-73`" :transform="`rotate(${n * 7.5})`" />
        <path d="M-82 0 H82 M0 -82 V82" class="axis" />
        <svg x="-103" y="-61" width="25" height="122" viewBox="0 0 25 122" overflow="hidden">
          <g :transform="`translate(0 ${61 - s.axes.y * 61})`">
            <g v-for="tick in ticks" :key="tick" :transform="`translate(0 ${tick * 6.1})`">
              <path :d="`M${tick % 5 === 0 ? 18 : 22} 0 H25`" />
              <text v-if="tick % 5 === 0 && Math.abs(tick) <= 10" x="8" y="0">{{ (tick / 10).toFixed(1) }}</text>
            </g>
          </g>
        </svg>
        <svg x="-61" y="78" width="122" height="22" viewBox="0 0 122 22" overflow="hidden">
          <g :transform="`translate(${61 - s.axes.x * 61} 0)`">
            <g v-for="tick in ticks" :key="tick" :transform="`translate(${tick * 6.1} 0)`">
              <path :d="`M0 0 V${tick % 5 === 0 ? 7 : 3}`" />
              <text v-if="tick % 5 === 0 && Math.abs(tick) <= 10" x="0" y="16">{{ (tick / 10).toFixed(1) }}</text>
            </g>
          </g>
        </svg>
        <path d="M-77 -3 L-81 0 L-77 3 M-3 77 L0 81 L3 77" class="reticle" />
        <rect x="-104" y="-94" width="38" height="14" rx="2" class="value-box" />
        <rect x="67" y="82" width="38" height="14" rx="2" class="value-box" />
        <text x="-85" y="-88" class="readout">{{ s.axes.y.toFixed(2) }}</text>
        <text x="86" y="90" class="readout">{{ s.axes.x.toFixed(2) }}</text>
        <text x="-67" y="90">{{ s.angle }}°</text>
        <text x="68" y="-80">{{ Math.round(s.magnitude * 100) }}%</text>
        <path :d="`M0 0 L${s.x * 40} ${s.y * 40}`" class="vector" />
        <path :d="`M${s.x * 40 - 9} ${s.y * 40} h18 M${s.x * 40} ${s.y * 40 - 9} v18`" class="reticle" />
      </g>
      <g :transform="`translate(${s.x * 29} ${s.y * 29})`">
        <circle r="25" class="knob" />
        <circle r="19" class="knob-rim" />
        <text y="1">{{ s.label }}</text>
      </g>
    </g>
  </svg>
</template>

<style scoped>
.controller {
  display: block;
  width: 100%;
  height: auto;
  overflow: visible;
}
.shell {
  stroke: var(--line);
  stroke-width: 1.8;
}
.control {
  cursor: pointer;
  outline: none;
}
.control > path,
.control > circle,
.control > rect,
.knob {
  fill: var(--key);
  stroke: #666;
  stroke-width: 1.6;
}
.control:hover > path,
.control:hover > circle,
.control:hover > rect {
  stroke: var(--text);
}
.control.selected > path,
.control.selected > circle,
.control.selected > rect,
.control:focus-visible > path,
.control:focus-visible > circle,
.control:focus-visible > rect {
  stroke: var(--accent);
  stroke-width: 2.5;
}
.control.pressed > path,
.control.pressed > circle,
.control.pressed > rect,
.stick.pressed .knob {
  fill: color-mix(in srgb, var(--accent) 28%, var(--key));
  stroke: var(--accent);
}
.control:focus-visible > path,
.control:focus-visible > circle,
.control:focus-visible > rect {
  stroke: var(--text);
  stroke-width: 3;
}
text {
  fill: var(--text);
  font:
    14px system-ui,
    sans-serif;
  text-anchor: middle;
  dominant-baseline: middle;
  pointer-events: none;
}
.glyph {
  fill: none !important;
  stroke: var(--muted) !important;
  stroke-width: 1.5 !important;
}
.grip path {
  stroke: var(--line);
  stroke-width: 2.5;
  fill: none;
}
.emblem path {
  stroke: var(--muted);
  fill: none;
  stroke-width: 1.5;
  opacity: 0.6;
}
.emblem path + path {
  stroke-width: 2.5;
}
.trigger-fill {
  fill: var(--accent) !important;
  stroke: none !important;
  opacity: 0.35;
  pointer-events: none;
}
.trigger-ruler {
  fill: none;
  stroke: var(--muted);
  stroke-width: 1;
}
.trigger-ruler text {
  stroke: none;
  font:
    10px ui-monospace,
    monospace;
}
.trigger-ruler .track {
  fill: var(--line);
  stroke: none;
}
.trigger-ruler .meter {
  fill: var(--accent);
  stroke: none;
}
.dpad text {
  font-size: 24px;
  fill: var(--muted);
}
.stick .socket {
  fill: var(--bg);
}
.stick .rim {
  fill: none;
  stroke: var(--line);
}
.knob-rim {
  fill: none;
  stroke: var(--line);
}
.stick.pressed .knob-rim {
  stroke: var(--accent);
}
.telemetry {
  pointer-events: none;
}
.telemetry path,
.telemetry circle {
  fill: none;
  stroke: var(--muted);
  stroke-width: 0.7;
}
.telemetry .axis {
  stroke-dasharray: 3 4;
  opacity: 0.6;
}
.telemetry .vector,
.telemetry .reticle {
  stroke: var(--accent);
  stroke-width: 1;
}
.telemetry text {
  font:
    9px ui-monospace,
    monospace;
}
</style>
