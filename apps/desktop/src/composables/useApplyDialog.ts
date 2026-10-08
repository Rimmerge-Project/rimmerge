// The apply dialog's own state and actions, shared with its panels through provide/inject.

import { useToast } from "primevue/usetoast";
import { computed, type InjectionKey, inject, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useLoggedOrderCheck } from "@/composables/useLoggedOrderCheck";
import { useModLabel } from "@/composables/useModLabel";
import { useTauriEvent } from "@/composables/useTauriEvent";
import { formatList } from "@/i18n/format";
import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import { usePendingActiveChangesQuery, useRescanMutation } from "@/queries/activeSet";
import { useApplyMutation, useApplyPreflightQuery } from "@/queries/apply";
import { useDashboardQuery } from "@/queries/dashboard";
import { useDefCacheCarrierQuery } from "@/queries/defCache";
import { useRulesQuery, useUpsertRuleMutation } from "@/queries/rules";
import { useVerifyOrderMutation } from "@/queries/verify";
import { RimmergeError } from "@/services/ipc";
import { useSessionStore } from "@/stores/session";
import type { ApplyReportDto } from "@/types/generated/ApplyReportDto";
import type { VerifyOperationDto } from "@/types/generated/VerifyOperationDto";
import type { VerifyProgressEventDto } from "@/types/generated/VerifyProgressEventDto";
import type { VerifyReorderDto } from "@/types/generated/VerifyReorderDto";
import type { VerifyReportDto } from "@/types/generated/VerifyReportDto";
import { defCacheBuildSeconds } from "@/utils/defCache";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { profileFilePath } from "@/utils/format";
import { joinPredictedVsObserved } from "@/utils/gameLog";
import {
  hasLoggingGaps,
  isConsoleSnapshot,
  isCutOffBeforePatchPhase,
} from "@/utils/gameLogCoverage";
import { describeSortProvenance } from "@/utils/sortProvenance";

/** What a successful apply did, for a host that reacts to it (the Launch RimWorld button). */
export type ApplyOutcome = {
  /** The request's own `writeModsConfig`: whether `ModsConfig.xml` was actually written. */
  readonly wroteModsConfig: boolean;
};

export type ApplyDialogEmit = {
  (event: "update:visible", value: boolean): void;
  (event: "applied", outcome: ApplyOutcome): void;
};

