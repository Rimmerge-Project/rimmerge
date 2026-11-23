<script setup lang="ts">
import { computed, nextTick, onScopeDispose, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import GroupFolderIcon from "@/components/mods/GroupFolderIcon.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { assertNever } from "@/utils/assertNever";
import {
  GROUP_DRAG_TYPE,
  type ReparentRefusal,
  ROW_DRAG_TYPE,
  reparentRefusal,
} from "@/utils/groupDrop";
import {
  COLOR_NAME_KEY,
  findNode,
  type GroupScopeChoice,
  isSameScope,
  visibleNodes,
} from "@/utils/organization";
import { searchFold } from "@/utils/searchFold";

/**
 * The group tree, following the WAI-ARIA tree pattern: one roving tab stop; Up/Down move,
 * Right expands or enters a group, Left collapses or goes to the parent, Home/End jump, typing
 * a name jumps to it (a space continues the name), Enter or a space outside a name scopes the
 * lists to the group, F2 renames, Delete deletes,
 * and Shift+F10 or the ContextMenu key opens the group menu. Two fixed entries come first:
 * "All mods" (no scope) and "Ungrouped". Counts include subgroups and say so for screen readers.
 * Colour is only ever an extra cue: every group is named in text.
 *
 * It is also a drop target. Mods dragged from the lists (`rowDrag`) can be dropped on a group to
 * add them to it, and a group dragged from the tree can be dropped on another group to nest it,
 * or on "All mods" to make it top level. The hovered entry shows a dashed outline, an icon and a
 * sentence (never colour alone), a refused target says why, and a collapsed group opens after
 * hovering it for a moment. A drop is only taken when the drag carries this app's own drag type, so
 * a file or text dragged in from elsewhere is ignored even while a stale row drag is recorded.
 * The keyboard has the group menu's "Move group to..." for nesting, and the row menu's Groups
 * dialog for adding one mod; adding a whole selection from the keyboard comes with the selection
 * toolbar.
 */
const {
  nodes,
  shown,
  rowDrag = null,
  openModGroupIds = new Set<number>(),
} = defineProps<{
  /** Every group, depth first, siblings by name. */
  nodes: readonly GroupNodeDto[];
  /** The tree search result (matches and their ancestors), or `null` while not searching. */
  shown: ReadonlySet<number> | null;
  /** The mods being dragged from a list (and how many of them a filter hides), or `null`. */
  rowDrag?: { readonly count: number; readonly hiddenCount: number } | null;
  /** The groups the mod open in the info panel is a direct member of: drawn highlighted. */
  openModGroupIds?: ReadonlySet<number>;
}>();
const emit = defineEmits<{
  /** Shift+F10, the ContextMenu key or a right click: open the menu of this group. */
  menu: [event: Event, groupId: number];
  rename: [groupId: number];
  delete: [groupId: number];
  /** Mods were dropped on this group: add them to it. */
  dropMods: [groupId: number];
  /** A group was dropped on a group (or on "All mods", `null`): make it that parent. */
  dropGroup: [groupId: number, parent: number | null];
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
  typeAhead += searchFold(character);
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
    if (candidate !== undefined && searchFold(nameOf(candidate)).startsWith(prefix)) {
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

/** What dropping the drag in progress on an entry would do, or why it is refused. */
type DragKind = "mods" | "group";

type DropOutcome =
  | { readonly kind: "accept"; readonly label: string }
  | { readonly kind: "refuse"; readonly reason: string };

const HOVER_EXPAND_MS = 800;

/** The group being dragged out of the tree, if any. */
const draggedGroupId = ref<number | null>(null);
const hoverKey = ref<string | null>(null);
/** Which kind of drag hovers `hoverKey`. */
const hoverDragKind = ref<DragKind>("mods");
let expandTimer: ReturnType<typeof setTimeout> | undefined;
let expandKey: string | null = null;

function refusalText(refusal: ReparentRefusal): string {
  switch (refusal) {
    case "inside-itself":
      return t("mods.groups.dialog.moveInvalid");
    case "already-there":
      return t("mods.groups.drop.alreadyThere");
    default:
      return assertNever(refusal);
  }
}

/** `text` with the number of dragged mods a filter hides, when there are any. */
function withHidden(text: string, hiddenCount: number): string {
  return hiddenCount > 0
    ? t("mods.groups.drop.withHidden", {
        text,
        hidden: t("mods.selection.hiddenPart", { count: hiddenCount }, hiddenCount),
      })
    : text;
}

function modsOutcome(entry: Entry, drag: { count: number; hiddenCount: number }): DropOutcome {
  if (entry.node === null) {
    return { kind: "refuse", reason: t("mods.groups.drop.needsGroup") };
  }
  const label = t(
    "mods.groups.drop.addTo",
    { count: drag.count, name: entry.node.name },
    drag.count,
  );
  return { kind: "accept", label: withHidden(label, drag.hiddenCount) };
}

function groupOutcome(entry: Entry, dragged: GroupNodeDto): DropOutcome {
  if (entry.key === "ungrouped") {
    return { kind: "refuse", reason: t("mods.groups.drop.notUngrouped") };
  }
  const parent = entry.node?.id ?? null;
  const refusal = reparentRefusal(nodes, dragged.id, parent);
  if (refusal !== null) {
    return { kind: "refuse", reason: refusalText(refusal) };
  }
  return {
    kind: "accept",
    label:
      entry.node === null
        ? t("mods.groups.drop.makeTopLevel", { name: dragged.name })
        : t("mods.groups.drop.nestInside", { name: dragged.name, parent: entry.node.name }),
  };
}

/** The outcome of dropping a `kind` drag on `entry`, or `null` while no such drag is recorded. */
function outcomeFor(entry: Entry, kind: DragKind): DropOutcome | null {
  if (kind === "mods") {
    return rowDrag === null ? null : modsOutcome(entry, rowDrag);
  }
  const dragged = findNode(nodes, draggedGroupId.value);
  return dragged === undefined ? null : groupOutcome(entry, dragged);
}

/** The kind of drag an event carries, by this app's private types; `null` for any other drag. */
function dragKindOf(event: DragEvent): DragKind | null {
  const types = event.dataTransfer?.types ?? [];
  if (types.includes(ROW_DRAG_TYPE)) {
    return "mods";
  }
  return types.includes(GROUP_DRAG_TYPE) ? "group" : null;
}

const hoverOutcome = computed<DropOutcome | null>(() => {
  const entry = entries.value.find((candidate) => candidate.key === hoverKey.value);
  return entry === undefined ? null : outcomeFor(entry, hoverDragKind.value);
});

/** The sentence under the tree: the hovered outcome, else what the drag in progress can do. */
const dropHint = computed(() => {
  const outcome = hoverOutcome.value;
  if (outcome !== null) {
    return outcome.kind === "accept" ? outcome.label : outcome.reason;
  }
  if (rowDrag !== null) {
    return withHidden(
      t("mods.groups.drop.idleMods", { count: rowDrag.count }, rowDrag.count),
      rowDrag.hiddenCount,
    );
  }
  return draggedGroupId.value === null ? null : t("mods.groups.drop.idleGroup");
});

function clearHover(): void {
  clearTimeout(expandTimer);
  expandKey = null;
  hoverKey.value = null;
}

onScopeDispose(() => clearTimeout(expandTimer));

watch(
  () => rowDrag,
  (current) => {
    if (current === null) {
      clearHover();
    }
  },
);

/** A collapsed group opens once a drag has hovered it for a moment, so a deep target is reachable. */
function scheduleExpand(entry: Entry): void {
  if (expandKey === entry.key) {
    return;
  }
  clearTimeout(expandTimer);
  expandKey = entry.key;
  const node = entry.node;
  if (node === null || node.childCount === 0 || isExpanded(node)) {
    return;
  }
  expandTimer = setTimeout(() => setExpanded(node, true), HOVER_EXPAND_MS);
}

function onDragStart(event: DragEvent, entry: Entry): void {
  if (entry.node === null) {
    return;
  }
  if (event.dataTransfer) {
    // Some engines refuse to start a drag with no data; a private type is never a text drop.
    event.dataTransfer.setData(GROUP_DRAG_TYPE, String(entry.node.id));
    event.dataTransfer.effectAllowed = "move";
  }
  draggedGroupId.value = entry.node.id;
}

function onDragEnd(): void {
  draggedGroupId.value = null;
  clearHover();
}

function onDragOver(event: DragEvent, entry: Entry): void {
  const kind = dragKindOf(event);
  const outcome = kind === null ? null : outcomeFor(entry, kind);
  if (kind === null || outcome === null) {
    return;
  }
  hoverKey.value = entry.key;
  hoverDragKind.value = kind;
  scheduleExpand(entry);
  if (outcome.kind === "refuse") {
    // Not cancelling the event leaves the drop refused and shows the not-allowed cursor.
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = "none";
    }
    return;
  }
  event.preventDefault();
  if (event.dataTransfer) {
    // Mods dropped on a group are added to it (they stay where they are), as on a Groups view header.
    event.dataTransfer.dropEffect = kind === "mods" ? "copy" : "move";
  }
}

function onDragLeave(event: DragEvent, entry: Entry): void {
  const next = event.relatedTarget;
  const row = event.currentTarget;
  if (next instanceof Node && row instanceof HTMLElement && row.contains(next)) {
    return;
  }
  if (hoverKey.value === entry.key) {
    clearHover();
  }
}

function onDrop(event: DragEvent, entry: Entry): void {
  const kind = dragKindOf(event);
  const outcome = kind === null ? null : outcomeFor(entry, kind);
  const droppedGroup = draggedGroupId.value;
  clearHover();
  if (outcome?.kind !== "accept") {
    return;
  }
  event.preventDefault();
  if (kind === "mods") {
    if (entry.node !== null) {
      emit("dropMods", entry.node.id);
    }
    return;
  }
  if (droppedGroup !== null) {
    // The dragged row may be gone once the move re-draws the tree, so its `dragend` is not relied on.
    draggedGroupId.value = null;
    emit("dropGroup", droppedGroup, entry.node?.id ?? null);
  }
}

defineExpose({ focusKey, focusCurrent });
</script>

<template>
  <div class="flex min-h-0 flex-col gap-1">
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
        :class="[
          isSameScope(scope, entry.choice)
            ? 'bg-accent-soft text-text inset-ring-accent inset-ring-2 font-semibold'
            : 'text-text',
          hoverKey === entry.key && hoverOutcome?.kind === 'accept'
            ? 'bg-accent-soft outline-accent outline-2 -outline-offset-2 outline-dashed'
            : '',
          hoverKey === entry.key && hoverOutcome?.kind === 'refuse'
            ? 'text-text-muted cursor-not-allowed outline-2 -outline-offset-2 outline-dotted'
            : '',
        ]"
        :draggable="entry.node !== null"
        :data-drop="hoverKey === entry.key ? hoverOutcome?.kind : undefined"
        :style="{ paddingInlineStart: `${(entry.node === null ? 0 : entry.node.depth - 1) * 14 + 8}px` }"
        :data-testid="entry.node === null ? `group-node-${entry.key}` : `group-node-${entry.node.id}`"
        @click="onItemClick(entry)"
        @focus="focusedKey = entry.key"
        @keydown="onKeydown($event, entry, index)"
        @contextmenu="onContextMenu($event, entry)"
        @dragstart="onDragStart($event, entry)"
        @dragend="onDragEnd"
        @dragover="onDragOver($event, entry)"
        @dragleave="onDragLeave($event, entry)"
        @drop="onDrop($event, entry)"
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
            class="size-1.5 shrink-0 rounded-full"
            :class="openModGroupIds.has(entry.node.id) ? 'bg-accent' : ''"
            aria-hidden="true"
            :data-testid="openModGroupIds.has(entry.node.id) ? 'group-node-open-dot' : undefined"
          />
          <GroupFolderIcon :color="entry.node.color" />
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
            v-if="openModGroupIds.has(entry.node.id)"
            class="sr-only"
            data-testid="group-node-open-mod"
          >{{ t("mods.groups.openModIsMember") }}</span>
          <span
            v-if="entry.node.color !== null"
            class="sr-only"
          >{{ t("mods.colors.rowColor", { name: t(COLOR_NAME_KEY[entry.node.color]) }) }}</span>
        </template>
        <i
          v-if="hoverKey === entry.key && hoverOutcome !== null"
          :class="['pi shrink-0 text-xs', hoverOutcome.kind === 'accept' ? 'pi-plus-circle' : 'pi-ban']"
          aria-hidden="true"
          data-testid="group-drop-icon"
        />
      </div>
    </div>
    <p
      v-if="dropHint !== null"
      class="text-xs"
      :class="hoverOutcome?.kind === 'refuse' ? 'text-status-danger' : 'text-text-muted'"
      data-testid="group-drop-hint"
    >
      {{ dropHint }}
    </p>
  </div>
</template>
