import { type MaybeRefOrGetter, onMounted, onScopeDispose, toValue } from "vue";

import { isInert, ownsArrowKeys, ownsEnterNatively } from "@/composables/shortcutTargets";
import { useDecideMutation, useRevertDecisionMutation } from "@/queries/findings";
import { useInboxStore } from "@/stores/inbox";
import { asFindingKey, type FindingKey } from "@/types/brands";
import type { DecideRequestDto } from "@/types/generated/DecideRequestDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";
import { mergeableDefKeyOf, primaryModIdOf } from "@/utils/finding";

/**
 * Where deciding/reverting a finding actually goes. Defaults to the
 * profile's own `decide`/`revert_decision` commands; `PatchInbox` passes
 * one that closes over a `patchId` and calls `decide_patch`/
 * `revert_patch_decision` instead — `useInboxKeys` itself never knows
 * which.
 */
export interface InboxKeysBackend {
  decide(request: DecideRequestDto): Promise<unknown>;
  revert(key: FindingKey): Promise<unknown>;
}

/**
 * The cursor/filter store `useInboxKeys` reads and writes. `useInboxStore`
 * (the profile inbox) and `usePatchStore` (`stores/patch.ts`) both satisfy
 * this structurally — see `stores/inbox.ts`'s `FindingFilterStore` for the
 * sibling interface `FindingList.vue` needs from the same stores.
 */
export interface InboxKeysStore {
  cursor: number;
  expandedKey: string | null;
  setCursor(index: number): void;
  setExpandedKey(key: string | null): void;
  cycleStatusFilter(): void;
}

/** What `useInboxKeys` needs from its host page to act on the cursor row. */
export interface UseInboxKeysOptions {
  /** The current page of findings, in display order. */
  items: MaybeRefOrGetter<ResolutionSummaryDto[]>;
  /** The full detail of the finding under the cursor, once loaded. */
  currentDetail: MaybeRefOrGetter<ResolutionDetailDto | undefined>;
  /**
   * The finding list's own DOM container. Deciding shortcuts
   * (Enter/digits/`i`/`u`) only act while focus is here or nowhere at
   * all (`document.body`) — see {@link isWithinDecideScope}.
   */
  containerRef: MaybeRefOrGetter<HTMLElement | null | undefined>;
  /** `o`: open a mod in the load-order why-panel. */
  onOpenOrder: (modId: string) => void;
  /** `g`: open a mod's detail page. */
  onOpenModDetail: (modId: string) => void;
  /** `/`: focus the search box. */
  onFocusSearch: () => void;
  /** `?`: toggle the shortcut help overlay. */
  onToggleHelp: () => void;
  /** `m` / picking a `merge` alternative: navigate to the merge editor for `findingKey`. */
  onOpenMerge: (findingKey: string) => void;
  /**
   * `Escape`: no-op when omitted (the profile inbox has no page to
   * "escape" to). `PatchInbox` passes a handler that returns focus to the
   * patch detail page's own header — deliberately not "back to the
   * profile findings".
   */
  onEscape?: () => void;
  /**
   * Where deciding/reverting goes — defaults to the profile's own
   * mutations (`useDecideMutation`/`useRevertDecisionMutation`).
   */
  backend?: InboxKeysBackend;
  /**
   * The cursor/filter store to read and write — defaults to
   * `useInboxStore()`. `PatchInbox` passes `usePatchStore()` so a patch's
   * cursor never shares state with the profile inbox's.
   */
  store?: InboxKeysStore;
}

/**
 * Whether the currently focused element is a safe place for a *deciding*
 * shortcut (Enter/digits/`i`/`u`) to act: nothing meaningfully focused
 * (`document.body`, the default when the user hasn't clicked into
 * anything) or somewhere inside the finding list itself. Narrower than
 * {@link isInert} — j/k/n/o/g/f/`/`/`?` don't decide anything, so they
 * aren't gated by this, only by `isInert`.
 */
