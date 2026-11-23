import { computed, nextTick, type Ref, ref, watch } from "vue";

import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import { ACTIVE_DEFAULT_SORT, isLoadOrderSort } from "@/utils/modListSort";
import type { PaneView } from "@/utils/panePrefs";

/** What {@link useShowInLoadOrder} reads from the Active pane and acts on. */
export interface ShowInLoadOrderOptions {
  /** The Active pane's rows from whichever query its shown view runs. */
  list: Ref<ModListDto | undefined>;
  /**
   * The List query's current answer: cached rows when they are fresh, else the fetch. `null` when
   * it failed. It is never answered from rows that predate the view and sort just set.
   */
  refresh: () => Promise<ModListDto | null>;
  /** The view the pane shows now (List while Groups cannot be used). */
  view: Ref<PaneView>;
  sort: Ref<ListSortDto>;
  /** Whether the pane's query carries a search, tag, source or group scope. */
  isFiltered: Ref<boolean>;
  /**
   * The filter inputs as typed now (search, tag, source and group scope, not the debounced query):
   * a change nobody here made means the user took over, and the request ends.
   */
  filterSignature: Ref<string>;
  setView: (view: PaneView) => void;
  setSort: (sort: ListSortDto) => void;
  /** Drops every filter that can hide a row from the Active pane, at once. */
  clearFilters: () => void;
  /** Sentences, built where the i18n function lives. */
  clearedNote: (modId: string) => string;
  announcement: (modId: string) => string;
  /** Scrolls the row into view and focuses it (the pane's own `focusRow`). */
  focusRow: (modId: string) => Promise<void>;
  announce: (text: string) => Promise<void>;
}

/**
 * "Show in load order" for the Active pane: switch it to List view in load order, then scroll to
 * the mod and focus its row. The filter stays when the mod is still shown under it. When the
 * filter hides the mod, the search, tag, source and group scope are cleared and `note` says so.
 *
 * The rows it looks in are the query's own fresh answer (`refresh`), asked for after the view and
 * sort are set and Colada has switched keys, never rows already on screen: a key last used a
 * while ago answers from its cache at once, stale. One request is current at a time. A newer
 * request, a sort or view change, a filter the user changes meanwhile, or a failed query ends the
 * older one without touching anything.
 */
export function useShowInLoadOrder(options: ShowInLoadOrderOptions) {
  const note = ref<string | null>(null);
  /** The latest request; an older one that wakes up finds this changed and stops. */
  let latestRequest = 0;

  /**
   * Whether the pane shows the mods in load order. Used only to decide whether the action is
   * offered, never whether the list can be reordered (that is the backend's answer).
   */
  const isLoadOrderShown = computed(
    () => options.view.value === "list" && isLoadOrderSort(options.sort.value),
  );

  /** Whether the pane's rows hold `modId`, for a caller deciding if the action would do something. */
  function isRowShown(modId: string): boolean {
    return options.list.value?.items.some((row) => row.modId === modId) ?? false;
  }

  function holds(rows: ModListDto, modId: string): boolean {
    return rows.items.some((row) => row.modId === modId);
  }

  /** The query's fresh rows, or `null` when this request was superseded, the user took over, or it failed. */
  async function freshRows(request: number, filters: string): Promise<ModListDto | null> {
    // Let Colada's own key watcher run for the view, sort and filters just set.
    await nextTick();
    if (request !== latestRequest) {
      return null;
    }
    const rows = await options.refresh();
    const isCurrent = request === latestRequest && options.filterSignature.value === filters;
    return isCurrent ? rows : null;
  }

  async function focusAndAnnounce(request: number, modId: string, text: string): Promise<void> {
    // The table of the new view renders on the next flush; its row is focused after that.
    await nextTick();
    if (request !== latestRequest) {
      return;
    }
    await options.focusRow(modId);
    await options.announce(text);
  }

  /** Starts the request for `modId`; a request already running is superseded. */
  async function show(modId: string): Promise<void> {
    latestRequest += 1;
    const request = latestRequest;
    note.value = null;
    options.setView("list");
    options.setSort(ACTIVE_DEFAULT_SORT);

    let rows = await freshRows(request, options.filterSignature.value);
    if (rows === null) {
      return;
    }
    let text = options.announcement(modId);
    if (!holds(rows, modId)) {
      if (!options.isFiltered.value) {
        // Gone from the list (activated away, rescanned out): nothing a filter was hiding.
        return;
      }
      options.clearFilters();
      note.value = options.clearedNote(modId);
      text = note.value;
      rows = await freshRows(request, options.filterSignature.value);
      if (rows === null) {
        return;
      }
      if (!holds(rows, modId)) {
        // Still missing with no filter left: the filter was not what hid it, so the note is false.
        note.value = null;
        return;
      }
    }
    await focusAndAnnounce(request, modId, text);
  }

  // The user leaving load order (another sort, or the Groups view) ends the request and its note.
  watch(isLoadOrderShown, (isShown) => {
    if (!isShown) {
      latestRequest += 1;
      note.value = null;
    }
  });

  // A filter the user sets afterwards makes the note stale.
  watch(options.isFiltered, (isFiltered) => {
    if (isFiltered) {
      note.value = null;
    }
  });

  return { note, isLoadOrderShown, isRowShown, show };
}
