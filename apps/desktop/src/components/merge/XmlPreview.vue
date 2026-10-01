<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

/**
 * A read-only, line-numbered preview of one rendered file's text —
 * `MergeModPage`'s right pane. Plain text, not syntax-highlighted (YAGNI:
 * the content is always short, generated XML/JSON, and a highlighter
 * would be a new dependency for a page that exists to answer "does this
 * file exist and roughly what's in it"). A styled `<div>`, not a real
 * `<pre>` — `white-space: pre` on the line-text span alone preserves
 * indentation without opting this template into `<pre>`'s "preserve
 * every incidental whitespace/newline in the markup itself" compiler
 * behavior, which would otherwise inject stray characters into the
 * rendered line numbers.
 */
const { content } = defineProps<{ content: string }>();
const { t } = useI18n();

/**
 * One trailing newline stripped before splitting — every rendered file
 * ends with one (see `rimmergeJson`/`aboutXml`/`patchFileFor` and the
 * real renderer they mirror), and `"a\n".split("\n")` would otherwise
 * produce a spurious empty last line with its own line number.
 */
const lines = computed(() => content.replace(/\n$/, "").split("\n"));
</script>

<template>
  <div
    class="surface-card overflow-x-auto focus-visible:outline"
    tabindex="0"
    role="region"
    :aria-label="t('merge.filePreviewLabel')"
    data-testid="xml-preview"
  >
    <div class="grid min-w-max grid-cols-[auto_1fr] gap-x-3 p-3 font-mono text-xs">
      <template
        v-for="(line, index) in lines"
        :key="index"
      >
        <span class="text-text-faint text-right select-none">{{ index + 1 }}</span>
        <span class="text-text whitespace-pre">{{ line }}</span>
      </template>
    </div>
  </div>
</template>
