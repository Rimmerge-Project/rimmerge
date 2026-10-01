import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { TagProvenanceDto } from "@/types/generated/TagProvenanceDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link TagProvenanceDto}'s `kind` —
 * `TagRuleEditor.vue`'s own source column. Exhaustive via
 * {@link assertNever}. Returns a {@link MessageDescriptor} — render it
 * through `t()`/`useTranslateMessage()`, never as text.
 */
export function tagProvenanceKindLabel(kind: TagProvenanceDto["kind"]): MessageDescriptor {
  switch (kind) {
    case "inferred":
      return descriptor("tagProvenance.inferred");
    case "manual":
      return descriptor("tagProvenance.manual");
    default:
      return assertNever(kind);
  }
}
