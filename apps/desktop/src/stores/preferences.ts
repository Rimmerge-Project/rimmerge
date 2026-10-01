import { defineStore } from "pinia";
import { ref } from "vue";

import { isSupportedLocale, type StoredLocale } from "@/i18n/locales";

const MOD_LABEL_MODE_KEY = "rimmerge.modLabelMode";
const LOCALE_KEY = "rimmerge.locale";

/** Whether a mod id renders as its display name or its bare id. */
export type ModLabelMode = "name" | "id";

/**
 * Reads the persisted label mode. Wrapped in `try`/`catch`: a private
 * window, blocked site data, or a test/preview context that throws on
 * `localStorage` access must still leave the app usable, defaulting to
 * `"name"` (the friendlier default) rather than crashing at store setup.
 */
function readStoredModLabelMode(): ModLabelMode {
  try {
    return localStorage.getItem(MOD_LABEL_MODE_KEY) === "id" ? "id" : "name";
  } catch {
    return "name";
  }
}

/** Best-effort persistence — see {@link readStoredModLabelMode} for why failures are swallowed. */
function persistModLabelMode(mode: ModLabelMode): void {
  try {
    localStorage.setItem(MOD_LABEL_MODE_KEY, mode);
  } catch {
    // Not persisted this session; the in-memory value still applies.
  }
}

/**
 * Reads the persisted locale choice. Defaults to `"system"` (re-detect
 * from the OS at every start) on a missing, unreadable, or corrupted
 * value — the same best-effort fallback {@link readStoredModLabelMode}
 * uses, and the correct default for a reader whose Windows display
 * language changed since their last visit.
 */
function readStoredLocale(): StoredLocale {
  try {
    const stored = localStorage.getItem(LOCALE_KEY);
    return stored === "system" || (stored !== null && isSupportedLocale(stored))
      ? stored
      : "system";
  } catch {
    return "system";
  }
}

/** Best-effort persistence — see {@link readStoredModLabelMode} for why failures are swallowed. */
function persistLocale(locale: StoredLocale): void {
  try {
    localStorage.setItem(LOCALE_KEY, locale);
  } catch {
    // Not persisted this session; the in-memory value still applies.
  }
}

/**
 * Cross-route viewing preferences — never server data, and never anything
 * that needs to survive a different machine (that would belong in the
 * profile's settings file instead — a per-machine viewing preference is
 * what `localStorage` is for). Currently
 * the mod-id-vs-display-name toggle; `modLabelMode` is read from
 * `localStorage` once, at store creation ("hydrated once"), and every
 * change is persisted back immediately.
 */
export const usePreferencesStore = defineStore("preferences", () => {
  const modLabelMode = ref<ModLabelMode>(readStoredModLabelMode());
  const locale = ref<StoredLocale>(readStoredLocale());

  /** Sets the label mode explicitly, persisting the change. */
  function setModLabelMode(mode: ModLabelMode): void {
    modLabelMode.value = mode;
    persistModLabelMode(mode);
  }

  /** Flips between `"name"` and `"id"` — the `t` shortcut's action. */
  function toggleModLabelMode(): void {
    setModLabelMode(modLabelMode.value === "name" ? "id" : "name");
  }

  /**
   * Records the language picker's choice. Called from `i18n.ts`'s
   * `setAppLocale`, which is the only place that actually applies a
   * locale switch — this store only remembers what was chosen.
   */
  function setLocale(value: StoredLocale): void {
    locale.value = value;
    persistLocale(value);
  }

  return { modLabelMode, setModLabelMode, toggleModLabelMode, locale, setLocale };
});
