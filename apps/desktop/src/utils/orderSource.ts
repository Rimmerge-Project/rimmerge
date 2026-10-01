import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import { assertNever } from "@/utils/assertNever";

/**
 * A human-readable, capitalized label for one {@link OrderSourceDto} —
 * reuses the identically-worded `shell.orderSource.current`/`.suggested`
 * keys (`TheShell.vue`'s own order-source toggle) for standalone uses:
 * `LoadOrderPage.vue`'s heading, the Dashboard tile, `ApplyPreflight.vue`'s
 * `<dd>` and `DefPage.vue`'s source button text. Inside a sentence use
 * {@link orderSourceSentenceLabel} instead. Exhaustive via
 * {@link assertNever}. Returns a {@link MessageDescriptor} — render it
 * through `t()`/`useTranslateMessage()`, never as text.
 */
export function orderSourceLabel(source: OrderSourceDto): MessageDescriptor {
  switch (source) {
    case "current":
      return descriptor("shell.orderSource.current");
    case "suggested":
      return descriptor("shell.orderSource.suggested");
    default:
      return assertNever(source);
  }
}

/**
 * The same label for use inside a sentence ("the current order") — the
 * toggle labels of {@link orderSourceLabel} are capitalized, so splicing
 * them mid-sentence reads "the Current order". Lowercase per locale in
 * `shell.orderSource.inSentence*`. Exhaustive via {@link assertNever}.
 */
export function orderSourceSentenceLabel(source: OrderSourceDto): MessageDescriptor {
  switch (source) {
    case "current":
      return descriptor("shell.orderSource.inSentenceCurrent");
    case "suggested":
      return descriptor("shell.orderSource.inSentenceSuggested");
    default:
      return assertNever(source);
  }
}
