import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { EdgeStatusDto } from "@/types/generated/EdgeStatusDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link EdgeStatusDto} — `ModEdges.vue`'s
 * own per-edge status. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function edgeStatusLabel(status: EdgeStatusDto): MessageDescriptor {
  switch (status) {
    case "satisfied":
      return descriptor("edgeStatus.satisfied");
    case "violated":
      return descriptor("edgeStatus.violated");
    case "unevaluated":
      return descriptor("edgeStatus.unevaluated");
    default:
      return assertNever(status);
  }
}
