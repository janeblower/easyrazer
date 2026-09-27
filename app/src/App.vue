<script setup>
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const status = ref(null)

onMounted(async () => {
  status.value = await invoke('status', { checkSynapse: true })
})
</script>

<template>
  <pre>{{ status }}</pre>
</template>
