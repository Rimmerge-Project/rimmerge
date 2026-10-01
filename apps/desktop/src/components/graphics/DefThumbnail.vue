<script setup lang="ts">
import { useElementVisibility } from "@vueuse/core";
import { useTemplateRef } from "vue";

import DefThumbnailBody from "@/components/graphics/DefThumbnailBody.vue";
import { useScrollContainer } from "@/composables/scrollContainer";
import type { DefRef } from "@/types/brands";

/**
 * A small preview of what a def shows: its default view, with no controls.
 * It asks the backend for nothing until its box is near the viewport (the
 * coverage list is not virtualized, so hundreds of these exist at once),
 * and its queries are released again when it scrolls away, so memory
 * follows what was recently on screen (near is measured against the
 * enclosing {@link useScrollContainer}). The caller sizes it
 * (`class="h-10 w-10 shrink-0"`); it is decorative (`aria-hidden`)
 * because the row it sits in already names the def in text.
 */
const { defRef } = defineProps<{ defRef: DefRef }>();

const root = useTemplateRef<HTMLElement>("root");
const scrollTarget = useScrollContainer();
// The 200 px margin is measured against the list's own scroller, not the window.
const isNearViewport = useElementVisibility(root, { rootMargin: "200px", scrollTarget });
</script>

<template>
  <div
    ref="root"
    class="bg-surface-2 overflow-hidden rounded-sm"
    aria-hidden="true"
    data-testid="def-thumbnail"
  >
    <DefThumbnailBody
      v-if="isNearViewport"
      :def-ref="defRef"
    />
  </div>
</template>
