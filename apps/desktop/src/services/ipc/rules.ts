// Rules, rule databases, RimSort import, and tags.

import type { DeleteRuleRequestDto } from "@/types/generated/DeleteRuleRequestDto";
import type { ImportReportDto } from "@/types/generated/ImportReportDto";
import type { OrphanedDecisionDto } from "@/types/generated/OrphanedDecisionDto";
import type { RecommendedRulesReportDto } from "@/types/generated/RecommendedRulesReportDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import type { RefreshRuleDatabasesRequestDto } from "@/types/generated/RefreshRuleDatabasesRequestDto";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";
import type { RuleDatabaseRefreshResultDto } from "@/types/generated/RuleDatabaseRefreshResultDto";
import type { RuleDatabaseViewDto } from "@/types/generated/RuleDatabaseViewDto";
import type { RuleDto } from "@/types/generated/RuleDto";
import type { RuleFilterDto } from "@/types/generated/RuleFilterDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import type { RuleSetDto } from "@/types/generated/RuleSetDto";
import type { SetManualTagRequestDto } from "@/types/generated/SetManualTagRequestDto";
import type { TaggingDto } from "@/types/generated/TaggingDto";
import { call } from "./core";

/** Lists every rule in effect, optionally filtered to one origin. */
export function listRules(filter: RuleFilterDto): Promise<RuleSetDto> {
  return call("list_rules", { filter });
}

/** Imports RimSort's three database files, replacing any previously imported rules. */
export function importRimSort(paths: RimSortPathsDto): Promise<ImportReportDto> {
  return call("import_rimsort", { paths });
}

/** Adds or replaces a rule, persisting the change. */
export function upsertRule(rule: RuleDto): Promise<RuleSetDto> {
  return call("upsert_rule", { rule });
}

/** Removes a rule, persisting the change. */
export function deleteRule(request: DeleteRuleRequestDto): Promise<RuleSetDto> {
  return call("delete_rule", { request });
}

/**
 * Promotes an imported pair or placement rule to a `UserDecision`-owned
 * copy, persisting the change. A no-op (returns the rule set unchanged)
 * when no imported rule matches `key`, a `UserDecision` rule already
 * exists at that key, or `key` names an incompatibility.
 */
export function promoteImportedRule(key: RuleKeyDto): Promise<RuleSetDto> {
  return call("promote_imported_rule", { key });
}

/**
 * Decisions whose key no longer matches a live finding in either order's
 * ledger — prunable via {@link revertDecision}.
 */
export function listOrphanedDecisions(): Promise<OrphanedDecisionDto[]> {
  return call("list_orphaned_decisions");
}

/**
 * Each rule database's cache status (enabled, last fetched, size, sha,
 * staleness, last failure), enriched with this profile's own re-import
 * signal.
 */
export function getRuleDatabases(): Promise<RuleDatabaseViewDto[]> {
  return call("get_rule_databases");
}

/**
 * Refreshes rule databases into the global cache, honouring the network
 * switch and each source's own fetch toggle: exactly the sources
 * `request` names, or every enabled source when it is omitted (the
 * Databases card's Refresh button). A notice that lists sources passes
 * exactly those, so a click never downloads one the notice did not
 * mention. Never rejects on a per-source failure — that comes back as a
 * `"failed"` outcome inside the returned list, exactly like every other
 * outcome.
 */
export function refreshRuleDatabases(
  request?: RefreshRuleDatabasesRequestDto,
): Promise<RuleDatabaseRefreshResultDto[]> {
  return call("refresh_rule_databases", request === undefined ? undefined : { request });
}

/**
 * What the Dashboard's "Get the recommended rules" step shows for the
 * loaded profile. Derived in Rust; `inProgress` while a click is running.
 */
export function getRecommendedRulesStep(): Promise<RecommendedRulesStepDto> {
  return call("get_recommended_rules_step");
}

/**
 * Turns on, downloads (only what is missing) and imports the recommended
 * rule databases. Emits `rules://recommended-progress` while it runs. A
 * per-source download failure comes back inside the report; a closed
 * network gate, a damaged settings file or a second concurrent run rejects.
 */
export function getRecommendedRules(): Promise<RecommendedRulesReportDto> {
  return call("get_recommended_rules");
}

/** Remembers, for the loaded profile, that the step was skipped. Idempotent. */
export function skipRecommendedRulesStep(): Promise<void> {
  return call("skip_recommended_rules_step");
}

/** The current tag assignments (inference plus manual overrides). */
export function listTags(): Promise<TaggingDto> {
  return call("list_tags");
}

/** Sets (or replaces) a manual tag override, persisting the change. */
export function setManualTag(request: SetManualTagRequestDto): Promise<TaggingDto> {
  return call("set_manual_tag", { request });
}
