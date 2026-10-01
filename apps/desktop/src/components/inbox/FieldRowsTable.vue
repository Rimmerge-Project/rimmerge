<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import FieldValueChip from "@/components/inbox/FieldValueChip.vue";
import { useModLabel } from "@/composables/useModLabel";
import { formatList } from "@/i18n/format";
import type { FieldRowDto } from "@/types/generated/FieldRowDto";
import type { FieldRowKindDto } from "@/types/generated/FieldRowKindDto";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";

/**
 * The fields table: `rows` is whatever the parent (`DefConflictView`)
 * already kind-filtered and paged — this component only orders and
 * groups them for display. `Conflict` rows render first, each
 * toucher's own value shown as a chip with the preferred one ringed and
 * captioned with its reason; `CleanMerge`/`ListEntry`/`MapEntry` rows
 * follow, grouped by their shared container path whenever more than one
 * row shares it — the "mod A adds item 1, mod B adds item 2" case,
 * extended to a tag-keyed map's own
 * mostly-uncontested keys (e.g. `wildAnimals`, dozens of entries where
 * only a handful actually collide) — showing each entry with the mod it
 * came from, in the final list's own order (already how `rows` arrives —
 * see `rim_session::def_conflict_view`'s own position-based `ListEntry`
 * sort). A lone `CleanMerge`/`MapEntry` row is its own trivial group, so
 * it gets no extra header. `Unchanged` rows (present only when the
 * parent's own kind filter asked for them) render last, plainly — they
 * carry no `values`/`preference` of their own. The table scrolls
 * horizontally in its own container (six columns, several of variable
 * width, easily wider than the panel).
 *
 * The "list case" dedup, extended to a keyed
 * map's own key agreement: when several mods independently
 * added the exact same `li` item (or agreed on the exact same map key)
 * under a colliding identity, `rim_session::def_conflict_view` folds them
 * into one `ListEntry`/`MapEntry` row — `row.agreedBy` names the other
 * contributors, rendered as a small "Also added by ..." line under the
 * value chip rather than a second, indistinguishable-looking row. This
 * view is read-only inspection (`DefConflictView`), not the merge editor
 * — resolving an actual contested key is `MergeFieldTable`'s own job.
 */
const { rows } = defineProps<{ rows: FieldRowDto[] }>();

const { t, locale } = useI18n();
const modLabel = useModLabel();

// `mapEntry` shares `listEntry`'s own label/badge treatment — a keyed
// map's own key row is `FieldRowKind`'s `ListEntry` twin (same row shape,
// same "several entries end up unioned together" story), and now also
// shares its container-header grouping (`orderedRows` below) instead of
// being silently dropped from the table.
//
// A function, not a precomputed `Record` — the latter would evaluate
// every `t()` call once at component creation, going stale on a later
// locale switch (the same module-scope-constant trap `TheShell.vue`'s
// `NAV_ITEMS` doc comment names).
function kindLabel(kind: FieldRowKindDto): string {
  switch (kind) {
    case "conflict":
      return t("inbox.fieldRowsTable.kindConflict");
    case "cleanMerge":
      return t("inbox.fieldRowsTable.kindCleanMerge");
    case "listEntry":
      return t("inbox.fieldRowsTable.kindListEntry");
    case "mapEntry":
      return t("inbox.fieldRowsTable.kindMapEntry");
    case "unchanged":
      return t("inbox.fieldRowsTable.kindUnchanged");
  }
}
const KIND_BADGE: Record<FieldRowKindDto, string> = {
  conflict: "bg-status-input-soft text-status-input",
  cleanMerge: "bg-status-auto-soft text-status-auto",
  listEntry: "bg-accent-soft text-accent",
  mapEntry: "bg-accent-soft text-accent",
  unchanged: "bg-surface-2 text-text-faint",
};

/**
 * A row's value is an XML fragment (renders in `MergeValueCell`'s `<pre>`,
 * not a truncated single-line span) whenever it's a `ListEntry`/`MapEntry`
 * row, or a `Conflict` row landing on an `li` item ("two mods adding
 * entries with the same identity is a Conflict row on that entry" —
 * `FieldRowKind` itself doesn't distinguish that case). `FieldRowDto`
 * carries no `entry`/`container` of its own (unlike `MergeFieldDto` —
 * this view is read-only, so there's no per-field DTO to extend the way
 * `dto::merge` was for the merge editor), so a `Conflict` row landing on
 * an *attributed* keyed-map key (rare) still renders as plain leaf text
 * rather than XML — a known gap this component shares with every other
 * `Value::Item` fallback.
 */
