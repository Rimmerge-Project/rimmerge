import type { DanglingCauseDto } from "@/types/generated/DanglingCauseDto";
import { assertNever } from "@/utils/assertNever";

/** The `t()` shape below takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>) => string;

/**
 * Human text for a `DanglingDefReference` finding's own `cause` —
 * `SuggestionPanel.vue`'s own evidence section and
 * `utils/rationale.ts`'s `Rationale::DanglingDefReference` rendering
 * both use this (the same underlying `DanglingCause` fact, rendered in
 * two different sentences — the evidence line and the suggestion's own
 * rationale — so the cause-to-phrase mapping itself lives in exactly
 * one place). `t`/`label` are the caller's own `useI18n().t`/
 * `useModLabel().label` — this function can't call either composable
 * itself (a plain `utils/` helper, not a component).
 */
export function danglingCauseText(
  cause: DanglingCauseDto,
  t: Translate,
  label: (id: string) => string,
): string {
  switch (cause.kind) {
    case "removedBy":
      return t("inbox.evidence.danglingDefReference.causeRemovedBy", { modId: label(cause.modId) });
    case "onlyInUnloadedFolder":
      return t("inbox.evidence.danglingDefReference.causeOnlyInUnloadedFolder", {
        modId: label(cause.modId),
        folder: cause.folder,
      });
    case "onlyInInactiveMod":
      return t("inbox.evidence.danglingDefReference.causeOnlyInInactiveMod", {
        modId: label(cause.modId),
      });
    case "definedNowhere":
      return t("inbox.evidence.danglingDefReference.causeDefinedNowhere");
    case "unexplained":
      return t("inbox.evidence.danglingDefReference.causeUnexplained");
    default:
      return assertNever(cause);
  }
}
