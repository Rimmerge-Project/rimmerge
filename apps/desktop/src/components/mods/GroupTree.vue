<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import {
  COLOR_NAME_KEY,
  COLOR_SWATCH_CLASS,
  type GroupScopeChoice,
  isSameScope,
  visibleNodes,
} from "@/utils/organization";

/**
 * The group tree, following the WAI-ARIA tree pattern: one roving tab stop; Up/Down move,
 * Right expands or enters a group, Left collapses or goes to the parent, Home/End jump, typing
 * a name jumps to it (a space continues the name), Enter or a space outside a name scopes the
 * lists to the group, F2 renames, Delete deletes,
 * and Shift+F10 or the ContextMenu key opens the group menu. Two fixed entries come first:
 * "All mods" (no scope) and "Ungrouped". Counts include subgroups and say so for screen readers.
 * Colour is only ever an extra cue: every group is named in text.
 */
const { nodes, shown } = defineProps<{
  /** Every group, depth first, siblings by name. */
  nodes: readonly GroupNodeDto[];
  /** The tree search result (matches and their ancestors), or `null` while not searching. */
  shown: ReadonlySet<number> | null;
}>();
const emit = defineEmits<{
  /** Shift+F10, the ContextMenu key or a right click: open the menu of this group. */
  menu: [event: Event, groupId: number];
  rename: [groupId: number];
  delete: [groupId: number];
}>();
const scope = defineModel<GroupScopeChoice>("scope", { required: true });
const expanded = defineModel<ReadonlySet<number>>("expanded", { required: true });

const { t } = useI18n();

/** One drawn entry: a fixed pseudo entry or a group. */
type Entry =
  | { readonly key: "all" | "ungrouped"; readonly choice: GroupScopeChoice; readonly node: null }
  | { readonly key: string; readonly choice: GroupScopeChoice; readonly node: GroupNodeDto };

const entries = computed<Entry[]>(() => [
  { key: "all", choice: { kind: "all" }, node: null },
  { key: "ungrouped", choice: { kind: "ungrouped" }, node: null },
  ...visibleNodes(nodes, expanded.value, shown).map(
    (node): Entry => ({
      key: `group-${node.id}`,
      choice: { kind: "group", id: node.id },
      node,
    }),
  ),
]);

const focusedKey = ref<string>("all");

/**
 * When the group that holds the tab stop disappears (it was deleted), the stop moves to its next
 * sibling, else its parent, else "All mods", so focus lands near where the user was instead of
 * jumping back to the top. Only the stop moves here; `focusCurrent` puts keyboard focus on it once
 * the dialog that caused the deletion is gone.
 */
watch(
  () => nodes,
  (current, previous) => {
    const lost = previous.find((node) => `group-${node.id}` === focusedKey.value);
    if (lost === undefined || current.some((node) => node.id === lost.id)) {
      return;
    }
    const siblings = previous.filter((node) => node.parent === lost.parent);
    const next = siblings[siblings.indexOf(lost) + 1];
    const parent = lost.parent === null ? undefined : `group-${lost.parent}`;
    const target = next === undefined ? parent : `group-${next.id}`;
    const stillThere = (key: string): boolean => current.some((node) => `group-${node.id}` === key);
    focusedKey.value = target !== undefined && stillThere(target) ? target : "all";
  },
);
/** The roving tab stop: the focused entry while it is drawn, else the first. */
const rovingKey = computed(() =>
  entries.value.some((entry) => entry.key === focusedKey.value) ? focusedKey.value : "all",
);

const treeElement = useTemplateRef<HTMLElement>("tree");

async function focusKey(key: string): Promise<void> {
  focusedKey.value = key;
  await nextTick();
  treeElement.value?.querySelector<HTMLElement>(`[data-key="${key}"]`)?.focus();
}

/** Puts keyboard focus on the roving tab stop. */
function focusCurrent(): Promise<void> {
  return focusKey(rovingKey.value);
}

function isExpanded(node: GroupNodeDto): boolean {
  return shown !== null || expanded.value.has(node.id);
}

