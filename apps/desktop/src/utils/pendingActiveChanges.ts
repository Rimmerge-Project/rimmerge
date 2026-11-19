import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";

/**
 * Whether the user has active-mod work of their own that nothing has kept yet: changes no rescan
 * has picked up, a scanned order Rimmerge has not applied, or only a settled reorder. The backend
 * measures every part against what Rimmerge last loaded or wrote, so an outside edit of
 * `ModsConfig.xml` alone never makes this true.
 */
export function hasOwnPendingChanges(pending: PendingActiveChangesDto): boolean {
  return (
    pending.unscanned.added.length > 0 ||
    pending.unscanned.removed.length > 0 ||
    pending.unapplied.added.length > 0 ||
    pending.unapplied.removed.length > 0 ||
    // A settled reorder changes no membership, so the diffs above are empty for it.
    pending.orderUnapplied
  );
}
