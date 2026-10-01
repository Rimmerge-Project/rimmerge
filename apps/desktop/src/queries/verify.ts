import { useMutation } from "@pinia/colada";

import { verifyOrder } from "@/services/ipc";
import type { VerifyRequestDto } from "@/types/generated/VerifyRequestDto";

/**
 * Runs the on-demand `VerifyOrder` replay pass —
 * a mutation, not a cached query, since it's explicitly triggered
 * ("Verify order" button) rather than something a page loads
 * automatically, and its own result is never persisted or cached (see
 * `verify_order`'s own Rust-side doc comment for why: `PatchWillFail`
 * must never sit in the always-visible inbox as a possibly-stale
 * prediction). No `onSuccess` invalidation either — a verify pass never
 * mutates the session, so there's nothing to invalidate.
 */
export function useVerifyOrderMutation() {
  return useMutation({
    mutation: (request: VerifyRequestDto) => verifyOrder(request),
  });
}
