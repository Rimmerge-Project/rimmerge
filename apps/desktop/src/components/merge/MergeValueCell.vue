<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

/**
 * One value cell in the merge field table: a leaf's text or a `<pre>` of
 * an XML fragment (a `li` list item, or a tag-keyed map's own key),
 * highlighted when it differs
 * from the row's base value — the same diff signal `MergeFieldDto.class`/
 * `changedBy` already carry, rendered per-cell so a reader can see *which*
 * owner changed without cross-referencing the row's badge.
 *
 * An XML value longer than {@link COLLAPSE_THRESHOLD} renders collapsed (a
 * short, truncated preview) behind an expand toggle rather than the whole
 * subtree at once — a single container field (e.g. `wildAnimals`) can
 * carry base + several candidates + a result, each a multi-KB XML blob,
 * and rendering every one of those in full for every visible row was
 * real, measured rendering cost on the merge page. Collapsed by
 * default; a prior expand choice resets whenever `value` itself changes
 * (a different row, or the same row's value moving on) rather than
 * carrying over to unrelated content.
 */
const {
  value,
  base = null,
  isXml = false,
} = defineProps<{
  value: string | null;
  /** The row's base value, to diff `value` against. `undefined`/`null` disables highlighting (e.g. the base cell itself). */
  base?: string | null;
  /**
   * Whether `value` is an XML fragment (a `li` item, or a `MergeFieldDto`
   * whose `entry !== "leaf"`) rather than plain leaf text — renders in a
   * scrolling `<pre>` instead of a truncated single-line `<span>`.
   */
  isXml?: boolean;
}>();

/** Characters, not lines — a XML fragment's own tags dominate the length long before line count would. */
const COLLAPSE_THRESHOLD = 240;

const { t } = useI18n();

const differsFromBase = computed(() => base !== null && value !== base);
const isLong = computed(() => isXml && (value?.length ?? 0) > COLLAPSE_THRESHOLD);
const expanded = ref(false);
watch(
  () => value,
  () => {
    expanded.value = false;
  },
);
const displayText = computed(() => {
  if (value === null || !isXml || expanded.value || !isLong.value) {
    return value;
  }
  return `${value.slice(0, COLLAPSE_THRESHOLD)}…`;
});
</script>

<template>
  <span
    v-if="value === null"
    class="text-text-faint text-xs italic"
  >{{ t("merge.value.absent") }}</span>
  <div
    v-else-if="isXml"
    class="max-w-full"
  >
    <pre
      class="bg-surface-2 max-w-full overflow-x-auto rounded p-1 text-xs"
      :class="differsFromBase ? 'text-accent' : 'text-text-muted'"
    >{{ displayText }}</pre>
    <button
      v-if="isLong"
      type="button"
      class="text-text-faint mt-0.5 cursor-pointer text-[10px] underline focus-visible:outline"
      data-testid="merge-value-toggle"
      @click="expanded = !expanded"
    >
      {{ expanded ? t("merge.value.showLess") : t("merge.value.showMore") }}
    </button>
  </div>
  <span
    v-else
    class="block truncate text-sm"
    :class="differsFromBase ? 'text-text font-medium' : 'text-text-muted'"
    :title="value"
  >{{ value }}</span>
</template>