function setExpanded(node: GroupNodeDto, isOpen: boolean): void {
  const next = new Set(expanded.value);
  if (isOpen) {
    next.add(node.id);
  } else {
    next.delete(node.id);
  }
  expanded.value = next;
}

function toggleExpanded(node: GroupNodeDto): void {
  setExpanded(node, !isExpanded(node));
}

function select(entry: Entry): void {
  scope.value = entry.choice;
}

/** Whether a drawn entry has no children drawn: a leaf, or a group nothing below matches. */
function hasDrawnChildren(node: GroupNodeDto): boolean {
  return node.childCount > 0 && entries.value.some((entry) => entry.node?.parent === node.id);
}

function nameOf(entry: Entry): string {
  if (entry.node !== null) {
    return entry.node.name;
  }
  return entry.key === "all" ? t("mods.groups.allMods") : t("mods.groups.ungrouped");
}

const TYPE_AHEAD_RESET_MS = 600;
let typeAhead = "";
let typeAheadTimer: ReturnType<typeof setTimeout> | undefined;

/** Jumps to the next entry whose name starts with the typed text. */
function typeAheadTo(character: string, from: number): void {
  clearTimeout(typeAheadTimer);
  typeAhead += character.toLowerCase();
  typeAheadTimer = setTimeout(() => {
    typeAhead = "";
  }, TYPE_AHEAD_RESET_MS);
  const all = entries.value;
  // A repeated single letter cycles; a longer prefix starts at the focused entry itself.
  const isCycling =
    typeAhead.length > 1 && [...typeAhead].every((letter) => letter === typeAhead[0]);
  const prefix = isCycling ? (typeAhead[0] ?? "") : typeAhead;
  const start = isCycling || typeAhead.length === 1 ? from + 1 : from;
  for (let step = 0; step < all.length; step += 1) {
    const candidate = all[(start + step) % all.length];
    if (candidate !== undefined && nameOf(candidate).toLowerCase().startsWith(prefix)) {
      void focusKey(candidate.key);
      return;
    }
  }
}

function moveBy(from: number, delta: number): void {
  const target = entries.value[Math.min(Math.max(from + delta, 0), entries.value.length - 1)];
  if (target !== undefined) {
    void focusKey(target.key);
  }
}

function onRight(entry: Entry, index: number): void {
  const node = entry.node;
  if (node === null || node.childCount === 0) {
    return;
  }
  if (!isExpanded(node)) {
    setExpanded(node, true);
  } else if (hasDrawnChildren(node)) {
    moveBy(index, 1);
  }
}

function onLeft(entry: Entry): void {
  const node = entry.node;
  if (node === null) {
    return;
  }
  if (node.childCount > 0 && isExpanded(node) && shown === null) {
    setExpanded(node, false);
    return;
  }
  const parent = entries.value.find((candidate) => candidate.node?.id === node.parent);
  if (parent !== undefined) {
    void focusKey(parent.key);
  }
}

function onKeydown(event: KeyboardEvent, entry: Entry, index: number): void {
  if (event.target instanceof HTMLElement && event.target.tagName === "INPUT") {
    return;
  }
  const node = entry.node;
  const isMenuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
  if (isMenuKey && node !== null) {
    event.preventDefault();
    emit("menu", event, node.id);
    return;
  }
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      moveBy(index, 1);
      return;
    case "ArrowUp":
      event.preventDefault();
      moveBy(index, -1);
      return;
    case "ArrowRight":
      event.preventDefault();
      onRight(entry, index);
      return;
    case "ArrowLeft":
      event.preventDefault();
      onLeft(entry);
      return;
    case "Home":
      event.preventDefault();
      moveBy(0, 0);
      return;
    case "End":
      event.preventDefault();
      moveBy(entries.value.length - 1, 0);
      return;
    case "Enter":
      event.preventDefault();
      select(entry);
      return;
    case " ":
      // A space inside a name being typed continues the search; otherwise it scopes like Enter.
      event.preventDefault();
      if (typeAhead === "") {
        select(entry);
      } else {
        typeAheadTo(" ", index);
      }
      return;
    case "F2":
      if (node !== null) {
        event.preventDefault();
        emit("rename", node.id);
      }
      return;
    case "Delete":
      if (node !== null) {
        event.preventDefault();
        emit("delete", node.id);
      }
      return;
    default:
      break;
  }
  const isPrintable = event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey;
  if (isPrintable) {
    typeAheadTo(event.key, index);
  }
}

