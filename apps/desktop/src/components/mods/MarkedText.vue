<script setup lang="ts">
import { computed } from "vue";

import type { TextRange } from "@/utils/textMarks";
import { splitMarked } from "@/utils/textMarks";

/**
 * A text with the parts a search matched marked, drawn as plain text runs: a matched run is a
 * `<span>` with a background and an underline (so it does not depend on colour alone), never
 * `<mark>` and never HTML. `<mark>` has the `mark` role, which screen readers may announce around
 * every matched row; a styled span is read as the one continuous text it is. The text itself is
 * unchanged, so its accessible name and `textContent` are the label.
 */
const { text, ranges = [] } = defineProps<{
  text: string;
  /** The backend's matches, in UTF-16 code units of {@link text}. */
  ranges?: readonly TextRange[];
}>();

const segments = computed(() => splitMarked(text, ranges));
</script>

<template>
  <template
    v-for="(segment, index) in segments"
    :key="index"
  >
    <span
      v-if="segment.isMarked"
      class="bg-search-match underline decoration-2 underline-offset-2"
      data-testid="search-match"
    >{{ segment.text }}</span>
    <template v-else>
      {{ segment.text }}
    </template>
  </template>
</template>
