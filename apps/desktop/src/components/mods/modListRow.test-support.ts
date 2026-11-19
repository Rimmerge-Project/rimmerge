import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import type { SourceDto } from "@/types/generated/SourceDto";

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
}

/** One `ModListRowDto`: a scanned local mod unless the overrides say otherwise. */
export function modRow(overrides: ModRowOverrides = {}): ModListRowDto {
  const state: ModListRowDto["state"] = overrides.missing
    ? { kind: "missing" }
    : overrides.pendingScan
      ? { kind: "pendingScan" }
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
  };
}

/** A whole list answer whose `total` is its row count. */
export function modList(items: ModListRowDto[]): ModListDto {
  return { total: items.length, items };
}
