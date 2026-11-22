import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ListSortKeyDto } from "@/types/generated/ListSortKeyDto";
import type { UpdateTimeDto } from "@/types/generated/UpdateTimeDto";
import { assertNever } from "@/utils/assertNever";

/** The Active list's own order: the working set as it stands. */
export const ACTIVE_DEFAULT_SORT: ListSortDto = { key: "loadOrder", direction: "ascending" };

/**
 * Whether a sort is the Active list's own load order. Only used to decide whether to offer "Show in
 * load order"; whether a list may be reordered is the backend's answer (`ModListDto.reorderable`),
 * never derived from this.
 */
export function isLoadOrderSort(sort: ListSortDto): boolean {
  return sort.key === ACTIVE_DEFAULT_SORT.key && sort.direction === ACTIVE_DEFAULT_SORT.direction;
}

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

function activeAscendingLabel(key: ListSortKeyDto): MessageDescriptor {
  switch (key) {
    case "loadOrder":
      return descriptor("mods.sort.activeAscending.loadOrder");
    case "name":
      return descriptor("mods.sort.activeAscending.name");
    case "packageId":
      return descriptor("mods.sort.activeAscending.packageId");
    case "lastUpdated":
      return descriptor("mods.sort.activeAscending.lastUpdated");
    default:
      return assertNever(key);
  }
}

function activeDescendingLabel(key: ListSortKeyDto): MessageDescriptor {
  switch (key) {
    case "loadOrder":
      return descriptor("mods.sort.activeDescending.loadOrder");
    case "name":
      return descriptor("mods.sort.activeDescending.name");
    case "packageId":
      return descriptor("mods.sort.activeDescending.packageId");
    case "lastUpdated":
      return descriptor("mods.sort.activeDescending.lastUpdated");
    default:
      return assertNever(key);
  }
}

/**
 * The sort a click on a criterion's button asks for: the active criterion
 * flips its direction, any other one starts ascending.
 */
export function nextSort(current: ListSortDto, key: ListSortKeyDto): ListSortDto {
  if (current.key !== key) {
    return { key, direction: "ascending" };
  }
  return { key, direction: current.direction === "ascending" ? "descending" : "ascending" };
}

/**
 * The accessible name and tooltip of a criterion's one button. An inactive
 * criterion says what a click selects ("Sort by name, A to Z"); the active one
 * says its current direction and what the next click does.
 */
export function sortButtonLabel(current: ListSortDto, key: ListSortKeyDto): MessageDescriptor {
  if (current.key !== key) {
    return ascendingLabel(key);
  }
  switch (current.direction) {
    case "ascending":
      return activeAscendingLabel(key);
    case "descending":
      return activeDescendingLabel(key);
    default:
      return assertNever(current.direction);
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
