<script setup lang="ts">
defineProps<{ name: keyof typeof PATHS }>();

const PATHS = {
  revert: ["M9 14 4 9l5-5", "M4 9h11a5 5 0 0 1 0 10h-3"],
  write: ["M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M7 3v5h8V3", "M7 21v-7h10v7"],
  random: [
    "M2 18h1.4c1.3 0 2.5-.6 3.3-1.7l6.1-8.6c.7-1.1 2-1.7 3.3-1.7H22",
    "m18 2 4 4-4 4",
    "M2 6h1.9c1.5 0 2.9.9 3.6 2.2",
    "M22 18h-5.9c-1.3 0-2.6-.7-3.3-1.8l-.5-.8",
    "m18 14 4 4-4 4",
  ],
  up: ["m6 15 6-6 6 6"],
  down: ["m6 9 6 6 6-6"],
  remove: ["M18 6 6 18", "m6 6 12 12"],
};
</script>

<template>
  <svg
    viewBox="0 0 24 24"
    width="18"
    height="18"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path v-for="d in PATHS[name]" :key="d" :d="d" />
  </svg>
</template>