function isXml(row: FieldRowDto): boolean {
  return row.kind === "listEntry" || row.kind === "mapEntry" || row.path.includes("/li[");
}

/** `path`'s own container — everything before its last `/` segment, or the whole path when it has none. Rows sharing a container are entries of the same list (or, for a lone `CleanMerge` row, just itself). */
function containerPath(path: string): string {
  const index = path.lastIndexOf("/");
  return index === -1 ? path : path.slice(0, index);
}

/** The mod a row's `preference` names as the winner, so its own value chip can be marked — `null` for `Preference::None` or a `MergeChoice` that isn't a plain `from`. */
function preferredModId(row: FieldRowDto): string | null {
  switch (row.preference.kind) {
    case "loadOrder":
    case "decision":
      return row.preference.winner;
    case "mergeChoice":
      return row.preference.choice.choice === "from" ? row.preference.choice.modId : null;
    case "none":
      return null;
  }
}

function mergeChoiceLabel(choice: MergeChoiceDto): string {
  if (choice.choice === "from") {
    return t("inbox.fieldRowsTable.mergeChoiceFrom", { modId: modLabel.label(choice.modId) });
  }
  if (choice.choice === "value") {
    return t("inbox.fieldRowsTable.mergeChoiceValue", { text: choice.text });
  }
  return t("inbox.fieldRowsTable.mergeChoiceDropped");
}

/** "who wins, and why" — `null` for anything but a `Conflict` row (every other kind's `preference` is always `None`). */
function preferenceText(row: FieldRowDto): string | null {
  switch (row.preference.kind) {
    case "loadOrder":
      return t("inbox.fieldRowsTable.preferenceLoadOrder", {
        winner: modLabel.label(row.preference.winner),
      });
    case "decision":
      return t("inbox.fieldRowsTable.preferenceDecision", {
        winner: modLabel.label(row.preference.winner),
      });
    case "mergeChoice":
      return t("inbox.fieldRowsTable.preferenceMergeChoice", {
        choice: mergeChoiceLabel(row.preference.choice),
      });
    case "none":
      return null;
  }
}

interface OrderedRow {
  row: FieldRowDto;
  /** Set only on the first row of a container sharing it with at least one other row — see this file's own doc comment. */
  groupHeader: { path: string; count: number } | null;
}

/**
 * Groups over `rows` as given — `DefConflictView`'s own accumulated,
 * paged set (every page `usePagedRows`'s `loadMore` has landed so far,
 * not just the server's current page). A single list whose entries
 * straddle a page boundary (more than
 * `PAGE_SIZE` clean-merge/list-entry rows precede it, or its own entries
 * span two pages) shows an *undercounted* `groupHeader.count` — only
 * however many of that container's rows have loaded so far — until
 * "Show more" pulls in the rest, at which point this computed reruns over
 * the now-larger `rows` and the count catches up on its own (this
 * function always re-groups the *whole* accumulated set from scratch, it
 * never appends to a stale grouping). No server-side change makes this
 * exact: the field list's own total (`fieldsTotal`) counts matching rows,
 * not rows-per-container, so there is no cheap way to know a specific
 * container's true final count before every page containing one of its
 * rows has loaded. Real defs are extremely unlikely to hit this (a
 * single list needing more than `PAGE_SIZE` — 200 — entries from other
 * fields first), so this is left as a documented, self-correcting
 * display quirk rather than dropping the count outright.
 */
const orderedRows = computed<OrderedRow[]>(() => {
  const result: OrderedRow[] = [];
  for (const row of rows) {
    if (row.kind === "conflict") {
      result.push({ row, groupHeader: null });
    }
  }

  const byContainer = new Map<string, FieldRowDto[]>();
  for (const row of rows) {
    // `mapEntry` joins the same grouping bucket as
    // `cleanMerge`/`listEntry` — a keyed map's own uncontested keys are
    // exactly the "several entries end up unioned together" shape this
    // grouping exists for. Without it, a `mapEntry` row would match none of
    // the three loops in this function and be silently dropped from the
    // table entirely.
    if (row.kind !== "cleanMerge" && row.kind !== "listEntry" && row.kind !== "mapEntry") {
      continue;
    }
    const key = containerPath(row.path);
    const existing = byContainer.get(key);
    if (existing) {
      existing.push(row);
    } else {
      byContainer.set(key, [row]);
    }
  }
  for (const [path, groupRows] of byContainer) {
    groupRows.forEach((row, index) => {
      result.push({
        row,
        groupHeader: index === 0 && groupRows.length > 1 ? { path, count: groupRows.length } : null,
      });
    });
  }

  for (const row of rows) {
    if (row.kind === "unchanged") {
      result.push({ row, groupHeader: null });
    }
  }
  return result;
});
</script>

