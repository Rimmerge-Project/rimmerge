<script setup lang="ts">
import { useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { useClampedText } from "@/composables/useClampedText";
import type { DescriptionDto } from "@/types/generated/DescriptionDto";

/**
 * Renders a mod's sanitized description runs as plain text spans —
 * **never** `v-html`. `run.text` reaches the DOM only through ordinary
 * `{{ }}` interpolation, which Vue escapes automatically; BBCode and any
 * unmatched/unrecognized tag (including a hostile `<script>`/`<img
 * onerror>`) arrive as literal text in a run and render as inert text,
 * exactly as RimWorld itself shows them. `components/mods/no-v-html.guard.test.ts`
 * enforces that no file under this directory ever adds one.
 */
const { description } = defineProps<{ description: DescriptionDto | null }>();
const { t } = useI18n();

const textElement = useTemplateRef<HTMLElement>("textElement");
// The toggle only appears when the clamp really hides lines: many
// descriptions are a single word.
const { expanded, showToggle } = useClampedText(textElement, () =>
  description?.runs.map((run) => run.text).join(" "),
);
</script>

<template>
  <div data-testid="mod-description">
    <p
      v-if="!description || description.runs.length === 0"
      class="text-text-faint text-sm"
      data-testid="mod-description-empty"
    >
      {{ t("modInfo.panel.noDescription") }}
    </p>
    <template v-else>
      <p
        ref="textElement"
        class="text-text-muted [overflow-wrap:anywhere] text-sm whitespace-pre-line"
        :class="expanded ? '' : 'line-clamp-12'"
        data-testid="mod-description-text"
      >
        <span
          v-for="(run, index) in description.runs"
          :key="index"
          :class="[run.bold ? 'font-bold' : '', run.italic ? 'italic' : '']"
        >{{ run.text }}</span>
      </p>
      <button
        v-if="showToggle"
        type="button"
        :aria-expanded="expanded"
        class="text-accent cursor-pointer text-xs underline"
        data-testid="mod-description-toggle"
        @click="expanded = !expanded"
      >
        {{ expanded ? t("modInfo.panel.showLess") : t("modInfo.panel.showMore") }}
      </button>
      <p
        v-if="description.truncated"
        class="text-text-faint text-xs"
        data-testid="mod-description-truncated"
      >
        {{ t("modInfo.panel.descriptionTruncated") }}
      </p>
    </template>
  </div>
</template>