function onContextMenu(event: MouseEvent, entry: Entry): void {
  if (entry.node !== null) {
    event.preventDefault();
    emit("menu", event, entry.node.id);
  }
}

function onItemClick(entry: Entry): void {
  focusedKey.value = entry.key;
  select(entry);
}

function swatchClass(node: GroupNodeDto): string {
  return node.color === null ? "border-border-strong border" : COLOR_SWATCH_CLASS[node.color];
}

defineExpose({ focusKey, focusCurrent });
</script>

<template>
  <div
    ref="tree"
    role="tree"
    class="flex min-h-0 flex-col gap-0.5 overflow-y-auto"
    :aria-label="t('mods.groups.treeLabel')"
    data-testid="group-tree"
  >
    <div
      v-for="(entry, index) in entries"
      :key="entry.key"
      role="treeitem"
      :tabindex="rovingKey === entry.key ? 0 : -1"
      :data-key="entry.key"
      :aria-level="entry.node === null ? 1 : entry.node.depth"
      :aria-selected="isSameScope(scope, entry.choice)"
      :aria-expanded="entry.node !== null && entry.node.childCount > 0 ? isExpanded(entry.node) : undefined"
      class="hover:bg-surface-2 focus-visible:outline-accent flex min-h-8 cursor-pointer items-center gap-2 rounded px-2 text-sm focus-visible:outline-2"
      :class="
        isSameScope(scope, entry.choice)
          ? 'bg-accent-soft text-text inset-ring-accent inset-ring-2 font-semibold'
          : 'text-text'
      "
      :style="{ paddingInlineStart: `${(entry.node === null ? 0 : entry.node.depth - 1) * 14 + 8}px` }"
      :data-testid="entry.node === null ? `group-node-${entry.key}` : `group-node-${entry.node.id}`"
      @click="onItemClick(entry)"
      @focus="focusedKey = entry.key"
      @keydown="onKeydown($event, entry, index)"
      @contextmenu="onContextMenu($event, entry)"
    >
      <template v-if="entry.node === null">
        <i
          :class="['pi text-xs', entry.key === 'all' ? 'pi-list' : 'pi-inbox']"
          aria-hidden="true"
        />
        <span class="min-w-0 flex-1 truncate">{{ nameOf(entry) }}</span>
      </template>
      <template v-else>
        <span
          class="inline-flex size-4 shrink-0 items-center justify-center"
          aria-hidden="true"
          :data-testid="`group-node-toggle-${entry.node.id}`"
          @click.stop="entry.node.childCount > 0 && shown === null && toggleExpanded(entry.node)"
        >
          <i
            v-if="entry.node.childCount > 0"
            :class="['pi text-xs', isExpanded(entry.node) ? 'pi-chevron-down' : 'pi-chevron-right']"
          />
        </span>
        <span
          class="size-3 shrink-0 rounded-full"
          :class="swatchClass(entry.node)"
          aria-hidden="true"
          :data-color="entry.node.color ?? undefined"
        />
        <span class="min-w-0 flex-1 truncate">{{ entry.node.name }}</span>
        <span
          class="text-text-muted shrink-0 text-xs font-normal"
          aria-hidden="true"
        >{{ entry.node.scopeCount }}</span>
        <span class="sr-only">{{
          t(
            "mods.groups.nodeModCount",
            { count: entry.node.scopeCount },
            entry.node.scopeCount,
          )
        }}</span>
        <span
          v-if="entry.node.childCount > 0"
          class="sr-only"
        >{{
          t("mods.groups.nodeSubgroupCount", { count: entry.node.childCount }, entry.node.childCount)
        }}</span>
        <span
          v-if="entry.node.color !== null"
          class="sr-only"
        >{{ t("mods.colors.rowColor", { name: t(COLOR_NAME_KEY[entry.node.color]) }) }}</span>
      </template>
    </div>
  </div>
</template>
