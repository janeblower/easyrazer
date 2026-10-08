<script setup lang="ts">
import type { System } from "../types";

const props = defineProps<{
  name:
    | "revert"
    | "write"
    | "random"
    | "up"
    | "down"
    | "remove"
    | "chip"
    | "rust"
    | "keyboard"
    | "mouse"
    | "music"
    | "macro"
    | "cog"
    | "ban"
    | System;
}>();

const ICONS = import.meta.glob<string>("../assets/icons/*.svg", { query: "?raw", import: "default", eager: true });
</script>

<template>
  <span class="app-icon" v-html="ICONS[`../assets/icons/${props.name}.svg`]" />
</template>

<style>
/* :where keeps the default size overridable by utility classes. */
:where(.app-icon) {
  width: 18px;
  height: 18px;
}
.app-icon {
  display: inline-flex;
}
.app-icon > svg {
  width: 100%;
  height: 100%;
}
</style>
