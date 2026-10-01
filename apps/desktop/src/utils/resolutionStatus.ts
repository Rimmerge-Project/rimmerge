import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { ResolutionStatusDto } from "@/types/generated/ResolutionStatusDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable label for one {@link ResolutionStatusDto} —
 * `BaseStatusPill.vue`'s own status pill. Reuses the identically-worded
 * `dashboard.table.*` keys (the dashboard's own status column already
 * spells out this same three-way enum) rather than duplicating the
 * English text, the same convention {@link import("@/utils/rule").ruleOriginLabel}
 * uses for `Layer`. Exhaustive via {@link assertNever}. Returns a
 * {@link MessageDescriptor} — render it through `t()`/`useTranslateMessage()`,
 * never as text.
 */
export function resolutionStatusLabel(status: ResolutionStatusDto): MessageDescriptor {
  switch (status) {
    case "auto":
      return descriptor("dashboard.table.auto");
    case "needsInput":
      return descriptor("dashboard.table.needsInput");
    case "userOverridden":
      return descriptor("dashboard.table.overridden");
    default:
      return assertNever(status);
  }
}
