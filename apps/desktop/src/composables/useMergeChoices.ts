import { type MaybeRefOrGetter, onScopeDispose, ref, toValue } from "vue";

import { useSetMergeChoicesMutation } from "@/queries/merge";
import { asFieldPath, type FieldPath, type FindingKey } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";

/** Debounce window for `set_merge_choices`, in milliseconds. */
const DEBOUNCE_MS = 150;

/**
 * Owns the merge editor's client-side `Record<FieldPath, MergeChoiceDto>`
 * map: seeds it from a preview's already-stored `choice` fields, and
 * sends the *full* map through `set_merge_choices` on every change,
 * debounced (last write wins) so a burst of keyboard picks becomes one
 * request. Every mutating method's promise resolves only once that
 * request actually lands and its refetch settles (`useSetMergeChoicesMutation`
 * invalidates every query) — callers that must not move the cursor until
 * the server agrees (`useMergeKeys`) `await` it.
 *
 * If the send fails, the local map is left exactly as it was (the
 * optimistic edit is *not* rolled back) — `setChoice`/`clearChoice`'s
 * returned promise rejects so the caller can react, but the next flush
 * (from any later edit) simply retries with the same value still in
 * place. On unmount with a flush still pending, {@link onScopeDispose}
 * cancels the timer and sends immediately with the key captured at
 * scheduling time, so a pick made just before `Esc` unmounts the editor
 * is never lost and never sent under a key the caller's own `key` source
 * may have already stopped providing (e.g. the route param, mid-teardown).
 */
type Waiter = { resolve: (state: MergeStateDto) => void; reject: (error: unknown) => void };

export function useMergeChoices(
  key: MaybeRefOrGetter<FindingKey>,
  /**
   * Against the profile's own decisions when `null`/omitted, or that
   * compat patch's own decisions when given — folded into every
   * `set_merge_choices` request the same way `useMergeFieldPage`'s own
   * `patchId` folds into `get_merge_preview`.
   */
  patchId: MaybeRefOrGetter<string | null> = null,
  debounceMs = DEBOUNCE_MS,
) {
  const choices = ref<Record<FieldPath, MergeChoiceDto>>({});
  const { mutateAsync } = useSetMergeChoicesMutation();

  let timer: ReturnType<typeof setTimeout> | null = null;
  let waiters: Waiter[] = [];
  /** The key for the next flush, captured when it was scheduled — see the doc comment above. */
  let pendingKey: FindingKey | null = null;
  /** `patchId`, captured alongside {@link pendingKey} at the same scheduling time and for the same reason. */
  let pendingPatchId: string | null = null;
  /** Whether a `set_merge_choices` request is in flight (sent but not yet settled). */
  let inFlight = false;

  /** Structural equality for a small JSON DTO — no functions/dates involved, so a stringify compare is enough to avoid churning `choices` for a no-op reseed. */
  function choiceEquals(a: MergeChoiceDto, b: MergeChoiceDto): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  /**
   * Syncs the local map from `fields`' stored `choice`s, both directions
   * — a path the server reports a choice for is added/updated, a path in
   * `fields` the server no longer reports one for is removed. Skipped
   * entirely while a flush is pending or in flight: `fields` comes from a
   * preview query that may resolve *after* a local edit already moved
   * past what it describes (most commonly, a refetch racing a debounced
   * or in-flight send), and syncing then would silently overwrite that
   * edit — including undoing a local removal — with what is, from the
   * client's perspective, already stale server state. Call this with
   * each preview page as it arrives.
   */
  function seed(fields: readonly MergeFieldDto[]): void {
    if (timer !== null || inFlight) {
      return;
    }
    let changed = false;
    const next = { ...choices.value };
    for (const field of fields) {
      const path = asFieldPath(field.path);
      const existing = next[path];
      if (field.choice === null) {
        if (existing !== undefined) {
          delete next[path];
          changed = true;
        }
      } else if (existing === undefined || !choiceEquals(existing, field.choice)) {
        next[path] = field.choice;
        changed = true;
      }
    }
    if (changed) {
      choices.value = next;
    }
  }

  /** Sends the current map for `flushKey`/`flushPatchId` and settles `waiters` from the result. Never throws itself — failure rejects the waiters instead (see the doc comment above for what that leaves the map as). */
  function sendAndSettle(
    flushKey: FindingKey,
    flushPatchId: string | null,
    settled: Waiter[],
  ): void {
    inFlight = true;
    // `patchId` is only spread in when set — `exactOptionalPropertyTypes`
    // rejects an explicit `patchId: undefined` against the mutation's own
    // `patchId?: string`.
    mutateAsync({
      key: flushKey,
      choices: choices.value,
      ...(flushPatchId !== null && { patchId: flushPatchId }),
    }).then(
      (state) => {
        inFlight = false;
        for (const waiter of settled) waiter.resolve(state);
      },
      (error: unknown) => {
        inFlight = false;
        for (const waiter of settled) waiter.reject(error);
      },
    );
  }

  function scheduleFlush(): Promise<MergeStateDto> {
    return new Promise<MergeStateDto>((resolve, reject) => {
      waiters.push({ resolve, reject });
      // Captured now, not when the timer fires — the caller's own `key`/
      // `patchId` sources (e.g. route params) may no longer resolve to
      // anything by then, most notably during the unmount `flush` below.
      pendingKey = toValue(key);
      pendingPatchId = toValue(patchId);
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = null;
        const settled = waiters;
        waiters = [];
        const flushKey = pendingKey;
        const flushPatchId = pendingPatchId;
        pendingKey = null;
        pendingPatchId = null;
        if (flushKey === null) {
          // Unreachable: the timer never exists without a prior
          // `scheduleFlush` call having just set `pendingKey` above.
          return;
        }
        sendAndSettle(flushKey, flushPatchId, settled);
      }, debounceMs);
    });
  }

  // A pending debounce must not survive unmount silently dropping the
  // user's last pick (or firing later against a stale `key`): cancel the
  // timer and send immediately, with the key/patchId/waiters captured at
  // scheduling time.
  onScopeDispose(() => {
    if (timer === null) {
      return;
    }
    clearTimeout(timer);
    timer = null;
    const settled = waiters;
    waiters = [];
    const flushKey = pendingKey;
    const flushPatchId = pendingPatchId;
    pendingKey = null;
    pendingPatchId = null;
    if (flushKey !== null) {
      sendAndSettle(flushKey, flushPatchId, settled);
    }
  });

  /** Sets `path`'s choice, sending the updated map (debounced). */
  function setChoice(path: FieldPath, choice: MergeChoiceDto): Promise<MergeStateDto> {
    choices.value = { ...choices.value, [path]: choice };
    return scheduleFlush();
  }

  /** Reverts `path` back to automatic (removes its stored choice), sending the updated map (debounced). */
  function clearChoice(path: FieldPath): Promise<MergeStateDto> {
    const next = { ...choices.value };
    delete next[path];
    choices.value = next;
    return scheduleFlush();
  }

  return { choices, seed, setChoice, clearChoice };
}