<template>
  <div
    class="flex flex-col gap-2"
    data-testid="def-conflict-fields-table"
  >
    <p
      v-if="rows.length === 0"
      class="text-text-faint text-xs"
      data-testid="def-conflict-fields-empty"
    >
      {{ t("inbox.fieldRowsTable.empty") }}
    </p>

    <div
      v-else
      class="overflow-x-auto"
    >
      <table class="w-full text-left text-xs">
        <thead>
          <tr class="text-text-faint uppercase">
            <th class="py-1 pr-3 font-medium">
              {{ t("inbox.fieldRowsTable.headerPath") }}
            </th>
            <th class="py-1 pr-3 font-medium">
              {{ t("inbox.fieldRowsTable.headerKind") }}
            </th>
            <th class="py-1 pr-3 font-medium">
              {{ t("inbox.fieldRowsTable.headerValues") }}
            </th>
            <th class="py-1 pr-3 font-medium">
              {{ t("inbox.fieldRowsTable.headerInGame") }}
            </th>
            <th class="py-1 pr-3 font-medium">
              {{ t("inbox.fieldRowsTable.headerAfterMerge") }}
            </th>
            <th class="py-1 font-medium">
              {{ t("inbox.fieldRowsTable.headerPreference") }}
            </th>
          </tr>
        </thead>
        <tbody>
          <template
            v-for="{ row, groupHeader } in orderedRows"
            :key="row.path"
          >
            <tr v-if="groupHeader">
              <td
                colspan="6"
                class="text-text-faint pt-2 pb-0.5 font-mono text-[11px]"
                data-testid="def-conflict-group-header"
              >
                {{
                  t(
                    "inbox.fieldRowsTable.groupHeader",
                    { path: groupHeader.path, count: groupHeader.count },
                    groupHeader.count,
                  )
                }}
              </td>
            </tr>
            <tr
              class="border-border-subtle border-t align-top"
              data-testid="def-conflict-field-row"
              :data-path="row.path"
            >
              <td class="py-1 pr-3 font-mono whitespace-nowrap">
                {{ row.path }}
              </td>
              <td class="py-1 pr-3">
                <span
                  class="rounded-full px-1.5 py-0.5 text-[10px] font-medium whitespace-nowrap"
                  :class="KIND_BADGE[row.kind]"
                >{{ kindLabel(row.kind) }}</span>
              </td>
              <td class="py-1 pr-3">
                <div class="flex flex-wrap items-center gap-2">
                  <FieldValueChip
                    v-for="value in row.values"
                    :key="value.modId"
                    :mod-id="value.modId"
                    :value="value.value"
                    :is-xml="isXml(row)"
                    :highlighted="value.modId === preferredModId(row)"
                    :data-testid="`def-conflict-value-${row.path}-${value.modId}`"
                  />
                </div>
                <div
                  v-if="row.agreedBy.length > 0"
                  class="text-text-faint mt-0.5 text-[11px]"
                  data-testid="def-conflict-agreed-by"
                >
                  {{
                    t("inbox.fieldRowsTable.alsoAddedBy", {
                      mods: formatList(
                        locale,
                        row.agreedBy.map((modId) => modLabel.label(modId)),
                      ),
                    })
                  }}
                </div>
              </td>
              <td class="py-1 pr-3">
                <FieldValueChip
                  v-if="row.inGame"
                  :mod-id="row.inGame.modId"
                  :value="row.inGame.value"
                  :is-xml="isXml(row)"
                  data-testid="def-conflict-in-game"
                />
                <span
                  v-else
                  class="text-text-faint"
                >—</span>
              </td>
              <td class="py-1 pr-3">
                <FieldValueChip
                  v-if="row.afterMerge"
                  :mod-id="row.afterMerge.modId"
                  :value="row.afterMerge.value"
                  :is-xml="isXml(row)"
                  data-testid="def-conflict-after-merge"
                />
                <span
                  v-else
                  class="text-text-faint"
                >—</span>
              </td>
              <td
                class="py-1 whitespace-nowrap"
                data-testid="def-conflict-preference"
              >
                {{ preferenceText(row) ?? "—" }}
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </div>
</template>
