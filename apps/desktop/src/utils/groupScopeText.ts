import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";

type Translate = (key: string, named: Record<string, unknown>, plural: number) => string;

/** Which way a group-level action goes. */
export type GroupScopeAction = "activate" | "deactivate";

/** The mods a group-level action will plan over: the inactive ids to activate, the active ones to deactivate. */
export function scopeIds(plan: GroupScopePlanDto, action: GroupScopeAction): string[] {
  return action === "activate" ? plan.inactiveIds : plan.activeIds;
}

/**
 * The lines the Activate or Deactivate dialog shows above its plan, saying how far the action
 * reaches: how many mods, whether subgroups are included, how many in-scope mods are not
 * installed (activate only) and how many of the mods the action covers the list's current
 * filter hides. Every line holds one number, so each is worded by its own plural rule.
 */
export function groupScopeLines(
  t: Translate,
  plan: GroupScopePlanDto,
  action: GroupScopeAction,
  hiddenCount: number,
): string[] {
  const count = scopeIds(plan, action).length;
  const hasSubgroups = plan.groupCount > 1;
  const lines: string[] = [];
  if (action === "activate") {
    lines.push(
      t(
        hasSubgroups ? "mods.bulk.scope.activateTree" : "mods.bulk.scope.activateGroup",
        { count },
        count,
      ),
    );
    if (plan.notInstalled > 0) {
      lines.push(
        t("mods.bulk.scope.notInstalled", { count: plan.notInstalled }, plan.notInstalled),
      );
    }
  } else {
    lines.push(
      t(
        hasSubgroups ? "mods.bulk.scope.deactivateTree" : "mods.bulk.scope.deactivateGroup",
        { count },
        count,
      ),
    );
  }
  if (hiddenCount > 0) {
    lines.push(t("mods.bulk.scope.hidden", { count: hiddenCount }, hiddenCount));
  }
  return lines;
}
