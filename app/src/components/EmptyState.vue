<script setup lang="ts">
import AppIcon from "./AppIcon.vue";

defineProps<{ icon: "keyboard" | "macro"; title: string; text: string }>();
</script>

<template>
  <div class="px-4 py-6 text-center flex flex-col gap-2 items-center">
    <span class="text-xl text-muted rounded-[10px] bg-key flex h-11 w-11 items-center justify-center"><AppIcon :name="icon" /></span>
    <span>{{ title }}</span>
    <span class="hint max-w-[400px]">{{ text }}</span>
    <slot />
  </div>
</template>