function isWithinDecideScope(container: HTMLElement | null | undefined): boolean {
  const active = document.activeElement;
  if (active === null || active === document.body) {
    return true;
  }
  return container?.contains(active) ?? false;
}

/**
 * Registers the inbox's keyboard shortcuts (j/k/arrows move, Enter
 * accepts the suggestion, 1..9 pick an alternative, `i` ignores, `u`
 * reverts, `n` opens a note, `o`/`g` navigate, `/` focuses search, `f`
 * cycles the status filter, `?` toggles help) for as long as the calling
 * component is mounted. Deciding goes through the same
 * `useDecideMutation`/`useRevertDecisionMutation` Colada mutations
 * `InboxPage`'s own mouse-driven buttons use (rather than a separately
 * injected ipc dependency) — one invalidation path, tested the same way
 * as every other mutation in the app, via `installMockIpc`.
 */
export function useInboxKeys(options: UseInboxKeysOptions) {
  const inbox = options.store ?? useInboxStore();
  // Always registered (composables called unconditionally, per the `vue`
  // skill), even when `options.backend` overrides them — the mutation
  // itself is never invoked in that case, so no IPC fixture for
  // `decide`/`revert_decision` is needed by a caller that only ever uses
  // a custom backend.
  const { mutateAsync: decideProfile } = useDecideMutation();
  const { mutateAsync: revertProfileDecision } = useRevertDecisionMutation();
  const decide = options.backend?.decide ?? decideProfile;
  const revertDecision = options.backend?.revert ?? revertProfileDecision;

  function moveCursor(delta: number): void {
    const items = toValue(options.items);
    const maxIndex = Math.max(items.length - 1, 0);
    inbox.setCursor(Math.min(Math.max(inbox.cursor + delta, 0), maxIndex));
  }

  /**
   * Runs `action` against the cursor's finding. Under the inbox's default
   * `needsInput` filter, deciding removes the finding from view once the
   * invalidated list refetches (which the mutation's own `onSuccess`
   * already awaits) — the *next* finding then slides into the same
   * cursor index on its own, so advancing the cursor unconditionally
   * here would skip one. Only bump the cursor when the decided finding
   * is still sitting at it after the refetch settles (e.g. a non-default
   * filter that doesn't exclude decided findings).
   */
  async function decideCurrent(action: ResolutionDetailDto["suggestion"]["action"]): Promise<void> {
    const detail = toValue(options.currentDetail);
    if (!detail) {
      return;
    }
    const decidedKey = detail.key;
    await decide({ key: decidedKey, action, note: null });

    const items = toValue(options.items);
    if (items[inbox.cursor]?.key === decidedKey && inbox.cursor < items.length - 1) {
      inbox.setCursor(inbox.cursor + 1);
    }
  }

  /** `Enter`: accepts the ledger's own suggestion. */
  async function acceptSuggestion(): Promise<void> {
    const detail = toValue(options.currentDetail);
    if (!detail) {
      return;
    }
    await decideCurrent(detail.suggestion.action);
  }

  /**
   * `1`..`9`: picks the nth alternative (1-based, so `1` is index 0). A
   * `merge` alternative always carries an empty choices map (see
   * `rim-resolve`'s `merge_alternative`) — deciding it opens the merge
   * editor instead of just recording the decision, same as `mergeCurrent`.
   */
  async function pickAlternative(oneBasedIndex: number): Promise<void> {
    const detail = toValue(options.currentDetail);
    const alternative = detail?.suggestion.alternatives[oneBasedIndex - 1];
    if (!detail || !alternative) {
      return;
    }
    if (alternative.action.kind === "merge") {
      const key = detail.key;
      await decideCurrent(alternative.action);
      options.onOpenMerge(key);
      return;
    }
    await decideCurrent(alternative.action);
  }

  /**
   * `m`: on a def-override/patch-collision finding, decides an empty
   * `Merge` (no field choices yet) and opens its editor. A no-op on every
   * other finding kind — {@link mergeableDefKeyOf} is `null` there.
   */
  async function mergeCurrent(): Promise<void> {
    const detail = toValue(options.currentDetail);
    if (!detail) {
      return;
    }
    const key = mergeableDefKeyOf(detail.finding);
    if (!key) {
      return;
    }
    const findingKey = detail.key;
    await decideCurrent({ kind: "merge", key, choices: {} });
    options.onOpenMerge(findingKey);
  }

  /** `i`: ignores the current finding outright. */
  async function ignoreCurrent(): Promise<void> {
    await decideCurrent({ kind: "ignore" });
  }

  /** `u`: reverts the decision on the current finding, if any. */
  async function revertCurrent(): Promise<void> {
    const detail = toValue(options.currentDetail);
    if (!detail?.hasDecision) {
      return;
    }
    await revertDecision(asFindingKey(detail.key));
  }

  /** `o`/`g`: navigates to `finding`'s representative mod, if it names one. */
  function navigateToPrimaryMod(
    finding: ResolutionDetailDto["finding"],
    go: (modId: string) => void,
  ): void {
    const modId = primaryModIdOf(finding);
    if (modId.length > 0) {
      go(modId);
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (isInert(event.target)) {
      return;
    }
    // A focused radio group moves its own selection on the arrows; every other key is ours.
    if (event.key.startsWith("Arrow") && ownsArrowKeys(event.target)) {
      return;
    }

    const detail = toValue(options.currentDetail);
    const canDecide = isWithinDecideScope(toValue(options.containerRef));

    if (/^[1-9]$/.test(event.key)) {
      if (!canDecide) {
        return;
      }
      event.preventDefault();
      void pickAlternative(Number(event.key));
      return;
    }

    switch (event.key) {
      case "j":
      case "ArrowDown":
        event.preventDefault();
        moveCursor(1);
        break;
      case "k":
      case "ArrowUp":
        event.preventDefault();
        moveCursor(-1);
        break;
      case "Enter":
        if (!canDecide || ownsEnterNatively(event.target)) {
          break;
        }
        event.preventDefault();
        void acceptSuggestion();
        break;
      case "i":
        if (!canDecide) {
          break;
        }
        event.preventDefault();
        void ignoreCurrent();
        break;
      case "u":
        if (!canDecide) {
          break;
        }
        event.preventDefault();
        void revertCurrent();
        break;
      case "m":
        if (!canDecide) {
          break;
        }
        event.preventDefault();
        void mergeCurrent();
        break;
      case "n":
        event.preventDefault();
        if (detail) {
          inbox.setExpandedKey(inbox.expandedKey === detail.key ? null : detail.key);
        }
        break;
      case "o":
        event.preventDefault();
        if (detail) {
          navigateToPrimaryMod(detail.finding, options.onOpenOrder);
        }
        break;
      case "g":
        event.preventDefault();
        if (detail) {
          navigateToPrimaryMod(detail.finding, options.onOpenModDetail);
        }
        break;
      case "/":
        event.preventDefault();
        options.onFocusSearch();
        break;
      case "f":
        event.preventDefault();
        inbox.cycleStatusFilter();
        break;
      case "?":
        event.preventDefault();
        options.onToggleHelp();
        break;
      case "Escape":
        if (!options.onEscape) {
          break;
        }
        event.preventDefault();
        options.onEscape();
        break;
      default:
        break;
    }
  }

  onMounted(() => {
    window.addEventListener("keydown", handleKeydown);
  });
  onScopeDispose(() => {
    window.removeEventListener("keydown", handleKeydown);
  });

  return {
    handleKeydown,
    acceptSuggestion,
    pickAlternative,
    ignoreCurrent,
    revertCurrent,
    mergeCurrent,
    moveCursor,
  };
}
