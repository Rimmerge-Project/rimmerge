// The one place RowEditor's target-keyed and free-standing modes are told apart.

import { type ComputedRef, computed } from "vue";
import { defRefOf } from "@/types/brands";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";
import type { AssignmentRowDto } from "@/types/generated/AssignmentRowDto";
import type { CoverageRowDto } from "@/types/generated/CoverageRowDto";
import type { ExistingMatchDto } from "@/types/generated/ExistingMatchDto";
import type { TargetRefDto } from "@/types/generated/TargetRefDto";
import type { WinnerDto } from "@/types/generated/WinnerDto";

/**
 * One normalized view of "which row this editor edits", telling the
 * target-keyed and free-standing modes apart exactly once so the rest of
 * this component reads `descriptor.value.*` uniformly instead of
 * branching on `coverageRow`/`ownDefName` at every use site. A
 * discriminated union (`type`) so a `descriptor.value.type === "target"`
 * check narrows `target`/`matches`/`winner` for TypeScript without ever
 * needing a non-null assertion on the optional `coverageRow` prop.
 */
export type RowDescriptor =
  | {
      type: "target";
      title: string;
      defaultDefName: string;
      existing: AssignmentRowDto | undefined;
      target: TargetRefDto;
      key: string;
      matches: ExistingMatchDto[];
      winner: WinnerDto | null;
      isOverride: boolean;
    }
  | {
      type: "own";
      title: string;
      defaultDefName: string;
      existing: AssignmentRowDto | undefined;
      target: null;
      key: string;
      matches: ExistingMatchDto[];
      winner: WinnerDto | null;
      isOverride: boolean;
    };

export function useRowDescriptor(
  props: () => {
    assignment: AssignmentDetailDto;
    coverageRow: CoverageRowDto | undefined;
    ownDefName: string | null | undefined;
  },
  section: ComputedRef<AssignmentDetailDto["sections"][number] | undefined>,
  /** The caller's own `useI18n().t` — this composable can't call `useI18n()` itself (see the module-level doc comment's own rule), only needed for the free-standing "no defName picked yet" title. */
  t: (key: string) => string,
): ComputedRef<RowDescriptor> {
  const descriptor = computed<RowDescriptor>(() => {
    const { assignment, coverageRow, ownDefName } = props();
    if (coverageRow) {
      const { target } = coverageRow;
      return {
        type: "target",
        title: defRefOf(target.def.defType, target.def.defName),
        defaultDefName: `${assignment.folderName}_${target.def.defName}`,
        existing: section.value?.rows.find(
          (entry) =>
            entry.target.def.defType === target.def.defType &&
            entry.target.def.defName === target.def.defName,
        )?.row,
        target,
        key: `target:${target.def.defType}/${target.def.defName}`,
        matches: coverageRow.matches,
        winner: coverageRow.winner,
        isOverride: coverageRow.intent === "override",
      };
    }
    const name = ownDefName ?? null;
    return {
      type: "own",
      title: name ?? t("assignments.rowEditor.newRowTitle"),
      defaultDefName: name ?? "",
      existing:
        name === null
          ? undefined
          : section.value?.standaloneRows.find((entry) => entry.row.defName === name)?.row,
      target: null,
      key: `own:${name ?? ""}`,
      matches: [],
      winner: null,
      isOverride: false,
    };
  });
  return descriptor;
}
