import { storeToRefs } from "pinia";

import { useModNamesQuery } from "@/queries/mods";
import { usePreferencesStore } from "@/stores/preferences";
import { baseModId } from "@/utils/modId";

/**
 * The shell-wide mod-id-vs-display-name toggle. `label(id)` resolves
 * through `useModNamesQuery`'s cached
 * `list_mod_names` map whenever the mode is `"name"`, falling back to the
 * bare id whenever the map has no entry (not loaded yet, or a mod that's
 * no longer active) — a `_steam`-suffixed id also resolves through its
 * base id, so a lookup succeeds however a finding happens to spell it.
 * `mode` lives in {@link usePreferencesStore}, hydrated once from
 * `localStorage` there, so every caller (there's no prop-drilling here on
 * purpose — this is a cross-cutting, always-available concern, the same
 * shape as a translation function) sees the same shared mode and the same
 * shared toggle.
 */
export function useModLabel() {
  const preferences = usePreferencesStore();
  const { modLabelMode: mode } = storeToRefs(preferences);
  const { data: names } = useModNamesQuery();

  /** The display name for `id` when the mode is `"name"` and it's known; `id` itself otherwise. */
  function label(id: string): string {
    if (mode.value !== "name") {
      return id;
    }
    const map = names.value;
    if (!map) {
      return id;
    }
    return map[id] ?? map[baseModId(id)] ?? id;
  }

  /**
   * `id`, meant for a `title` tooltip — only while `label(id)` might show
   * something other than `id` itself (mode `"name"`). Redundant (and so
   * omitted, returning `undefined`) once the id is already the visible
   * text.
   */
  function titleFor(id: string): string | undefined {
    return mode.value === "name" ? id : undefined;
  }

  /**
   * `id`'s resolved name when `list_mod_names` has one, else the
   * author-declared `displayName` (from a declared dependency, load
   * hint, or "required by" entry) when in name mode, else the bare id.
   * `id` doesn't have to be an active mod — a declared dependency may be
   * missing or inactive, so `list_mod_names` may have nothing for it; an
   * unresolved `label()` call returns the id unchanged, which is how
   * "nothing for it" is detected here.
   */
  function labelWithFallback(id: string, displayName: string | null): string {
    const resolved = label(id);
    if (resolved !== id) {
      return resolved;
    }
    return mode.value === "name" ? (displayName ?? id) : id;
  }

  return { label, labelWithFallback, mode, toggle: preferences.toggleModLabelMode, titleFor };
}
