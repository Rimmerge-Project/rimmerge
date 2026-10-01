import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { EdgeStrengthDto } from "@/types/generated/EdgeStrengthDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link EdgeStrengthDto} — `ModEdges.vue`'s
 * own group headers. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function edgeStrengthLabel(strength: EdgeStrengthDto): MessageDescriptor {
  switch (strength) {
    case "hard":
      return descriptor("edgeStrength.hard");
    case "declared":
      return descriptor("edgeStrength.declared");
    case "soft":
      return descriptor("edgeStrength.soft");
    case "awareness":
      return descriptor("edgeStrength.awareness");
    case "inferred":
      return descriptor("edgeStrength.inferred");
    default:
      return assertNever(strength);
  }
}
