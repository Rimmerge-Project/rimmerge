import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";
import type { MembershipMarkDto } from "@/types/generated/MembershipMarkDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import type { SourceDto } from "@/types/generated/SourceDto";
import type { UpdateTimeDto } from "@/types/generated/UpdateTimeDto";

/** What a test can say about one list row; the row's `state` is derived from it. */
export interface ModRowOverrides {
  modId?: string;
  name?: string;
  source?: SourceDto | null;
  position?: number | null;
  tags?: string[];
  hardDependents?: number;
  /** A working-set id no folder backs. */
  missing?: boolean;
  /** Activated since the last scan. */
  pendingScan?: boolean;
  /** Tags added by hand; shown on an unscanned row (a scanned row lists its own `tags`). */
  manualTags?: string[];
  /** The groups the mod is a direct member of. */
  groups?: GroupLabelDto[];
  /** The mod's own colour. */
  color?: ColorTokenDto | null;
  updated?: UpdateTimeDto;
  /** How the row differs from the saved active mod list; none unless said. */
  membership?: MembershipMarkDto;
}

/** One `ModListRowDto`: a scanned local mod unless the overrides say otherwise. */
export function modRow(overrides: ModRowOverrides = {}): ModListRowDto {
  const state: ModListRowDto["state"] = overrides.missing
    ? { kind: "missing" }
    : overrides.pendingScan
      ? { kind: "pendingScan", manualTags: overrides.manualTags ?? [] }
      : {
          kind: "scanned",
          tags: overrides.tags ?? [],
          hardDependents: overrides.hardDependents ?? 0,
        };
  return {
    modId: overrides.modId ?? "active.mod",
    name: overrides.name ?? "Active Mod",
    source: overrides.source === undefined ? "local" : overrides.source,
    position: overrides.position ?? null,
    state,
    generated: null,
    workshopId: null,
    updated: overrides.updated ?? { kind: "unknown" },
    membership: overrides.membership ?? "none",
    groups: overrides.groups ?? [],
    color: overrides.color ?? null,
  };
}

/**
 * A whole list answer whose `total` is its row count. `reorderable` defaults to
 * `true`, the Active list in its own load order.
 */
export function modList(items: ModListRowDto[], reorderable = true): ModListDto {
  return { total: items.length, items, reorderable };
}
