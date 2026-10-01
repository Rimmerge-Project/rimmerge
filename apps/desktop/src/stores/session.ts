import { defineStore } from "pinia";
import { ref } from "vue";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { ProjectPathsDto } from "@/types/generated/ProjectPathsDto";
import type { ScanWarningDto } from "@/types/generated/ScanWarningDto";
import { baseModId } from "@/utils/modId";

/**
 * Cross-route session state: which project is loaded and its paths, which
 * order source the UI is browsing, and the current scan's notes — never
 * server data (that's Pinia Colada's job; see `queries/`), and `scanNotes`
 * is a one-time `load_project` response, not something re-fetchable later,
 * so it lives here rather than behind a query. The confidence threshold
 * lives in the backend's settings and `queries/settings.ts`'s own
 * query/mutation pair, not here.
 *
 * `gameLogSummary` is the
 * same shape of thing as `scanNotes` — a one-time `import_game_log`
 * response, not something a query re-fetches — held here so the
 * `/startup` page, the dashboard's import button, and the apply
 * dialog's def-cache note all read the same last-imported log without
 * re-importing it themselves. Never persisted or cached on the backend
 * (the log is chosen by the user each time), so it's cleared on
 * `reset()` like every other per-project field here.
 */
export const useSessionStore = defineStore("session", () => {
  const paths = ref<ProjectPathsDto | null>(null);
  const loaded = ref(false);
  const selected = ref<OrderSourceDto>("current");
  const scanNotes = ref<ScanWarningDto[]>([]);
  const gameLogSummary = ref<GameLogSummaryDto | null>(null);

  /**
   * Whether the imported log's **last** load event (`loadEvents.at(-1)` —
   * the order the session that wrote the log ended up playing under)
   * differs from `activeOrder` (the order source currently being
   * analyzed, in load order, as raw mod ids) — `null` when there's
   * nothing to compare (no log imported, or the log carried no
   * `Initializing new game with mods:` / `Loading game from file ... with
   * mods:` block at all). Ids compare through their base id: the log's
   * package ids never carry `_steam`, while raw `ModsConfig.xml` order
   * ids do for a mod installed both locally and from the Workshop.
   * Callers pass the *currently selected* order's
   * own ids; comparing against a stale order would defeat the point of
   * this warning. A plain function, not a `computed` — callers wrap it
   * in their own `computed` alongside whatever order data they hold, so
   * this store never needs to know which order a given caller means.
   */
  function loggedOrderDiffersFrom(activeOrder: readonly string[]): boolean | null {
    const loggedOrder = gameLogSummary.value?.loadEvents.at(-1)?.mods;
    if (!loggedOrder) {
      return null;
    }
    return (
      loggedOrder.length !== activeOrder.length ||
      loggedOrder.some((id, index) => baseModId(id) !== baseModId(activeOrder[index] ?? ""))
    );
  }

  /**
   * Marks a project as loaded at `projectPaths`, taking `selectedSource`
   * from the backend (`ProjectSummaryDto.selected`) — the session decides
   * which order a fresh load opens on, so the store never carries a
   * second default of its own. A project switch (setup → load a
   * different project without a full app restart) therefore cannot carry
   * over whichever order source the previous project's user was browsing.
   */
  function setLoaded(projectPaths: ProjectPathsDto, selectedSource: OrderSourceDto): void {
    paths.value = projectPaths;
    loaded.value = true;
    selected.value = selectedSource;
  }

  /** Changes which order source the UI is browsing. */
  function setSelected(source: OrderSourceDto): void {
    selected.value = source;
  }

  /** Records this load's scan notes (`ProjectSummaryDto.warnings`), shown on the dashboard and mod detail pages. */
  function setScanNotes(notes: ScanWarningDto[]): void {
    scanNotes.value = notes;
  }

  /** Records a fresh `import_game_log` result, replacing any previous one. */
  function setGameLogSummary(summary: GameLogSummaryDto): void {
    gameLogSummary.value = summary;
  }

  /** Clears the loaded project, returning to the setup page's state. */
  function reset(): void {
    paths.value = null;
    loaded.value = false;
    selected.value = "current";
    scanNotes.value = [];
    gameLogSummary.value = null;
  }

  /**
   * Marks the loaded project lost after a recoverable backend failure —
   * `apps/desktop/src-tauri/src/state.rs`'s `with_session` returns
   * `session_lost` exactly when a command panicked (or, defensively, the
   * lock was poisoned by something else) and discarded its own copy of
   * the `Session`, never for any other reason. Unlike {@link reset}, this
   * deliberately keeps `paths` (and `gameLogSummary`, still valid for the
   * same install) so `main.ts`'s global session-lost handler can send the
   * user straight back through the setup page's own load flow — same
   * paths, same visible progress UI — instead of forcing a full manual
   * re-entry. Returns the paths to reload from, or `null` when there's
   * nothing to reload (no project had ever finished loading yet), in
   * which case the caller should fall back to a plain {@link reset}.
   */
  function markLostForReload(): ProjectPathsDto | null {
    loaded.value = false;
    return paths.value;
  }

  return {
    paths,
    loaded,
    selected,
    scanNotes,
    gameLogSummary,
    loggedOrderDiffersFrom,
    setLoaded,
    setSelected,
    setScanNotes,
    setGameLogSummary,
    reset,
    markLostForReload,
  };
});
