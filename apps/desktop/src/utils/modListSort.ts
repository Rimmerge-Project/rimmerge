import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import type { SortDirectionDto } from "@/types/generated/SortDirectionDto";
import type { UpdateTimeDto } from "@/types/generated/UpdateTimeDto";
import { assertNever } from "@/utils/assertNever";

/** The Active list's own order: the working set as it stands. */
export const ACTIVE_DEFAULT_SORT: ListSortDto = { key: "loadOrder", direction: "ascending" };

/** The Inactive list's default, which the backend applies when no sort is sent. */
export const INACTIVE_DEFAULT_SORT: ListSortDto = { key: "name", direction: "ascending" };

/** Every criterion the Active list offers, in display order. */
export const ACTIVE_SORT_KEYS: readonly ListSortKeyDto[] = [
  "loadOrder",
  "name",
  "packageId",
  "lastUpdated",
];

/** The Inactive list has no load order to sort by. */
export const INACTIVE_SORT_KEYS: readonly ListSortKeyDto[] = ["name", "packageId", "lastUpdated"];

/**
 * Whether `sort` shows the working set in its own order, the only view in
 * which reordering the Active list is meaningful. Mirrors the backend's
 * `ListSort::is_load_order`.
 */
export function isLoadOrderSort(sort: ListSortDto): boolean {
  return sort.key === "loadOrder" && sort.direction === "ascending";
}

/** A criterion's own name ("Name", "Last updated", ...). */
export function sortKeyLabel(key: ListSortKeyDto): MessageDescriptor {
  switch (key) {
    case "loadOrder":
      return descriptor("mods.sort.key.loadOrder");
    case "name":
      return descriptor("mods.sort.key.name");
    case "packageId":
      return descriptor("mods.sort.key.packageId");
    case "lastUpdated":
      return descriptor("mods.sort.key.lastUpdated");
    default:
      return assertNever(key);
  }
}

function ascendingLabel(key: ListSortKeyDto): MessageDescriptor {
  switch (key) {
    case "loadOrder":
      return descriptor("mods.sort.ascending.loadOrder");
    case "name":
      return descriptor("mods.sort.ascending.name");
    case "packageId":
      return descriptor("mods.sort.ascending.packageId");
    case "lastUpdated":
      return descriptor("mods.sort.ascending.lastUpdated");
    default:
      return assertNever(key);
  }
}

function descendingLabel(key: ListSortKeyDto): MessageDescriptor {
  switch (key) {
    case "loadOrder":
      return descriptor("mods.sort.descending.loadOrder");
    case "name":
      return descriptor("mods.sort.descending.name");
    case "packageId":
      return descriptor("mods.sort.descending.packageId");
    case "lastUpdated":
      return descriptor("mods.sort.descending.lastUpdated");
    default:
      return assertNever(key);
  }
}

/**
 * The accessible name and tooltip of one arrow: it names both the criterion
 * and the direction ("Sort by name, A to Z").
 */
export function sortButtonLabel(
  key: ListSortKeyDto,
  direction: SortDirectionDto,
): MessageDescriptor {
  switch (direction) {
    case "ascending":
      return ascendingLabel(key);
    case "descending":
      return descendingLabel(key);
    default:
      return assertNever(direction);
  }
}

/** Why a mod has no update time, worded; `null` when it has one. */
export function updateUnavailableLabel(updated: UpdateTimeDto): MessageDescriptor | null {
  switch (updated.kind) {
    case "known":
      return null;
    case "shippedWithGame":
      return descriptor("mods.sort.updatedUnavailable.shippedWithGame");
    case "unknown":
      return descriptor("mods.sort.updatedUnavailable.unknown");
    default:
      return assertNever(updated);
  }
}
