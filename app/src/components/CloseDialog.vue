<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import type { CloseAction } from '../types'

const emit = defineEmits<{ choose: [action: CloseAction, remember: boolean]; cancel: [] }>()
const remember = ref(false)
const tray = ref<HTMLButtonElement | null>(null)
const onKey = (e: KeyboardEvent) => e.key === 'Escape' && emit('cancel')

onMounted(() => {
  window.addEventListener('keydown', onKey)
  tray.value?.focus()
})
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="backdrop" @click.self="emit('cancel')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="cd-title">
      <h3 id="cd-title">Закрыть окно</h3>
      <p>В трее EasyRazer продолжит следить за клавиатурой и вернёт подсветку после переподключения.</p>
      <label><input v-model="remember" type="checkbox" /> Больше не спрашивать</label>
      <div class="btns">
        <button @click="emit('choose', 'exit', remember)">Закрыть программу</button>
        <button ref="tray" class="primary" @click="emit('choose', 'tray', remember)">Свернуть в трей</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; }
.dlg { background: var(--panel); border: 1px solid #3a3a3a; border-radius: 10px; padding: 18px; max-width: 380px; }
.dlg h3 { margin: 0 0 8px; font-size: 15px; }
.dlg label { display: block; margin-top: 10px; color: var(--muted); }
.btns { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
</style>
