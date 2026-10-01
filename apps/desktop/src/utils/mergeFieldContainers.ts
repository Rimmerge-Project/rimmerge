import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";

/**
 * Aggregate counts for one tag-keyed map field ("container") across
 * every currently-loaded field of a merge preview — every section
 * (conflicts/auto/unchanged), not just one, so a container header can
 * say "14 entries, 1 conflict" regardless of which section it renders in.
 * May undercount
 * once a section has more matching rows than have loaded so far — the
 * same self-correcting-until-"Show more" caveat
 * `FieldRowsTable.vue`'s own container grouping already documents.
 */
export interface ContainerStats {
  /** Every loaded field this container has, across every section. */
  total: number;
  /** How many of those are `Conflict`-classified — a genuine collision a person must resolve, as opposed to a key the plan already auto-resolved. */
  conflicts: number;
}

/**
 * Groups `fields` (the union of a `MergeFieldTable`'s three section
 * props) by `MergeFieldDto.container`, skipping ordinary (non-map)
 * fields entirely — they carry no `container` to group by.
 */
export function containerStats(fields: readonly MergeFieldDto[]): Map<string, ContainerStats> {
  const stats = new Map<string, ContainerStats>();
  for (const field of fields) {
    if (field.container === null) {
      continue;
    }
    const existing = stats.get(field.container) ?? { total: 0, conflicts: 0 };
    existing.total += 1;
    if (field.class === "conflict") {
      existing.conflicts += 1;
    }
    stats.set(field.container, existing);
  }
  return stats;
}

/** A container collapses by default once it has more than this many entries and nothing left to review — small enough that a def with a genuinely short list still opens flat. */
const AUTO_COLLAPSE_THRESHOLD = 12;

/**
 * Whether `container`'s own group should render collapsed in a
 * collapsible section (auto-resolved/unchanged — the conflicts section
 * never collapses a container, same as it never collapses at all). An
 * explicit user toggle (`overrides`) always wins; absent one, a large,
 * fully-resolved container (no member still `Conflict`-classified)
 * collapses by default so a keyed map's dozens of uncontested keys don't
 * dominate the section they auto-resolved into — the finding this whole
 * feature answers ("looks like a list not being rendered as list").
 */
export function isContainerCollapsedByDefault(stats: ContainerStats): boolean {
  return stats.total > AUTO_COLLAPSE_THRESHOLD && stats.conflicts === 0;
}

export function effectiveContainerCollapsed(
  container: string,
  stats: ContainerStats,
  overrides: Readonly<Record<string, boolean>>,
): boolean {
  return overrides[container] ?? isContainerCollapsedByDefault(stats);
}

/**
 * One run of consecutive fields sharing the same non-null `container` —
 * or a single ordinary field, treated as its own trivial "group" so the
 * caller doesn't special-case it. Assumes same-container fields are
 * contiguous within `fields`, true today because the field list is
 * already split per `DiffClass` bucket (conflicts/auto/unchanged) before
 * grouping, and a keyed map's own members never straddle more than one
 * bucket's worth of *other* fields between them. A hypothetical future
 * violation would only render the same container's header twice in one
 * section, not silently drop or duplicate a row.
 */
export interface FieldGroup {
  container: string | null;
  fields: MergeFieldDto[];
}

export function groupConsecutiveByContainer(fields: readonly MergeFieldDto[]): FieldGroup[] {
  const groups: FieldGroup[] = [];
  for (const field of fields) {
    const last = groups.at(-1);
    if (field.container !== null && last?.container === field.container) {
      last.fields.push(field);
      continue;
    }
    groups.push({ container: field.container, fields: [field] });
  }
  return groups;
}
