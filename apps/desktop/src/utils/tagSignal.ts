import type { TagSignalDto } from "@/types/generated/TagSignalDto";
import { assertNever } from "@/utils/assertNever";

/** The `t()` shape below takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>) => string;

/**
 * A human-readable label for one matched {@link TagSignalDto} —
 * `SuggestionPanel.vue`'s own `tagInferred` evidence list and
 * `TagRuleEditor.vue`'s own signals column both render this. A plain
 * string, not a {@link MessageDescriptor}: `assemblyRefTo`/`dependsOn`
 * name a mod id, which must go through `label` (`useModLabel().label`)
 * — the same real bug this migration fixes for `describeAction`'s own
 * `dropEdge`/`keepEdge` arms — so this composes the resolved label into
 * the sentence rather than leaving it to a template's own interpolation.
 * `t` is the caller's own `useI18n().t`; this function can't call
 * `useI18n()` itself (a plain `utils/` helper, not a component).
 */
export function tagSignalLabel(
  signal: TagSignalDto,
  t: Translate,
  label: (id: string) => string,
): string {
  switch (signal.kind) {
    case "urlContains":
      return t("tagSignal.urlContains", { needle: signal.needle });
    case "assemblyRefTo":
      return t("tagSignal.assemblyRefTo", { mod: label(signal.modId) });
    case "dependsOn":
      return t("tagSignal.dependsOn", { mod: label(signal.modId) });
    default:
      return assertNever(signal);
  }
}
