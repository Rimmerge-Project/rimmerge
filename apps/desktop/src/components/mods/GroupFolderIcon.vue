<script setup lang="ts">
import { computed } from "vue";

import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import { COLOR_TEXT_CLASS } from "@/utils/organization";

/**
 * A group's folder: filled and tinted in the group's colour, or only outlined when it has none, so
 * "no colour" has a shape cue and not just a missing tint. Decoration only (the name and the
 * colour word are text beside it), hence `aria-hidden`.
 */
const { color } = defineProps<{ color: ColorTokenDto | null }>();

const tint = computed(() => (color === null ? "text-text-muted" : COLOR_TEXT_CLASS[color]));
</script>

<template>
  <svg
    class="size-4 shrink-0"
    :class="tint"
    viewBox="0 0 24 24"
    :fill="color === null ? 'none' : 'currentColor'"
    stroke="currentColor"
    stroke-width="2"
    stroke-linejoin="round"
    aria-hidden="true"
    :data-color="color ?? undefined"
    data-testid="group-folder-icon"
  >
    <path d="M3 7a2 2 0 0 1 2-2h4.2a2 2 0 0 1 1.5.7L12 7h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" />
  </svg>
</template>
