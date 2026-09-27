<script setup>
import { ref, onMounted, onUnmounted } from 'vue'

const emit = defineEmits(['answer', 'later'])
const yes = ref(null)
const onKey = e => e.key === 'Escape' && emit('later')

onMounted(() => {
  window.addEventListener('keydown', onKey)
  yes.value.focus()
})
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="backdrop" @click.self="emit('later')">
    <div class="dlg" role="dialog" aria-modal="true" aria-labelledby="ao-title">
      <h3 id="ao-title">Запускать EasyRazer вместе с Windows?</h3>
      <p>Программа будет в трее и сама вернёт подсветку.</p>
      <div class="btns">
        <button @click="emit('answer', false)">Не надо</button>
        <button ref="yes" class="primary" @click="emit('answer', true)">Включить</button>
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
