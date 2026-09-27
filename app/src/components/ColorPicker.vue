<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import type { Rgb } from '../types'

const props = defineProps<{ modelValue: Rgb | null; disabled: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [rgb: Rgb | null] }>()

const PRESETS = [
  '#ff0000', '#ff8000', '#ffff00', '#44d62c', '#00ff80', '#00ffff',
  '#0040ff', '#8000ff', '#ff00ff', '#ff4080', '#ffffff', '#ffc080',
]

const open = ref(false)
const root = ref<HTMLElement | null>(null)
// Colour to come back to after "no colour" is switched off again.
const last = ref<Rgb>(props.modelValue ?? [0x44, 0xd6, 0x2c])
watch(
  () => props.modelValue,
  v => v && (last.value = v),
)

const hex = (rgb: Rgb) => '#' + rgb.map(c => c.toString(16).padStart(2, '0')).join('')
const rgbOf = (h: string) => [1, 3, 5].map(i => Number.parseInt(h.slice(i, i + 2), 16)) as Rgb

const pick = (h: string) => emit('update:modelValue', rgbOf(h))
const toggleNone = () => emit('update:modelValue', props.modelValue ? null : last.value)

const onOutside = (e: MouseEvent) => root.value && !root.value.contains(e.target as Node) && (open.value = false)
watch(open, o => (o ? document.addEventListener('mousedown', onOutside) : document.removeEventListener('mousedown', onOutside)))
watch(
  () => props.disabled,
  d => d && (open.value = false),
)
onUnmounted(() => document.removeEventListener('mousedown', onOutside))
</script>

<template>
  <span ref="root" class="cp">
    <button
      class="sw"
      :class="{ empty: !modelValue }"
      :style="{ background: hex(last) }"
      :disabled="disabled"
      :title="modelValue ? 'Цвет' : 'Нет цвета'"
      :aria-label="modelValue ? 'Цвет ' + hex(modelValue) : 'Нет цвета'"
      @click="open = !open"
    ></button>
    <div v-if="open" class="pop" role="dialog" aria-label="Выбор цвета" @keydown.esc="open = false">
      <div class="grid">
        <button v-for="c in PRESETS" :key="c" class="preset" :style="{ background: c }" :aria-label="c" @click="pick(c)"></button>
      </div>
      <div class="actions">
        <label class="custom">
          Другой…
          <input type="color" :value="hex(last)" @input="pick(($event.target as HTMLInputElement).value)" />
        </label>
        <button class="none" :class="{ on: !modelValue }" :aria-pressed="!modelValue" @click="toggleNone">⊘ Без цвета</button>
      </div>
    </div>
  </span>
</template>

<style scoped>
.cp { position: relative; display: inline-flex; }
.sw { width: 36px; height: 30px; padding: 0; border: 1px solid #555; border-radius: 6px; position: relative; overflow: hidden; }
.sw.empty { opacity: 0.35; }
.sw.empty::after { content: ''; position: absolute; left: -4px; top: 50%; width: 44px; border-top: 2px solid var(--error); transform: rotate(-35deg); }
.pop { position: absolute; top: 36px; left: 0; z-index: 10; background: var(--panel); border: 1px solid #3a3a3a; border-radius: 8px; padding: 10px; width: 200px; box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5); }
.grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 6px; }
.preset { width: 26px; height: 26px; padding: 0; border: 1px solid #555; border-radius: 5px; }
.actions { display: flex; justify-content: space-between; align-items: center; margin-top: 10px; gap: 6px; }
.custom { position: relative; padding: 4px 8px; border: 1px solid #3a3a3a; border-radius: 6px; background: var(--key); cursor: pointer; font-size: 12px; }
.custom input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
.none { font-size: 12px; padding: 4px 8px; }
.none.on { background: var(--error); border-color: var(--error); color: #fff; }
</style>
