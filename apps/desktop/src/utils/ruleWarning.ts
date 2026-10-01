import { formatList } from "@/i18n/format";
import type { Translate } from "@/i18n/messageDescriptor";
import { descriptor, type MessageDescriptor, renderMessage } from "@/i18n/messageDescriptor";
import type { ModKnowledgeValueKindDto } from "@/types/generated/ModKnowledgeValueKindDto";
import type { RuleWarningDto } from "@/types/generated/RuleWarningDto";
import { assertNever } from "@/utils/assertNever";

/**
 * How one ignored mod-knowledge row is phrased. `noun` kinds are a value the
 * data held that this build does not understand, slotted into the one "Ignored
 * an unknown {what}" sentence; `sentence` kinds are a row refused for another
 * reason and have a whole sentence of their own. Keyed by `satisfies Record`,
 * so a new kind is a compile error here.
 */
type KnowledgeValueMessage = { readonly noun: string } | { readonly sentence: string };

const KNOWLEDGE_VALUE_MESSAGES = {
  precedenceRule: { noun: "ruleWarning.what.precedenceRule" },
  behaviour: { noun: "ruleWarning.what.behaviour" },
  matchMode: { noun: "ruleWarning.what.matchMode" },
  gateMatchMode: { noun: "ruleWarning.what.gateMatchMode" },
  gateBehaviour: { noun: "ruleWarning.what.gateBehaviour" },
  conditionalType: { noun: "ruleWarning.what.conditionalType" },
  topLevelSection: { noun: "ruleWarning.what.topLevelSection" },
  logShapeRole: { noun: "ruleWarning.what.logShapeRole" },
  placeholderType: { noun: "ruleWarning.what.placeholderType" },
  templateInvalid: { sentence: "ruleWarning.refusedRow.templateInvalid" },
  captureMissing: { sentence: "ruleWarning.refusedRow.captureMissing" },
  markerLength: { sentence: "ruleWarning.refusedRow.markerLength" },
  idTooLong: { sentence: "ruleWarning.refusedRow.idTooLong" },
  templateUncompilable: { sentence: "ruleWarning.refusedRow.templateUncompilable" },
} as const satisfies Record<ModKnowledgeValueKindDto, KnowledgeValueMessage>;

function describeUnknownValue(
  kind: ModKnowledgeValueKindDto,
  section: string,
  value: string,
): MessageDescriptor {
  const message: KnowledgeValueMessage = KNOWLEDGE_VALUE_MESSAGES[kind];
  if ("noun" in message) {
    return descriptor("ruleWarning.unknownModKnowledgeValue", {
      what: descriptor(message.noun),
      value,
      section,
    });
  }
  return descriptor(message.sentence, { value, section });
}

/**
 * The headline for one rules-file / mod-knowledge load warning. Exhaustive
 * via {@link assertNever}; `locale` formats the dropped-rule id list. A
 * warning's English technical text is never part of it — see
 * {@link ruleWarningTechnicalDetail}.
 */
export function describeRuleWarning(warning: RuleWarningDto, locale: string): MessageDescriptor {
  switch (warning.kind) {
    case "droppedClusterRules":
      return descriptor(
        "ruleWarning.droppedClusterRules",
        { count: warning.ruleIds.length, ids: formatList(locale, warning.ruleIds) },
        warning.ruleIds.length,
      );
    case "unknownModKnowledgeValue":
      return describeUnknownValue(warning.what, warning.section, warning.value);
    case "modKnowledgeRowsOverRoleLimit":
      return descriptor(
        "ruleWarning.rowsOverRoleLimit",
        { count: warning.ignored, role: warning.role, section: warning.section },
        warning.ignored,
      );
    case "precedenceRuleMissingFramework":
      return descriptor("ruleWarning.precedenceRuleMissingFramework", {
        defType: warning.defType,
      });
    case "modKnowledgeCacheUnreadable":
      return descriptor("ruleWarning.modKnowledgeCacheUnreadableHeadline");
    default:
      return assertNever(warning);
  }
}

/** The warning's English technical text, for a line of its own; `null` when the headline says it all. */
export function ruleWarningTechnicalDetail(warning: RuleWarningDto): string | null {
  switch (warning.kind) {
    case "modKnowledgeCacheUnreadable":
      return warning.reason;
    case "droppedClusterRules":
    case "unknownModKnowledgeValue":
    case "modKnowledgeRowsOverRoleLimit":
    case "precedenceRuleMissingFramework":
      return null;
    default:
      return assertNever(warning);
  }
}

/** One headline per line, each followed by its technical-details line when it has one, for a toast's `detail`. */
export function ruleWarningsDetail(
  warnings: readonly RuleWarningDto[],
  t: Translate,
  locale: string,
): string {
  return warnings
    .flatMap((warning) => {
      const headline = renderMessage(t, describeRuleWarning(warning, locale));
      const technical = ruleWarningTechnicalDetail(warning);
      return technical === null
        ? [headline]
        : [headline, t("common.technicalDetail", { detail: technical })];
    })
    .join("\n");
}