export function useApplyDialog(visible: () => boolean, emit: ApplyDialogEmit) {
  const session = useSessionStore();
  const modLabel = useModLabel();
  const { data: dashboard } = useDashboardQuery();
  const { data: defCacheCarrier } = useDefCacheCarrierQuery();
  const loggedOrder = useLoggedOrderCheck();
  const { mutateAsync: runApply, isLoading } = useApplyMutation();
  /**
   * The hard problems in the order this dialog would write, read while the
   * dialog is visible. `requiresConfirmation` is computed in Rust; the
   * dialog only asks it.
   */
  const { data: preflight, refresh: refreshPreflight } = useApplyPreflightQuery(
    () => session.selected,
    visible,
  );
  const { mutateAsync: runVerify, isLoading: verifying } = useVerifyOrderMutation();
  const { mutateAsync: upsertRule, isLoading: creatingPairRule } = useUpsertRuleMutation();
  /**
   * `upsert_rule` replaces a
   * `userDecision` pair rule at a key **wholesale**, so creating one from a
   * verify row without first reading what is already stored would silently
   * clear that rule's `overridesDeclared` (dropping it out of
   * `Layer::DeclaredOverride`) and overwrite a hand-written comment. This
   * query is what the dialog reads before it writes. Scoped to
   * `userDecision` because that is exactly the origin `upsert_rule` itself
   * replaces — an imported rule at the same key survives untouched.
   */
  const { data: userRules } = useRulesQuery(() => "userDecision");
  const toast = useToast();
  const { t, locale } = useI18n();

  /**
   * Pure copy, no behaviour change.
   * `seconds` is sourced from the last imported log's own def-cache
   * build timer when one is in hand (`session.gameLogSummary`); the
   * parenthetical is omitted entirely, rather than guessed, when no log
   * has been imported, the log is a console snapshot (it never shows a
   * whole startup, so it cannot say what the cache did), or that run never
   * (re)built the cache.
   */
  const defCacheNote = computed<string | null>(() => {
    if (!defCacheCarrier.value?.carrierModId) {
      return null;
    }
    const summary = session.gameLogSummary;
    const defCacheLines = summary && !isConsoleSnapshot(summary) ? summary.defCacheLines : [];
    const seconds = defCacheBuildSeconds(defCacheLines);
    const parenthetical =
      seconds === null ? "" : t("apply.dialog.defCacheParenthetical", { seconds });
    return t("apply.dialog.defCacheNote", { parenthetical });
  });

  /** Writes `ModsConfig.xml`; off still saves decisions and rules. */
  const writeModsConfig = ref(true);
  /**
   * Set once Apply was clicked while the order has a hard problem the user
   * has not decided yet: the confirmation panel replaces the action row
   * until they go back or confirm. Independent of
   * {@link awaitingForceConfirm}, which is about a running game and only
   * ever follows this step.
   */
  const awaitingProblemConfirm = ref(false);

  /**
   * `apply` refuses to write
   * `ModsConfig.xml` while the working active-mod set is stale
   * (`unscanned` non-empty) — this dialog says so up front, forcing
   * `writeModsConfig` off and disabling the checkbox, rather than letting the
   * user hit the real refusal after clicking Apply. `unapplied` (a scanned
   * order not yet written) is a milder, non-blocking fact, shown as an
   * added/removed list above the summary.
   */
  const { data: pendingActiveChanges } = usePendingActiveChangesQuery();
  const isStaleActiveSet = computed(() => {
    const diff = pendingActiveChanges.value?.unscanned;
    return diff !== undefined && (diff.added.length > 0 || diff.removed.length > 0);
  });
  const unappliedActiveChanges = computed(() => pendingActiveChanges.value?.unapplied ?? null);
  const hasUnappliedActiveChanges = computed(() => {
    const diff = unappliedActiveChanges.value;
    return diff !== null && (diff.added.length > 0 || diff.removed.length > 0);
  });
  const { mutateAsync: rescanNow, isLoading: rescanning } = useRescanMutation();
  const rescanError = ref<CommandErrorDescriptor | null>(null);

  async function rescanFromApplyDialog(): Promise<void> {
    rescanError.value = null;
    try {
      await rescanNow();
    } catch (error: unknown) {
      rescanError.value = describeCommandError(error);
    }
  }

  /**
   * `writeModsConfig`'s own value from just before staleness forced it off —
   * `null` means "nothing to restore" (either never forced, or already
   * restored). Restoring it matters: without it, the box would stay
   * unchecked even after an in-dialog Rescan cleared
   * {@link isStaleActiveSet}.
   */
  let writeModsConfigBeforeStale: boolean | null = null;

  watch(
    isStaleActiveSet,
    (stale) => {
      if (stale) {
        writeModsConfigBeforeStale ??= writeModsConfig.value;
        writeModsConfig.value = false;
      } else if (writeModsConfigBeforeStale !== null) {
        writeModsConfig.value = writeModsConfigBeforeStale;
        writeModsConfigBeforeStale = null;
      }
    },
    { immediate: true },
  );
  /**
   * Renders the generated merge mod into `Mods/` (or removes it when no
   * `Merge`/`ShipAsset` decision remains) as part of this apply.
   * **Starts unchecked**, even when a complete merge entry exists: writing
   * into `Mods/` is a deliberate act (the CLI's `apply --write-merge-mod`
   * is opt-in the same way). The user's choice sticks for the dialog's
   * lifetime and resets to unchecked on the next reopen.
   */
  const writeMergeMod = ref(false);
  /** Set once a first attempt was refused because the game looks running. */
  const awaitingForceConfirm = ref(false);
  const errorMessage = ref<CommandErrorDescriptor | null>(null);
  /**
   * The last successful apply's report, kept only while it carries
   * merge-mod information worth showing (a written/removed path or
   * skipped merges) — set instead of closing immediately so the user sees
   * where the merge mod landed before dismissing the dialog. `null` closes
   * the dialog right after the toast.
   */
  const lastReport = ref<ApplyReportDto | null>(null);
  /**
   * True when {@link lastReport} is the result of an apply that asked to
   * write the merge mod but came back with nothing to render — the
   * backend's write step always removes a stale generated folder when no
   * `Merge`/`ShipAsset` decision remains, which looks identical to
   * "nothing to do" from the report alone (`mergeModPath: null`,
   * `skippedMerges: []`). Tracked alongside `lastReport` instead of
   * derived from it since `ApplyReportDto` carries no flag for "removed".
   */
  const mergeModRemoved = ref(false);
  /**
   * The "Verify order" summary. `verifyReport` is
   * `null` until the first (or a fresh) run resolves; `verifyProgress`
   * tracks the `verify://progress` event stream while a pass is in
   * flight — a real-install pass can run for a long time on a large
   * install (see `VerifyProgressEventDto`'s own doc comment, whose `total`
   * also grows once mid-stream), so a bare
   * `:loading` spinner alone would read as a frozen dialog.
   */
  const verifyReport = ref<VerifyReportDto | null>(null);
  const verifyProgress = ref<VerifyProgressEventDto | null>(null);
  const verifyErrorMessage = ref<CommandErrorDescriptor | null>(null);

  useTauriEvent<VerifyProgressEventDto>("verify://progress", (payload) => {
    verifyProgress.value = payload;
  });

  const verifyProgressPercent = computed(() => {
    const progress = verifyProgress.value;
    if (!progress || progress.total === 0) {
      return 0;
    }
    return (progress.checked / progress.total) * 100;
  });

  /**
   * `VerifyReportDto.operations` is already grouped by `(mod, operation
   * identity)` on the Rust side: `VerifyOrder`'s own per-def architecture
   * reports one real operation matching several def targets (a common
   * `OR`-list head) once per def, which would render the identical headline
   * repeatedly. Grouping happens once, backend-side
   * (`apps/desktop/src-tauri/src/dto/verify.rs`), so this component just
   * iterates `operations` directly.
   */
  const verifyOperations = computed<readonly VerifyOperationDto[]>(
    () => verifyReport.value?.operations ?? [],
  );

  /**
   * Joins the just-run "Verify order"
   * predictions against the last imported log's own observed failures —
   * `(ModId, normalizeLogText(operation))` against the already-grouped
   * `VerifyOperationDto`, never a raw `Finding` and never an ungrouped log
   * line (see `utils/gameLog.ts`'s own doc comment). `null` until both a
   * verify pass and a log import have happened.
   *
   * **Never gated on {@link loggedOrder}**: the panel still renders when
   * the imported log's own order differs from the one just verified —
   * silently hiding it would lose the evidence entirely rather than flag it
   * as suspect. The template renders {@link loggedOrder}'s own warning
   * directly above this panel instead, exactly the failure the log's own
   * `load_events` field exists to catch: otherwise every
   * predicted-vs-observed row would silently compare two unrelated runs.
   */
  const predictedVsObserved = computed(() => {
    const report = verifyReport.value;
    const summary = session.gameLogSummary;
    if (!report || !summary) {
      return null;
    }
    return joinPredictedVsObserved(report.operations, summary.patchFailures);
  });

  /**
   * Why "predicted, not observed" is weaker evidence than it reads, when the
   * imported log is known to be missing part of the session: a console
   * snapshot (earlier entries are gone), a log where the game stopped
   * writing messages, or a `Player.log` that ended without a clean exit
   * before any patch result was logged (the patch phase may never have run).
   * Every limit that applies is listed (a cut-off log can also have gaps),
   * most specific first; empty for a log with none. The panel still renders
   * — hiding the evidence would lose it — and states the limits.
   */
  const predictedVsObservedCaveats = computed<MessageDescriptor[]>(() => {
    const summary = session.gameLogSummary;
    if (!summary) {
      return [];
    }
    if (isConsoleSnapshot(summary)) {
      return [descriptor("apply.diff.pvoSnapshotNote")];
    }
    const caveats: MessageDescriptor[] = [];
    if (isCutOffBeforePatchPhase(summary.coverage)) {
      caveats.push(descriptor("apply.diff.pvoCutOffNote"));
    }
    if (hasLoggingGaps(summary)) {
      caveats.push(descriptor("apply.diff.pvoGapsNote"));
    }
    return caveats;
  });

  /**
   * The counterfactual phase's own raw stats, or `null` when it did
   * nothing at all (the phase was off, or nothing survived reconciliation
   * for it to look at) — worth surfacing rather than keeping to the CLI:
   * the phase is what turns an unactionable `Unknown` row into an
   * order-fixable one, so a reader seeing few order-fixable rows should
   * be able to tell "the experiment ran and found nothing" from "the
   * experiment never ran". Returns the numbers only, never a built
   * sentence: `ApplyOrderDiff.vue` renders it through `t()`, so the
   * message re-localizes correctly after a language switch (see
   * `apps/desktop/CLAUDE.md`'s "helpers return data, never a rendered
   * string" rule).
   */
  const verifyCounterfactualStats = computed(() => {
    const stats = verifyReport.value?.counterfactual;
    if (!stats) {
      return null;
    }
    const skipped = stats.skippedTooManyMods + stats.skippedCoOwner;
    if (stats.jobs === 0 && skipped === 0) {
      return null;
    }
    return {
      jobs: stats.jobs,
      resolved: stats.resolved,
      demotedDeadTargets: stats.demotedDeadTargets,
      skipped,
      rejectedForRegression: stats.rejectedForRegression,
    };
  });

  const verifyCauseCounts = computed(() => {
    const counts = { removedBy: 0, notYetInjected: 0, deadTarget: 0, unknown: 0 };
    for (const operation of verifyOperations.value) {
      for (const def of operation.defs) {
        counts[def.cause.kind] += 1;
      }
    }
    return counts;
  });

  /**
   * A judgment call:
   * an operation is order-fixable only when *every* def target under it is
   * — `RemovedBy`/`NotYetInjected` name a concrete other mod whose relative
   * position is the problem, so reordering may resolve them, and each such
   * operation gets its own row. `DeadTarget`/`Unknown` are not: on the real
   * install these are overwhelmingly stale patches against defs that no
   * longer exist, nothing reordering can fix, so listing what can be well
   * over a hundred of them one row each would drown the
   * summary — grouped by mod instead, a count per mod rather than a wall
   * of identical-looking rows. A mixed operation (rare — would mean the
   * same physical operation classified two different ways across its own
   * def targets) is treated as not order-fixable rather than guessing
   * which cause represents it as a whole.
   */
  function isOrderFixableOperation(operation: VerifyOperationDto): boolean {
    return operation.defs.every(
      (def) => def.cause.kind === "removedBy" || def.cause.kind === "notYetInjected",
    );
  }

  /**
   * One predicted-failure operation, plus the
   * distinct reorders its def targets ask for — the rows the "Create pair
   * rule" buttons hang off.
   */
  type ReorderRow = {
    /** `${modId}|${operation}`, the `v-for` key and the "who created this rule" identity. */
    key: string;
    operation: VerifyOperationDto;
    reorders: readonly VerifyReorderDto[];
  };

  /** `after|before` — one pair rule's identity, independent of which row offered it. */
  function pairKeyOf(reorder: VerifyReorderDto): string {
    return `${reorder.after}|${reorder.before}`;
  }

  function rowOf(operation: VerifyOperationDto): ReorderRow {
    return {
      key: `${operation.modId}|${operation.operation}`,
      operation,
      reorders: distinctReorders(operation),
    };
  }

  /**
   * The distinct reorders under one operation, in def order.
   *
   * **Deduplicated, not assumed unique**: the common shape is an `OR`-list
   * head matching many defs all blocked by the same mod — one relation to
   * state, so one button. A genuinely mixed operation (different defs
   * blocked by different mods) gets one button each rather than silently
   * offering only the first. Direction is never computed here: each
   * `VerifyReorderDto` arrives already oriented from
   * `rim_resolve::ledger::suggest` (see `dto/verify.rs`).
   */
  function distinctReorders(operation: VerifyOperationDto): readonly VerifyReorderDto[] {
    const seen = new Map<string, VerifyReorderDto>();
    for (const def of operation.defs) {
      if (def.reorder && !seen.has(pairKeyOf(def.reorder))) {
        seen.set(pairKeyOf(def.reorder), def.reorder);
      }
    }
    return [...seen.values()];
  }

  /** Operations whose *every* def target is order-fixable — their own section. */
  const orderFixableOperations = computed<readonly ReorderRow[]>(() =>
    verifyOperations.value.filter(isOrderFixableOperation).map(rowOf),
  );

  /**
   * Operations the display rule above files under
   * "not order-fixable" that nonetheless carry a reorder for *some* of
   * their def targets. They keep their place in the grouped-by-mod
   * section — a mixed operation is genuinely not fully fixable by a
   * reorder — but they get the same button, because the fix is real for
   * the def targets it covers and `apps/cli`'s `verify` prints the very
   * same `rule set-pair` line for them. Gating the button on
   * {@link isOrderFixableOperation}, a presentation judgment, would make the
   * two surfaces disagree about whether a fix exists.
   */
  const partlyFixableOperations = computed<readonly ReorderRow[]>(() =>
    verifyOperations.value
      .filter((operation) => !isOrderFixableOperation(operation))
      .map(rowOf)
      .filter((row) => row.reorders.length > 0),
  );

  /**
   * Pair rules created from this verify report, `pairKey` -> the
   * {@link ReorderRow.key} that created it: the row that made the rule
   * reads "Rule created", any other row asking for the same relation reads
   * "Rule already exists". Cleared whenever a fresh verify pass starts
   * (the report it annotates is gone) and whenever the dialog reopens.
   */
  const createdPairRules = ref(new Map<string, string>());
  /** `pairKey` -> the failure from a rejected `upsert_rule`. */
  const pairRuleErrors = ref(new Map<string, CommandErrorDescriptor>());

  /**
   * `pairKey`s that already carried a `userDecision` pair rule when the
   * current verify report was produced — a **snapshot**, taken once in
   * {@link verifyOrderNow}, never a live read of {@link userRules}.
   *
   * Live would be wrong twice over: the mutation invalidates that query,
   * so a rule created here would appear in it moments later and relabel
   * the very row that created it "already exists"; and the honest question
   * this answers is what was true of the order the prediction was computed
   * against.
   */
  const existingUserPairKeys = ref(new Set<string>());

  /**
   * Writes one `userDecision` pair rule from a verified reorder — the
   * accept half of the sort -> verify -> accept -> re-sort
   * loop. `upsert_rule` re-sorts the session and emits `session://changed`
   * on its own, so every query behind this dialog refreshes without
   * anything further here; the verify *report* is deliberately **not**
   * re-run automatically (a minute-long pass is the user's own call to
   * make, and the button's own confirmation says so).
   *
   * **An existing rule at the same key is preserved, not replaced**:
   * `upsert_rule` writes a whole `PairRule`, so an
   * existing row's `overridesDeclared` and its hand-written comment are
   * carried forward verbatim; the suggestion's rationale is only used when
   * there is no comment to keep. The button is normally already disabled
   * for such a key ({@link existingUserPairKeys}), so this is the guard for
   * the one window where it is not: {@link userRules} still loading (or
   * failed) while a minute-long verify pass finishes and the user clicks.
   */
  async function createPairRule(row: ReorderRow, reorder: VerifyReorderDto): Promise<void> {
    const pairKey = pairKeyOf(reorder);
    if (createdPairRules.value.has(pairKey)) {
      return;
    }
    const existing = (userRules.value?.pairs ?? []).find(
      (pair) => pair.after === reorder.after && pair.before === reorder.before,
    );
    const keptComment = existing?.comment?.trim() ? existing.comment : null;
    pairRuleErrors.value.delete(pairKey);
    try {
      await upsertRule({
        kind: "pair",
        after: reorder.after,
        before: reorder.before,
        origin: "userDecision",
        comment: keptComment ?? reorder.rationale,
        promotedFrom: null,
        alreadyPromoted: false,
        overridesDeclared: existing?.overridesDeclared ?? false,
      });
      createdPairRules.value.set(pairKey, row.key);
    } catch (error: unknown) {
      pairRuleErrors.value.set(pairKey, describeCommandError(error));
    }
  }

  const groupedByModOperations = computed(() => {
    const groups = new Map<string, VerifyOperationDto[]>();
    for (const operation of verifyOperations.value) {
      if (isOrderFixableOperation(operation)) {
        continue;
      }
      const forMod = groups.get(operation.modId) ?? [];
      forMod.push(operation);
      groups.set(operation.modId, forMod);
    }
    return [...groups.entries()]
      .map(([modId, operations]) => ({
        modId,
        operations,
        defTargetCount: operations.reduce((sum, operation) => sum + operation.defs.length, 0),
      }))
      .sort((a, b) => b.operations.length - a.operations.length);
  });

  async function verifyOrderNow(): Promise<void> {
    verifyErrorMessage.value = null;
    verifyProgress.value = null;
    // A fresh pass replaces the report these annotate; a "Rule created"
    // badge left over from the previous one would point at rows that no
    // longer exist.
    createdPairRules.value.clear();
    pairRuleErrors.value.clear();
    try {
      verifyReport.value = await runVerify({ source: session.selected });
      existingUserPairKeys.value = new Set(
        (userRules.value?.pairs ?? []).map((pair) => `${pair.after}|${pair.before}`),
      );
    } catch (error: unknown) {
      verifyErrorMessage.value = describeCommandError(error);
    }
  }

  const modsConfigPath = computed(() => session.paths?.modsConfig ?? "");
  const decisionsPath = computed(() =>
    session.paths ? profileFilePath(session.paths.profileDir, "decisions.json") : "",
  );
  const rulesPath = computed(() =>
    session.paths ? profileFilePath(session.paths.profileDir, "rules.json") : "",
  );
  const movedMods = computed(() => dashboard.value?.movedMods ?? 0);
  const sortProvenance = computed(() =>
    dashboard.value ? describeSortProvenance(dashboard.value.sortProvenance, t) : "",
  );
  /** Shown as information only: needs-input findings no longer gate Apply. */
  const needsInput = computed(
    () => dashboard.value?.ledgerStats[session.selected]?.needsInput ?? 0,
  );

  // Every reopen starts completely clean — a leftover force-warning,
  // error, checkbox choice, or merge-mod summary from a previous apply
  // must never bleed into the next one (the dialog stays mounted, just
  // hidden, between opens).
  watch(visible, (isVisible) => {
    if (isVisible) {
      writeModsConfig.value = !isStaleActiveSet.value;
      // If already stale on open, `true` (the default) is the true
      // "before" baseline a later un-staling (an in-dialog Rescan) must
      // restore to — leaving this `null` here would give the un-force
      // watcher above nothing to restore, so the checkbox would stay forced
      // off even after the working set stopped being stale (the e2e
      // active-set spec exercises exactly this "already stale on open" order).
      writeModsConfigBeforeStale = isStaleActiveSet.value ? true : null;
      writeMergeMod.value = false;
      awaitingProblemConfirm.value = false;
      awaitingForceConfirm.value = false;
      errorMessage.value = null;
      lastReport.value = null;
      mergeModRemoved.value = false;
      verifyReport.value = null;
      verifyProgress.value = null;
      verifyErrorMessage.value = null;
      createdPairRules.value.clear();
      pairRuleErrors.value.clear();
      existingUserPairKeys.value = new Set();
      rescanError.value = null;
    }
  });

  function close(): void {
    emit("update:visible", false);
  }

  function announceSuccess(
    report: ApplyReportDto,
    removedMergeMod: boolean,
    wroteModsConfig: boolean,
  ): void {
    const lines: string[] = [];
    // The report always names the ModsConfig path; listing it when nothing was
    // written there reads as if it had been (the owner's misreading).
    if (wroteModsConfig) {
      lines.push(t("apply.result.toastModsConfigPath", { path: report.modsConfigPath }));
      if (report.backupPath) {
        lines.push(t("apply.result.toastBackupPath", { path: report.backupPath }));
      }
    }
    lines.push(
      t("apply.result.toastDecisionsPath", { path: report.decisionsPath }),
      t("apply.result.toastRulesPath", { path: report.rulesPath }),
    );
    if (report.mergeModPath) {
      lines.push(t("apply.result.mergeModPath", { path: report.mergeModPath }));
    } else if (removedMergeMod) {
      lines.push(t("apply.result.mergeModRemoved"));
    }
    if (report.mergeModBackupPath) {
      lines.push(t("apply.result.mergeModBackupPath", { path: report.mergeModBackupPath }));
    }
    if (report.skippedMerges.length > 0) {
      lines.push(
        t("apply.result.toastSkippedStillNeedsInput", {
          list: formatList(locale.value, report.skippedMerges),
        }),
      );
    }
    toast.add({
      severity: "success",
      summary: wroteModsConfig
        ? t("apply.result.toastAppliedSummary")
        : t("apply.result.toastSavedSummary"),
      detail: lines.join("\n"),
      life: 8000,
    });
  }

  /**
   * Closes the dialog after a successful apply, unless the report carries
   * merge-mod information worth showing first — then it stays open with
   * that summary in place of the form, dismissed only by the user.
   * `wroteMergeMod` is the request's own `writeMergeMod` flag, needed to
   * tell "removed" (asked to write, nothing came back) from "never asked";
   * `wroteModsConfig` likewise is the request's own `writeModsConfig`.
   */
  function finishSuccess(
    report: ApplyReportDto,
    wroteMergeMod: boolean,
    wroteModsConfig: boolean,
  ): void {
    const removed =
      wroteMergeMod && report.mergeModPath === null && report.skippedMerges.length === 0;
    announceSuccess(report, removed, wroteModsConfig);
    // Once per successful apply, before the dialog closes or switches to the merge-mod summary.
    emit("applied", { wroteModsConfig });
    if (report.mergeModPath !== null || report.skippedMerges.length > 0 || removed) {
      lastReport.value = report;
      mergeModRemoved.value = removed;
      return;
    }
    close();
  }

  /**
   * Whether the order has a hard problem the user still has to answer.
   * Always asks a current preflight: `refresh` fetches when the held one is
   * stale (an invalidation keeps the old data and only marks it stale) or
   * errored, and joins a fetch already in flight, so a decision made since
   * the dialog opened can never be missed. A failed fetch is shown and stops
   * the apply (`null`) rather than skipping the check; neither data nor an
   * error is a broken invariant, never "nothing to confirm".
   */
  async function needsProblemConfirmation(): Promise<boolean | null> {
    const fresh = await refreshPreflight();
    if (fresh.error) {
      errorMessage.value = describeCommandError(fresh.error);
      return null;
    }
    if (!fresh.data) {
      throw new Error("the apply preflight resolved with neither data nor an error");
    }
    return fresh.data.requiresConfirmation;
  }

  /** The Apply click: confirm unanswered hard problems first, unless nothing is written. */
  async function submit(): Promise<void> {
    errorMessage.value = null;
    if (writeModsConfig.value) {
      const needsConfirmation = await needsProblemConfirmation();
      if (needsConfirmation === null) {
        return;
      }
      if (needsConfirmation) {
        awaitingProblemConfirm.value = true;
        return;
      }
    }
    await performApply();
  }

  /** "Apply anyway" on the confirmation panel. */
  async function confirmProblems(): Promise<void> {
    awaitingProblemConfirm.value = false;
    // The panel is only reachable with the box on; a stale-forced-off box means
    // nothing is left to confirm.
    if (!writeModsConfig.value) {
      return;
    }
    await performApply();
  }

  /** "Go back" on the confirmation panel. */
  function goBackFromConfirm(): void {
    awaitingProblemConfirm.value = false;
  }

  async function performApply(): Promise<void> {
    errorMessage.value = null;
    try {
      const wroteModsConfig = writeModsConfig.value;
      const wroteMergeMod = writeMergeMod.value;
      const report = await runApply({
        source: session.selected,
        writeModsConfig: wroteModsConfig,
        writeMergeMod: wroteMergeMod,
        force: false,
      });
      finishSuccess(report, wroteMergeMod, wroteModsConfig);
    } catch (error: unknown) {
      if (error instanceof RimmergeError && error.code === "rimworld_running") {
        awaitingForceConfirm.value = true;
        return;
      }
      errorMessage.value = describeCommandError(error);
    }
  }

  async function writeAnyway(): Promise<void> {
    errorMessage.value = null;
    try {
      const report = await runApply({
        source: session.selected,
        writeModsConfig: true,
        writeMergeMod: writeMergeMod.value,
        force: true,
      });
      finishSuccess(report, writeMergeMod.value, true);
    } catch (error: unknown) {
      errorMessage.value = describeCommandError(error);
    }
  }

  return {
    session,
    modLabel,
    dashboard,
    defCacheCarrier,
    loggedOrder,
    runApply,
    isLoading,
    runVerify,
    verifying,
    upsertRule,
    creatingPairRule,
    userRules,
    toast,
    defCacheNote,
    writeModsConfig,
    preflight,
    awaitingProblemConfirm,
    pendingActiveChanges,
    isStaleActiveSet,
    unappliedActiveChanges,
    hasUnappliedActiveChanges,
    rescanNow,
    rescanning,
    rescanError,
    rescanFromApplyDialog,
    writeMergeMod,
    awaitingForceConfirm,
    errorMessage,
    lastReport,
    mergeModRemoved,
    verifyReport,
    verifyProgress,
    verifyErrorMessage,
    verifyProgressPercent,
    verifyOperations,
    predictedVsObserved,
    predictedVsObservedCaveats,
    verifyCounterfactualStats,
    verifyCauseCounts,
    isOrderFixableOperation,
    pairKeyOf,
    rowOf,
    distinctReorders,
    orderFixableOperations,
    partlyFixableOperations,
    createdPairRules,
    pairRuleErrors,
    existingUserPairKeys,
    createPairRule,
    groupedByModOperations,
    verifyOrderNow,
    modsConfigPath,
    decisionsPath,
    rulesPath,
    movedMods,
    sortProvenance,
    needsInput,
    close,
    announceSuccess,
    finishSuccess,
    submit,
    confirmProblems,
    goBackFromConfirm,
    writeAnyway,
  };
}

export type ApplyDialogState = ReturnType<typeof useApplyDialog>;

export const APPLY_DIALOG_KEY: InjectionKey<ApplyDialogState> = Symbol("ApplyDialog");

export function useApplyDialogState(): ApplyDialogState {
  const state = inject(APPLY_DIALOG_KEY);
  if (!state) {
    throw new Error("useApplyDialogState: no ApplyDialog provided this state");
  }
  return state;
}
