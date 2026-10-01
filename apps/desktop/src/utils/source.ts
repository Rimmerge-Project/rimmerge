import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { SourceDto } from "@/types/generated/SourceDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link SourceDto} — `ModDetailPage.vue`'s
 * own header and `ModsPage.vue`'s source filter both render this rather
 * than the raw wire value. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through
 * `t()`/`useTranslateMessage()`, never as text.
 */
export function sourceLabel(source: SourceDto): MessageDescriptor {
  switch (source) {
    case "core":
      return descriptor("source.core");
    case "dlc":
      return descriptor("source.dlc");
    case "local":
      return descriptor("source.local");
    case "workshop":
      return descriptor("source.workshop");
    default:
      return assertNever(source);
  }
}
