// Relative imports on purpose — `e2e/` sits outside `tsconfig.app.json`'s
// `include`, so it can't rely on that config's `@/*` path alias. Type
// imports only: everything here (including `FindingKindDto` et al.) is
// erased at compile time, which matters — see the note on
// `installScenario` below.
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { IpcFixtures } from "../../src/services/ipc.mock";
import type { ActionDto } from "../../src/types/generated/ActionDto";
import type { ActivateRequestDto } from "../../src/types/generated/ActivateRequestDto";
import type { AddAssignmentSectionRequestDto } from "../../src/types/generated/AddAssignmentSectionRequestDto";
import type { AppConfigDto } from "../../src/types/generated/AppConfigDto";
import type { AppLinkTargetDto } from "../../src/types/generated/AppLinkTargetDto";
import type { ApplyPreflightDto } from "../../src/types/generated/ApplyPreflightDto";
import type { ApplyRequestDto } from "../../src/types/generated/ApplyRequestDto";
import type { AppSettingsDto } from "../../src/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "../../src/types/generated/AppSettingsResponseDto";
import type { AssignmentSchemaDto } from "../../src/types/generated/AssignmentSchemaDto";
import type { AssignmentSkipDto } from "../../src/types/generated/AssignmentSkipDto";
import type { CaveatDto } from "../../src/types/generated/CaveatDto";
import type { CheckForUpdateOutcomeDto } from "../../src/types/generated/CheckForUpdateOutcomeDto";
import type { ClearAssignmentRowRequestDto } from "../../src/types/generated/ClearAssignmentRowRequestDto";
import type { CopyAssignmentRowFromRequestDto } from "../../src/types/generated/CopyAssignmentRowFromRequestDto";
import type { CreateAssignmentRequestDto } from "../../src/types/generated/CreateAssignmentRequestDto";
import type { CreatePatchRequestDto } from "../../src/types/generated/CreatePatchRequestDto";
import type { DeactivateRequestDto } from "../../src/types/generated/DeactivateRequestDto";
import type { DecidePatchRequestDto } from "../../src/types/generated/DecidePatchRequestDto";
import type { DecideRequestDto } from "../../src/types/generated/DecideRequestDto";
import type { DefConflictViewRequestDto } from "../../src/types/generated/DefConflictViewRequestDto";
import type { DefGraphicDto } from "../../src/types/generated/DefGraphicDto";
import type { DefTextureDto } from "../../src/types/generated/DefTextureDto";
import type { DeleteRuleRequestDto } from "../../src/types/generated/DeleteRuleRequestDto";
import type { DiffClassDto } from "../../src/types/generated/DiffClassDto";
import type { EntryKindDto } from "../../src/types/generated/EntryKindDto";
import type { ExportAssignmentRequestDto } from "../../src/types/generated/ExportAssignmentRequestDto";
import type { ExportPatchRequestDto } from "../../src/types/generated/ExportPatchRequestDto";
import type { FaceAvailabilityDto } from "../../src/types/generated/FaceAvailabilityDto";
import type { FindingDto } from "../../src/types/generated/FindingDto";
import type { FindingKindDto } from "../../src/types/generated/FindingKindDto";
import type { GameLogSummaryDto } from "../../src/types/generated/GameLogSummaryDto";
import type { GraphicFaceDto } from "../../src/types/generated/GraphicFaceDto";
import type { GraphicFacesDto } from "../../src/types/generated/GraphicFacesDto";
import type { GraphicSlotDto } from "../../src/types/generated/GraphicSlotDto";
import type { GraphicVariantDto } from "../../src/types/generated/GraphicVariantDto";
import type { InspectDefRequestDto } from "../../src/types/generated/InspectDefRequestDto";
import type { LaunchNetworkChecksOutcomeDto } from "../../src/types/generated/LaunchNetworkChecksOutcomeDto";
import type { MergeChoiceDto } from "../../src/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "../../src/types/generated/MergeFieldDto";
import type { MergeModEntryDto } from "../../src/types/generated/MergeModEntryDto";
import type { MergeOwnerDto } from "../../src/types/generated/MergeOwnerDto";
import type { MergePreviewRequestDto } from "../../src/types/generated/MergePreviewRequestDto";
import type { MergeStateDto } from "../../src/types/generated/MergeStateDto";
import type { ModLinkKindDto } from "../../src/types/generated/ModLinkKindDto";
import type { NotificationDto } from "../../src/types/generated/NotificationDto";
import type { NotificationKeyDto } from "../../src/types/generated/NotificationKeyDto";
import type { NotificationKindDto } from "../../src/types/generated/NotificationKindDto";
import type { OrderSourceDto } from "../../src/types/generated/OrderSourceDto";
import type { PairRuleDto } from "../../src/types/generated/PairRuleDto";
import type { PlacementRuleDto } from "../../src/types/generated/PlacementRuleDto";
import type { PreflightItemDto } from "../../src/types/generated/PreflightItemDto";
import type { PreviewMergeModFileRequestDto } from "../../src/types/generated/PreviewMergeModFileRequestDto";
import type { ProgressEventDto } from "../../src/types/generated/ProgressEventDto";
import type { RationaleDto } from "../../src/types/generated/RationaleDto";
import type { ReadDefTextureRequestDto } from "../../src/types/generated/ReadDefTextureRequestDto";
import type { RefreshRuleDatabasesRequestDto } from "../../src/types/generated/RefreshRuleDatabasesRequestDto";
import type { RemoveAssignmentSectionRequestDto } from "../../src/types/generated/RemoveAssignmentSectionRequestDto";
import type { ResolutionStatusDto } from "../../src/types/generated/ResolutionStatusDto";
import type { ResolveDefGraphicRequestDto } from "../../src/types/generated/ResolveDefGraphicRequestDto";
import type { RimSortPathsDto } from "../../src/types/generated/RimSortPathsDto";
import type { RuleDatabaseRefreshResultDto } from "../../src/types/generated/RuleDatabaseRefreshResultDto";
import type { RuleDatabaseViewDto } from "../../src/types/generated/RuleDatabaseViewDto";
import type { RuleDto } from "../../src/types/generated/RuleDto";
import type { RuleKeyDto } from "../../src/types/generated/RuleKeyDto";
import type { RuleSetDto } from "../../src/types/generated/RuleSetDto";
import type { SectionReferenceDto } from "../../src/types/generated/SectionReferenceDto";
import type { SetAssignmentRowRequestDto } from "../../src/types/generated/SetAssignmentRowRequestDto";
import type { SetManualTagRequestDto } from "../../src/types/generated/SetManualTagRequestDto";
import type { SetMergeChoicesRequestDto } from "../../src/types/generated/SetMergeChoicesRequestDto";
import type { SettingsDto } from "../../src/types/generated/SettingsDto";
import type { SkippedImportDto } from "../../src/types/generated/SkippedImportDto";
import type { SlotSourceDto } from "../../src/types/generated/SlotSourceDto";
import type { TextureDto } from "../../src/types/generated/TextureDto";
import type { UpdateAssignmentRequestDto } from "../../src/types/generated/UpdateAssignmentRequestDto";
import type { UpdatePatchRequestDto } from "../../src/types/generated/UpdatePatchRequestDto";
import type { VariantLabelDto } from "../../src/types/generated/VariantLabelDto";
import type { VerifyOperationDto } from "../../src/types/generated/VerifyOperationDto";
import type { VerifyReportDto } from "../../src/types/generated/VerifyReportDto";
import type { VerifyRequestDto } from "../../src/types/generated/VerifyRequestDto";

declare global {
  interface Window {
    /** Tauri's in-page IPC bridge (installed by `mockIPC`) — used to emit mocked events from a fixture. */
    __TAURI_INTERNALS__: { invoke(command: string, args?: unknown): Promise<unknown> };
    /**
     * When set, `load_project` emits each of these `project://progress`
     * payloads (in order) and then waits for {@link __RELEASE_LOAD_PROJECT__}
     * to be called before it resolves — so a spec can read the setup page's
     * scan-stage caption while the load is still running.
     */
    __LOAD_PROJECT_PROGRESS__?: ProgressEventDto[];
    /** Set by the mock while it holds `load_project`; calling it lets the load finish. */
    __RELEASE_LOAD_PROJECT__?: () => void;
    /** Every `decide` payload the mock received, in order — for the inbox keyboard-walk spec. */
    __DECIDE_CALLS__?: DecideRequestDto[];
    /** Every `revert_decision` key the mock received, in order. */
    __REVERT_CALLS__?: string[];
    /** Every `apply` payload the mock received, in order — for the apply-dialog spec. */
    __APPLY_CALLS__?: ApplyRequestDto[];
    /**
     * When `true`, the mock's `apply` fixture rejects with
     * `rimworld_running` for any call that isn't `force: true` — set from
     * the spec (via `page.evaluate`) before opening the apply dialog to
     * drive the "game is running" path.
     */
    __APPLY_FORCE_GAME_RUNNING__?: boolean;
    /** Every `verify_order` payload the mock received, in order — for the apply-dialog spec's own "Verify order" coverage. */
    __VERIFY_ORDER_CALLS__?: VerifyRequestDto[];
    /**
     * When set, the next `verify_order` call returns this report
     * verbatim instead of the default two-operation fixture — set from
     * the spec (via `page.evaluate`) to drive a scenario the shared
     * default (pinned by several other specs' own count assertions)
     * can't express, such as a cosmetic verify row.
     */
    __VERIFY_ORDER_OVERRIDE__?: VerifyReportDto | null;
    /** Every `resolve_def_graphic` request the mock received, in order. */
    __RESOLVE_DEF_GRAPHIC_CALLS__?: ResolveDefGraphicRequestDto[];
    /** Every `read_def_texture` request the mock received, in order. */
    __READ_DEF_TEXTURE_CALLS__?: ReadDefTextureRequestDto[];
    /** Every `upsert_rule` payload the mock received, in order. */
    __UPSERT_RULE_CALLS__?: RuleDto[];
    /** Every `save_app_config` payload the mock received, in order. */
    __SAVE_APP_CONFIG_CALLS__?: AppConfigDto[];
    /** Every `delete_rule` payload the mock received, in order. */
    __DELETE_RULE_CALLS__?: DeleteRuleRequestDto[];
    /** Every `promote_imported_rule` key the mock received, in order. */
    __PROMOTE_RULE_CALLS__?: RuleKeyDto[];
    /** Every `set_manual_tag` payload the mock received, in order. */
    __SET_MANUAL_TAG_CALLS__?: SetManualTagRequestDto[];
    /** Every `set_settings` payload the mock received, in order. */
    __SET_SETTINGS_CALLS__?: SettingsDto[];
    /** Every `import_rimsort` payload the mock received, in order. */
    __IMPORT_RIMSORT_CALLS__?: RimSortPathsDto[];
    /** Every `set_merge_choices` payload the mock received, in order. */
    __SET_MERGE_CHOICES_CALLS__?: SetMergeChoicesRequestDto[];
    /** Every `create_patch` payload the mock received, in order — for the patches spec. */
    __CREATE_PATCH_CALLS__?: CreatePatchRequestDto[];
    /** Every `update_patch` payload the mock received, in order — for the patches spec's scope-editing assertions. */
    __UPDATE_PATCH_CALLS__?: UpdatePatchRequestDto[];
    /** Every `decide_patch` payload the mock received, in order — for the patches spec. */
    __DECIDE_PATCH_CALLS__?: DecidePatchRequestDto[];
    /** Every `export_patch` payload the mock received, in order — for the patches spec. */
    __EXPORT_PATCH_CALLS__?: ExportPatchRequestDto[];
    /** Every `create_assignment` payload the mock received, in order — for the assignments spec. */
    __CREATE_ASSIGNMENT_CALLS__?: CreateAssignmentRequestDto[];
    /** Every `update_assignment` payload the mock received, in order — for the assignments spec. */
    __UPDATE_ASSIGNMENT_CALLS__?: UpdateAssignmentRequestDto[];
    /** Every `set_assignment_row` payload the mock received, in order — for the assignments spec. */
    __SET_ASSIGNMENT_ROW_CALLS__?: SetAssignmentRowRequestDto[];
    /** Every `export_assignment` payload the mock received, in order — for the assignments spec. */
    __EXPORT_ASSIGNMENT_CALLS__?: ExportAssignmentRequestDto[];
    /** Every `add_assignment_section` payload the mock received, in order — for the assignments spec. */
    __ADD_ASSIGNMENT_SECTION_CALLS__?: AddAssignmentSectionRequestDto[];
    /** Every `remove_assignment_section` payload the mock received, in order — for the assignments spec. */
    __REMOVE_ASSIGNMENT_SECTION_CALLS__?: RemoveAssignmentSectionRequestDto[];
    /** Every `activate_mods` payload the mock received, in order — for the active-set spec. */
    __ACTIVATE_CALLS__?: ActivateRequestDto[];
    /** Every `deactivate_mods` payload the mock received, in order — for the active-set spec. */
    __DEACTIVATE_CALLS__?: DeactivateRequestDto[];
    /** How many `rescan_project` calls the mock received — for the active-set spec. */
    __RESCAN_CALLS__?: number;
    /** Every `open_mod_link` payload the mock received, in order — for the mod info panel spec. */
    __OPEN_MOD_LINK_CALLS__?: { modId: string; link: ModLinkKindDto }[];
    /** Every `open_app_link` payload the mock received, in order — for the settings/sidebar link specs. */
    __OPEN_APP_LINK_CALLS__?: AppLinkTargetDto[];
    /**
     * When set, the next `export_assignment` call reports one hardcoded
     * skipped field per section that has a row instead of `skipped: []`
     * — set from the spec (via `page.evaluate`) to drive the "export
     * shows skipped" scenario without needing a genuinely vanished item
     * modeled end to end.
     */
    __ASSIGNMENT_EXPORT_FORCE_SKIP__?: boolean;
    /**
     * The folder path `plugin:dialog|open` returns from the mocked
     * native folder picker — `null` (the default) mimics the user
     * cancelling, exactly like `services/dialogs.ts::pickFolder`'s own
     * cancel case. Set from the spec (via `page.evaluate`) before
     * clicking "Choose…".
     */
    __PICK_FOLDER_RESULT__?: string | null;
    /**
     * The file path `plugin:dialog|open` returns from the mocked native
     * file picker (`services/dialogs.ts::pickFile`) —
     * `null` (the default) mimics the user cancelling. Set from the spec
     * before clicking "Import game log". Distinguished from
     * {@link __PICK_FOLDER_RESULT__} by the mocked `open()` call's own
     * `options.directory` flag, since both share one Tauri command.
     */
    __PICK_FILE_RESULT__?: string | null;
    /**
     * The `GameLogSummaryDto` the next `import_game_log` call returns —
     * `IMPORT_GAME_LOG_DEFAULT` (below) unless a spec overrides it via
     * `page.evaluate` to drive a specific predicted-vs-observed or
     * different-order scenario.
     */
    __IMPORT_GAME_LOG_RESULT__?: GameLogSummaryDto;
    /**
     * Fields laid over `IMPORT_GAME_LOG_DEFAULT` for the next
     * `import_game_log` call (ignored when {@link __IMPORT_GAME_LOG_RESULT__}
     * is set) — the short way to drive a coverage case (a console snapshot,
     * logging gaps, read losses) without spelling out a whole summary.
     */
    __IMPORT_GAME_LOG_OVERRIDES__?: Partial<GameLogSummaryDto>;
    /**
     * The active mod `get_def_cache_carrier` reports as a def-cache carrier
     * — `null` (no carrier active, the default) unless a spec sets one
     * via `page.evaluate` to drive the apply dialog's own def-cache note.
     */
    __DEF_CACHE_CARRIER_MOD_ID__?: string | null;
    /**
     * When `true`, the next `refresh_rule_databases` call reports
     * `community` as `Failed` instead of `Updated` — set from the spec
     * (via `page.evaluate`) to drive the "a failing source leaves the
     * cached copy in use" scenario deterministically.
     */
    __RULE_DATABASE_REFRESH_FORCE_FAIL__?: boolean;
    /**
     * Every `refresh_rule_databases` request the mock received, in order:
     * `null` for a call that named no sources (the Databases card's
     * Refresh button), the request otherwise.
     */
    __REFRESH_RULE_DATABASES_CALLS__?: (RefreshRuleDatabasesRequestDto | null)[];
    /**
     * The sources the mock's `ruleDatabasesStale` notice lists, set from a
     * spec (via `page.evaluate`, before the notifications query next
     * refetches). Unset or empty, no such notice exists. Like the real
     * notice it needs Welcome answered and internet access on.
     */
    __STALE_RULE_DATABASES__?: RuleDatabaseViewDto["database"][];
    /** Every `dismiss_notification` payload the mock received, in order. */
    __DISMISS_NOTIFICATION_CALLS__?: NotificationKeyDto[];
    /** Every `mute_notification_kind` payload the mock received, in order. */
    __MUTE_NOTIFICATION_CALLS__?: NotificationKindDto[];
    /** How many `complete_welcome` calls the mock received. */
    __COMPLETE_WELCOME_CALLS__?: number;
    /** How many `enable_recommended_rule_databases` calls the mock received. */
    __ENABLE_RECOMMENDED_CALLS__?: number;
    /** How many `reset_settings` calls the mock received. */
    __RESET_SETTINGS_CALLS__?: number;
    /** How many `check_for_update` calls the mock received. */
    __CHECK_FOR_UPDATE_CALLS__?: number;
    /**
     * The next `check_for_update`/automatic-check-within
     * `run_launch_network_checks` result — `undefined` (an "unchanged"
     * response, the default) unless a spec sets one via `page.evaluate`
     * to drive a specific "up to date"/"new version"/"rate limited"/
     * "failed" Settings-page result line.
     */
    __CHECK_FOR_UPDATE_RESULT__?: CheckForUpdateOutcomeDto;
    /** How many `run_launch_network_checks` calls actually reached the mocked port (i.e. weren't refused by the shared gate) — for the first-run e2e spec. */
    __RUN_LAUNCH_NETWORK_CHECKS_CALLS__?: number;
    /** Every `update_app_settings` payload the mock received, in order. */
    __UPDATE_APP_SETTINGS_CALLS__?: AppSettingsDto[];
  }
}

/**
 * Installs a full, stateful mock-IPC session directly inside the page —
 * ~60 mods, ~120 findings across every kind, both order sources (with a
 * few moved mods between them), a few rules of each origin, and some
 * tags. Every mutating command (`decide`/`revert_decision`/
 * `upsert_rule`/`delete_rule`/`set_manual_tag`/`set_settings`) mutates
 * real state here, so a spec can decide a finding and then observe the
 * list/dashboard reflect it.
 *
 * This function is handed to Playwright's `page.addInitScript` **as a
 * function value, not called from Node** — Playwright serializes it via
 * `Function.prototype.toString()` and evaluates that source directly in
 * the browser, so everything the mock needs (the `Scenario` class, its
 * helpers, `ALL_KINDS`, ...) has to be declared *inside* this function
 * body. A reference to anything in this module's outer scope would be
 * `undefined` in the page — only TypeScript *type* imports are safe
 * above (erased before this function's source is ever captured); a
 * runtime import here would silently break the mock.
 */
export function installScenario(): void {
  const MOD_COUNT = 60;

  function modId(index: number): string {
    return `mod.${String(index).padStart(3, "0")}`;
  }

  function modName(index: number): string {
    return `Mod ${index}`;
  }

  function sourceOf(index: number) {
    if (index === 0) return "core" as const;
    if (index === 1) return "dlc" as const;
    return index % 3 === 0 ? ("local" as const) : ("workshop" as const);
  }

  const ALL_KINDS: FindingKindDto[] = [
    "edgeDropped",
    "anyOfChoice",
    "defOverride",
    "patchCollision",
    "textureOverride",
    "duplicateAssembly",
    "likelyDuplicateMod",
    "missingMod",
    "missingDependency",
    "incompatiblePair",
    "unsupportedVersion",
    "undeclaredHardDependency",
    "lazyReferenceViolated",
    "declarationQuestioned",
    "duplicateTemplateName",
    "keyedTranslationCollision",
    "soundOverride",
    "undeclaredTypeDependency",
    "runtimePatchCollision",
    "tagInferred",
    "ruleOverruled",
    "placementOverruled",
    "placementQuestioned",
  ];

  interface FindingEntry {
    key: string;
    kind: FindingKindDto;
    finding: FindingDto;
    confidence: number;
    status: ResolutionStatusDto;
    effective: ActionDto;
    suggestionAction: ActionDto;
    alternatives: { action: ActionDto; rationale: string; rationaleCode?: RationaleDto }[];
    rationale: string;
    /**
     * A precise structured code for {@link rationale}, for the rare
     * fixture a spec asserts the exact rendered rationale text
     * against (`clean-merge.spec.ts`'s `CleanWall` finding, whose own
     * `rationale`/alternative text mirrors real `ledger::suggest`/
     * `redecide_for_clean_merge` output on purpose — see that
     * fixture's own comment). Every other fixture leaves this unset
     * and gets {@link GENERIC_RATIONALE_CODE} — no spec in this suite
     * asserts a mismatch, since a fixture's own `rationale` text is
     * otherwise never meant to match the real backend's.
     */
    rationaleCode?: RationaleDto;
    hasDecision: boolean;
    note: string | null;
    modIds: string[];
    /**
     * Set once a `Merge` decision exists on a mergeable finding (see
     * `mergeFixtures`/`mergeChoicesByKey` below), or baked in directly for
     * a finding representing an already-auto-promoted clean `Merge` (no
     * decision at all — see the `CleanWall` fixture).
     */
    mergeState: MergeStateDto | null;
  }

  function buildFindingForKind(
    kind: FindingKindDto,
    repeat: number,
    confidence: number,
  ): FindingEntry {
    const a = modId((repeat * 7) % (MOD_COUNT - 1));
    const b = modId((repeat * 7 + 1) % MOD_COUNT);
    const status: ResolutionStatusDto = confidence >= 80 ? "auto" : "needsInput";
    const base = { confidence, status, hasDecision: false, note: null, mergeState: null };

    switch (kind) {
      case "edgeDropped":
        return {
          ...base,
          key: `edge_dropped:${a}:${b}:loadAfter:${repeat}`,
          kind,
          finding: {
            kind: "edgeDropped",
            after: a,
            before: b,
            edgeKind: "loadAfter",
            detail: "loadAfter",
            strength: "declared",
            winner: null,
          },
          effective: { kind: "dropEdge", after: a, before: b, edgeKind: "loadAfter" },
          suggestionAction: { kind: "dropEdge", after: a, before: b, edgeKind: "loadAfter" },
          alternatives: [
            {
              action: { kind: "keepEdge", after: a, before: b, edgeKind: "loadAfter" },
              rationale: "keep the edge and let the sorter break the cycle elsewhere",
            },
          ],
          rationale: "declared edges in a cycle are cut at the least-depended-on side",
          modIds: [a, b],
        };
      case "anyOfChoice":
        return {
          ...base,
          key: `any_of_choice:${a}:Shared${repeat}.dll`,
          kind,
          finding: {
            kind: "anyOfChoice",
            after: a,
            assembly: `Shared${repeat}.dll`,
            candidates: [b, modId((repeat * 3) % MOD_COUNT)],
          },
          effective: { kind: "chooseCandidate", after: a, chosen: b },
          suggestionAction: { kind: "chooseCandidate", after: a, chosen: b },
          alternatives: [
            {
              action: {
                kind: "chooseCandidate",
                after: a,
                chosen: modId((repeat * 3) % MOD_COUNT),
              },
              rationale: "the other candidate providing the same assembly",
            },
          ],
          rationale: "already loads before the requiring mod in the current order",
          modIds: [a, b],
        };
      case "defOverride":
        return {
          ...base,
          key: `def_override:ThingDef/Wall${repeat}:[${a},${b}]`,
          kind,
          finding: {
            kind: "defOverride",
            key: { defType: "ThingDef", defName: `Wall${repeat}` },
            owners: [a, b],
            winner: b,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: {
                kind: "preferWinner",
                key: { defType: "ThingDef", defName: `Wall${repeat}` },
                winner: a,
              },
              rationale: "prefer the other owner instead",
            },
          ],
          rationale: "the winner declares a relation to the other owner",
          modIds: [a, b],
        };
      case "patchCollision":
        return {
          ...base,
          key: `patch_collision:ThingDef/Wall${repeat}:Contested:none:[${a},${b}]`,
          kind,
          finding: {
            kind: "patchCollision",
            key: { defType: "ThingDef", defName: `Wall${repeat}` },
            selector: "defName",
            subPath: "statBases/MaxHitPoints",
            mods: [a, b],
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: {
                kind: "preferWinner",
                key: { defType: "ThingDef", defName: `Wall${repeat}` },
                winner: a,
              },
              rationale: "prefer the first owner's patch instead",
            },
          ],
          rationale: "contested patches apply additively in load order",
          modIds: [a, b],
        };
      case "textureOverride":
        return {
          ...base,
          key: `texture_override:Things/Wall${repeat}:[${a},${b}]`,
          kind,
          // `winner` is recomputed from the selected order on every read (`findingFor`).
          finding: {
            kind: "textureOverride",
            texturePath: `Things/Wall${repeat}`,
            owners: [a, b],
            winner: b,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [],
          rationale: "the last owner in load order wins",
          modIds: [a, b],
        };
      case "duplicateAssembly":
        return {
          ...base,
          key: `duplicate_assembly:Shared${repeat}.dll:[${a},${b}]`,
          kind,
          finding: {
            kind: "duplicateAssembly",
            assemblyName: `Shared${repeat}.dll`,
            owners: [a, b],
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            { action: { kind: "removeMod", modId: b }, rationale: "remove the duplicate" },
          ],
          rationale: "duplicate assemblies are usually harmless but worth a look",
          modIds: [a, b],
        };
      case "likelyDuplicateMod":
        return {
          ...base,
          key: `likely_duplicate_mod:[${a},${b}]`,
          kind,
          finding: { kind: "likelyDuplicateMod", a, b, sharedDefs: 4 + repeat },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            { action: { kind: "removeMod", modId: b }, rationale: "remove the likely duplicate" },
          ],
          rationale: "both mods define many identical defs",
          modIds: [a, b],
        };
      case "missingMod":
        return {
          ...base,
          key: `missing_mod:${a}`,
          kind,
          finding: { kind: "missingMod", modId: a },
          effective: { kind: "removeMod", modId: a },
          suggestionAction: { kind: "removeMod", modId: a },
          alternatives: [{ action: { kind: "ignore" }, rationale: "keep it in ModsConfig anyway" }],
          rationale: "not found on disk",
          modIds: [a],
        };
      case "missingDependency":
        return {
          ...base,
          key: `missing_dependency:${a}:${b}`,
          kind,
          finding: { kind: "missingDependency", modId: a, dependency: b, displayName: null },
          effective: { kind: "ignore" },
          suggestionAction: { kind: "ignore" },
          alternatives: [],
          rationale: "no reliable automatic action",
          modIds: [a, b],
        };
      case "incompatiblePair":
        return {
          ...base,
          key: `incompatible_pair:[${a},${b}]`,
          kind,
          finding: { kind: "incompatiblePair", a, b },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            { action: { kind: "removeMod", modId: b }, rationale: "remove one of the pair" },
          ],
          rationale: "both mods declare incompatibility",
          modIds: [a, b],
        };
      case "unsupportedVersion":
        return {
          ...base,
          key: `unsupported_version:${a}`,
          kind,
          finding: { kind: "unsupportedVersion", modId: a },
          effective: { kind: "ignore" },
          suggestionAction: { kind: "ignore" },
          alternatives: [],
          rationale: "usually still works",
          modIds: [a],
        };
      case "undeclaredHardDependency":
        return {
          ...base,
          key: `undeclared_hard_dependency:${a}:${b}`,
          kind,
          finding: {
            kind: "undeclaredHardDependency",
            after: a,
            before: b,
            detail: "AssemblyRef Foo.dll",
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: { kind: "reorder", after: a, before: b },
              rationale: "declare it explicitly",
            },
          ],
          rationale: "the sorter already honors the hard edge",
          modIds: [a, b],
        };
      case "lazyReferenceViolated":
        return {
          ...base,
          key: `lazy_reference_violated:${a}:${b}`,
          kind,
          finding: {
            kind: "lazyReferenceViolated",
            after: a,
            before: b,
            detail: "AssemblyRef Bar.dll",
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            { action: { kind: "reorder", after: a, before: b }, rationale: "reorder anyway" },
          ],
          rationale: "JIT-resolved; load order doesn't affect it",
          modIds: [a, b],
        };
      case "declarationQuestioned":
        return {
          ...base,
          key: `declaration_questioned:${a}:${b}:findMod`,
          kind,
          finding: {
            kind: "declarationQuestioned",
            declaredAfter: a,
            declaredBefore: b,
            declaredLayer: "declared",
            declaredDetail: `${a} declares loadAfter ${b}`,
            relationKind: "findMod",
            relationDetail: `${a}'s patch checks for ${b} via FindMod`,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: { kind: "reorder", after: b, before: a },
              rationale: "Force the opposite order the relation implies, as a user decision.",
            },
          ],
          rationale: `${a} loads after ${b} per a declared edge; the FindMod check implies the opposite.`,
          modIds: [a, b],
        };
      case "duplicateTemplateName":
        return {
          ...base,
          key: `duplicate_template_name:WallBase${repeat}:[${a},${b}]`,
          kind,
          finding: { kind: "duplicateTemplateName", name: `WallBase${repeat}`, owners: [a, b] },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: {
                kind: "preferWinner",
                key: { defType: "template_name", defName: `WallBase${repeat}` },
                winner: a,
              },
              rationale: "Force one mod's template to win.",
            },
            {
              action: {
                kind: "preferWinner",
                key: { defType: "template_name", defName: `WallBase${repeat}` },
                winner: b,
              },
              rationale: "Force one mod's template to win.",
            },
          ],
          rationale: `The last-loaded registration wins in XmlInheritance: ${b}.`,
          modIds: [a, b],
        };
      case "keyedTranslationCollision":
        return {
          ...base,
          key: `keyed_translation_collision:[${a},${b}]`,
          kind,
          finding: { kind: "keyedTranslationCollision", a, b, keys: ["Greeting", "Farewell"] },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [],
          rationale:
            "These two mods define 2 of the same translation keys; the last-loaded one wins each time.",
          modIds: [a, b],
        };
      case "soundOverride":
        return {
          ...base,
          key: `sound_override:shot_fire${repeat}:[${a},${b}]`,
          kind,
          finding: { kind: "soundOverride", path: `shot_fire${repeat}`, owners: [a, b] },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: {
                kind: "preferWinner",
                key: { defType: "sound", defName: `shot_fire${repeat}` },
                winner: a,
              },
              rationale: "Force one mod's sound to win.",
            },
            {
              action: {
                kind: "preferWinner",
                key: { defType: "sound", defName: `shot_fire${repeat}` },
                winner: b,
              },
              rationale: "Force one mod's sound to win.",
            },
          ],
          rationale: "A sound override is cosmetic and audible in-game; it never blocks.",
          modIds: [a, b],
        };
      case "undeclaredTypeDependency":
        return {
          ...base,
          key: `undeclared_type_dependency:${a}:${b}:Framework.Utils`,
          kind,
          finding: {
            kind: "undeclaredTypeDependency",
            user: a,
            provider: b,
            typeName: "Framework.Utils",
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: { kind: "reorder", after: a, before: b },
              rationale: "Pin the relation explicitly as a load-order rule.",
            },
          ],
          rationale: `${a} uses a type from ${b}'s assembly but declares no dependency on it.`,
          modIds: [a, b],
        };
      case "runtimePatchCollision": {
        // Three owners at `repeat`, `repeat+1`, `repeat+2` mod `MOD_COUNT`
        // — always pairwise distinct (`MOD_COUNT` is far larger than 3),
        // unlike the shared `a`/`b` picker above, which can coincide onto
        // the same mod for some `repeat` values and would render a
        // self-duplicate owner in the runtime-patch card. Confidence is
        // always 85: `ledger::suggest::runtime_patch_collision` hardcodes it
        // (`Action::Accept` is the only outcome; there's no fix that
        // "resolves" a runtime-patch collision the way accepting a
        // def-override winner does), never the passed-in `confidence` — the previous
        // shape threading that through made an identical-looking finding
        // flip between `auto` and `needsInput` depending only on which
        // `repeat` generated it.
        const owners = [
          modId(repeat % MOD_COUNT),
          modId((repeat + 1) % MOD_COUNT),
          modId((repeat + 2) % MOD_COUNT),
        ];
        const lastPatcher = owners[owners.length - 1] as string;
        return {
          confidence: 85,
          status: "auto",
          hasDecision: false,
          note: null,
          mergeState: null,
          // The per-target regroup: keyed by the target itself, not the mod pair —
          // see `FindingKey::RuntimePatchCollision`'s own doc comment.
          key: `runtime_patch_collision:Verse.Pawn:Kill:[${owners.join(",")}]`,
          kind,
          finding: {
            kind: "runtimePatchCollision",
            targetType: "Verse.Pawn",
            targetMethod: "Kill",
            owners,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: owners.map((owner) => ({
            action: {
              kind: "preferWinner",
              key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
              winner: owner,
            },
            rationale: "Force this mod's patch to run last, regardless of load order.",
          })),
          rationale: `${owners.length} mods patch Verse.Pawn.Kill; ${lastPatcher} loads last today, so its patch runs last.`,
          modIds: owners,
        };
      }
      case "transpilerCollision": {
        // Same pairwise-distinct-owner shape as `runtimePatchCollision`
        // above. Confidence is always 55 — `ledger::suggest::
        // transpiler_collision` hardcodes it, and offers no alternatives
        // at all (the right transpiler order isn't derivable), unlike its
        // `runtimePatchCollision` sibling's per-owner `preferWinner` set.
        const owners = [
          modId(repeat % MOD_COUNT),
          modId((repeat + 1) % MOD_COUNT),
          modId((repeat + 2) % MOD_COUNT),
        ];
        return {
          confidence: 55,
          status: "needsInput",
          hasDecision: false,
          note: null,
          mergeState: null,
          key: `transpiler_collision:Verse.Verb_LaunchProjectile:TryCastShot:[${owners.join(",")}]`,
          kind,
          finding: {
            kind: "transpilerCollision",
            targetType: "Verse.Verb_LaunchProjectile",
            targetMethod: "TryCastShot",
            owners,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [],
          rationale: `${owners.length} mods each rewrite Verse.Verb_LaunchProjectile.TryCastShot's IL with a runtime-patch transpiler.`,
          modIds: owners,
        };
      }
      case "ruleOverruled":
        return {
          ...base,
          key: `rule_overruled:${a}:${b}:rim_sort_community`,
          kind,
          finding: {
            kind: "ruleOverruled",
            after: a,
            before: b,
            origin: "rimSortCommunity",
            comment: null,
            winner: { after: b, before: a, layer: "hard", detail: "ships a load-time AssemblyRef" },
            witnessCycle: [],
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          // `rim_resolve::ledger::suggest::rule_overruled`: `Reorder`
          // is gated on the winner's layer (`Hard` excludes it, matching
          // this fixture's winner), but `PromoteRule` is gated
          // independently on the rule's own origin — a bare `hard` winner
          // still offers `Promote` for a non-`userDecision` origin like
          // this one.
          alternatives: [
            {
              action: { kind: "promoteRule", rule: { kind: "pair", after: a, before: b } },
              rationale:
                "Keep this rule as your own decision, so it survives even though it stays overruled.",
            },
          ],
          rationale: `${a} loading after ${b} per a RimSort community rule was overruled: a hard (load-time) requirement says ${b} loads after ${a} instead.`,
          modIds: [a, b],
        };
      case "placementOverruled":
        return {
          ...base,
          key: `placement_overruled:${a}:bottom:rim_sort_community`,
          kind,
          finding: {
            kind: "placementOverruled",
            modId: a,
            placement: "bottom",
            origin: "rimSortCommunity",
            by: { after: b, before: a, layer: "hard", detail: "ships a load-time AssemblyRef" },
            landedAt: 3,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [],
          rationale: `${a} is pinned at the bottom, but a hard (load-time) requirement overrules it.`,
          modIds: [a],
        };
      case "placementQuestioned":
        return {
          ...base,
          key: `placement_questioned:${a}:bottom:may_require`,
          kind,
          finding: {
            kind: "placementQuestioned",
            modId: a,
            placement: "bottom",
            other: b,
            relationKind: "mayRequire",
            relationDetail: `${b} may require ${a}`,
          },
          effective: { kind: "accept" },
          suggestionAction: { kind: "accept" },
          alternatives: [
            {
              action: { kind: "reorder", after: b, before: a },
              rationale: "Enforce this relation explicitly as a user decision.",
            },
          ],
          rationale: `${a} is pinned at the bottom; this only tests whether a mod is present, not its load position.`,
          modIds: [a, b],
        };
      default:
        return {
          ...base,
          key: `tag_inferred:${a}:framework`,
          kind: "tagInferred",
          finding: {
            kind: "tagInferred",
            modId: a,
            tag: "framework",
            matched: [
              { kind: "urlContains", needle: "framework" },
              { kind: "assemblyRefTo", modId: b },
            ],
          },
          effective: { kind: "addTag", modId: a, tag: "framework" },
          suggestionAction: { kind: "addTag", modId: a, tag: "framework" },
          alternatives: [
            {
              action: { kind: "removeTag", modId: a, tag: "framework" },
              rationale: "remove the tag",
            },
          ],
          rationale: "matched def namespace and url signals",
          modIds: [a],
        };
    }
  }

  // --- Merge editor fixtures -------------------------------------------
  //
  // Three real-shaped previews:
  // the `BionicHeart` def override (two owners, every field `oneSided`),
  // a three-owner `HeadTypeDef/HeadNormal`
  // def override with two genuine `Conflict` rows, and the `plantDensity`
  // patch collision (`Agreeing` — already `Complete` with no
  // input needed). These never round-trip through the real Rust
  // `FindingKey`/`FieldPath` parsers (this is a mock replacing the whole
  // backend), so the key/path strings only need to be unique and stable
  // within this fixture, not literally parseable.

  interface MergeFixtureField {
    path: string;
    depth: number;
    isListItem: boolean;
    /**
     * The tag-keyed map this field is a key of, e.g. `"wildAnimals"` for
     * `path: "wildAnimals/Cobra"` — `undefined` for a plain leaf or `li`
     * item. Drives `MergeFieldDto.entry`/`.container` (`buildMergeFieldDto`)
     * and `get_def_conflict_view`'s own `mapEntry` row kind
     * (`defConflictRowKind`), the same way `isListItem` already drives the
     * `listItem`/`listEntry` pair.
     */
    container?: string;
    diffClass: DiffClassDto;
    changedBy: string[];
    confidence: number;
    base: string | null;
    candidates: Record<string, string | null>;
    /**
     * `get_def_conflict_view`'s own "list case" dedup, extended to a keyed
     * map's own key agreement:
     * other mods that independently added the exact same `li` item (or
     * agreed on the exact same map key) as this one — rendered straight
     * into `FieldRowDto.agreedBy`, never a second `MergeFixtureField` of
     * its own. `undefined`/omitted for every field but one of those two
     * shapes.
     */
    agreedBy?: string[];
  }
  interface MergeFixture {
    key: string;
    defKey: { defType: string; defName: string };
    kind: "defOverride" | "patchCollision";
    owners: MergeOwnerDto[];
    base: string;
    winner: string;
    fields: MergeFixtureField[];
    caveats: CaveatDto[];
    /**
     * `get_def_conflict_view`'s own touchers table, when it differs from
     * what `owners`/`base` alone would derive
     * — e.g. a patcher the collision's own `mods` excludes (a stopper
     * upstream of the contested field) but that still shows up as a
     * toucher of the def. `undefined` derives the generic way every other
     * fixture already does: `owners` as-is for `defOverride`, `base` plus
     * one row per `owners` entry for `patchCollision`.
     */
    touchers?: {
      modId: string;
      position: number;
      isGenerated: boolean;
      role: "owner" | "patcher";
      opCount: number;
    }[];
    /**
     * A `Stopper::Replay`-shaped fold stop (the "unsupported op
     * mid-sequence" fixture): every path in
     * `unreachedPaths` gets `inGame: null` in `get_def_conflict_view`'s
     * own response regardless of what its own `MergeFixtureField` would
     * otherwise resolve to, and the view carries exactly one `Problem`
     * naming this stopper. `undefined` for every fixture whose effective
     * def resolves completely (every one but the unsupported-op fixture).
     */
    unsupportedOp?: {
      modId: string;
      opIndex: number;
      class: string;
      xpath: string | null;
      reason: string;
      unreachedPaths: string[];
    };
    /**
     * A stored `PreferWinner` decision's own winner, per contested field
     * path — `get_def_conflict_view`'s own `Preference::Decision` case.
     * `undefined`/no entry for a path falls back to `loadOrder` (or
     * `mergeChoice`, which is checked first).
     */
    decisionWinners?: Record<string, string>;
    /**
     * The structural guard: when
     * set, `computeMergeState` forces `needsFieldInput` with
     * `unresolved === total === fields.length`, regardless of `choices`
     * — mirrors `rim-session`'s own guard, which can never be cleared by
     * a per-field pick. `get_merge_preview`'s own response surfaces it
     * verbatim as `MergePreviewDto.structuralGuard`. `undefined` for
     * every fixture the guard never fires for (every one but the
     * dedicated guarded fixture below).
     */
    structuralGuard?: { field: string; by: string };
  }

  const mergeFixtures = new Map<string, MergeFixture>();
  const mergeChoicesByKey = new Map<string, Record<string, MergeChoiceDto | undefined>>();

  function registerMergeFixture(fixture: MergeFixture): void {
    mergeFixtures.set(fixture.key, fixture);
    mergeChoicesByKey.set(fixture.key, {});
  }

  /** What the plan currently produces for one field, mirroring `dto::merge::field_result`. */
  function mergeFieldResult(
    f: MergeFixtureField,
    choice: MergeChoiceDto | undefined,
  ): string | null {
    if (choice) {
      if (choice.choice === "from") return f.candidates[choice.modId] ?? null;
      if (choice.choice === "value") return choice.text;
      return null; // drop
    }
    if (f.diffClass === "conflict") return null;
    if (f.diffClass === "unchanged") return f.base;
    const by = f.changedBy[0];
    return by !== undefined ? (f.candidates[by] ?? null) : f.base;
  }

  /** Mirrors `dto::merge::EntryKindDto::from(&EntryKind)`. */
  function mergeFieldEntry(f: MergeFixtureField): EntryKindDto {
    if (f.container !== undefined) return "mapEntry";
    return f.isListItem ? "listItem" : "leaf";
  }

  function buildMergeFieldDto(
    fixture: MergeFixture,
    f: MergeFixtureField,
    choice: MergeChoiceDto | undefined,
  ): MergeFieldDto {
    return {
      path: f.path,
      depth: f.depth,
      isListItem: f.isListItem,
      entry: mergeFieldEntry(f),
      container: f.container ?? null,
      class: f.diffClass,
      changedBy: f.changedBy,
      confidence: f.confidence,
      base: f.base,
      candidates: f.candidates,
      result: mergeFieldResult(f, choice),
      choice: choice ?? null,
      preselected: f.diffClass === "conflict" ? fixture.winner : null,
    };
  }

  /**
   * Mirrors `rim_resolve::domain::merge_status`'s Complete/NeedsFieldInput
   * split. The structural guard (`structuralGuard`, when set) forces
   * `needsFieldInput` with `unresolved === total` unconditionally — no
   * field choice ever clears it, mirroring `rim-session`'s own
   * `state_from_plan` (checked before the ordinary conflict-count logic,
   * same as the real one).
   */
  function computeMergeState(
    fixture: MergeFixture,
    choices: Record<string, MergeChoiceDto | undefined>,
  ): MergeStateDto {
    if (fixture.structuralGuard) {
      return {
        kind: "needsFieldInput",
        unresolved: fixture.fields.length,
        total: fixture.fields.length,
      };
    }
    const conflictFields = fixture.fields.filter((f) => f.diffClass === "conflict");
    const unresolved = conflictFields.filter((f) => !choices[f.path]).length;
    if (unresolved > 0) {
      return { kind: "needsFieldInput", unresolved, total: fixture.fields.length };
    }
    const opCount = fixture.fields.filter((f) => f.diffClass !== "unchanged").length;
    return { kind: "complete", opCount };
  }

  function escapeXmlText(text: string): string {
    return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  }

  /**
   * Mirrors `rim_merge::plan::build_resolved_node` closely enough for the
   * mock tier: every resolved (non-null) field's value, assembled under
   * the def type — a leaf for a single-segment path, the already-rendered
   * `<li ...>` text (see `MergeFixtureField.candidates`) nested under its
   * container for a two-segment list-item path. None of this fixture's
   * paths go deeper than that, so unlike the real renderer this doesn't
   * need general nesting. `null` outside `defOverride`/`complete`, per
   * `MergePreviewDto::resolved_xml`'s own contract.
   */
  function buildResolvedXml(
    fixture: MergeFixture,
    choices: Record<string, MergeChoiceDto | undefined>,
  ): string | null {
    if (fixture.kind !== "defOverride") return null;
    if (computeMergeState(fixture, choices).kind !== "complete") return null;

    const topLevel: string[] = [];
    const containers = new Map<string, string[]>();
    for (const f of fixture.fields) {
      const result = mergeFieldResult(f, choices[f.path]);
      if (result === null) continue;
      const segments = f.path.split("/");
      const tag = segments[0];
      if (tag === undefined) continue;
      if (segments.length === 1) {
        topLevel.push(`  <${tag}>${escapeXmlText(result)}</${tag}>`);
        continue;
      }
      const items = containers.get(tag) ?? [];
      items.push(`    ${result}`);
      containers.set(tag, items);
    }
    const containerBlocks = Array.from(containers.entries(), ([tag, items]) =>
      [`  <${tag}>`, ...items, `  </${tag}>`].join("\n"),
    );
    return [
      `<${fixture.defKey.defType}>`,
      ...topLevel,
      ...containerBlocks,
      `</${fixture.defKey.defType}>`,
    ].join("\n");
  }

  const bionicHeartUnchanged: MergeFixtureField[] = Array.from({ length: 40 }, (_, i) => ({
    path: `unchangedField${i}`,
    depth: 0,
    isListItem: false,
    diffClass: "unchanged",
    changedBy: [],
    confidence: 0,
    base: "same",
    candidates: { "ludeon.rimworld": "same", "example.bionicsfork": "same" },
  }));
  registerMergeFixture({
    key: "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]",
    defKey: { defType: "HediffDef", defName: "BionicHeart" },
    kind: "defOverride",
    owners: [
      { modId: "ludeon.rimworld", name: "RimWorld", position: 0 },
      { modId: "example.bionicsfork", name: "Example Bionics Fork", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "example.bionicsfork",
    caveats: [],
    fields: [
      // `defName` is a literal XML child element like any other field —
      // both owners set it identically, so it's `unchanged`, but (like
      // the real `rim_merge::plan::build_resolved_node`, which never
      // special-cases or skips an unchanged field) it still lands in the
      // resolved XML via `buildResolvedXml`'s own `unchanged -> f.base`
      // rule below. `e2e/specs/summary.spec.ts`'s resolved-XML toggle
      // assertion relies on this to see the def's own name in the
      // preview.
      {
        path: "defName",
        depth: 0,
        isListItem: false,
        diffClass: "unchanged",
        changedBy: [],
        confidence: 100,
        base: "BionicHeart",
        candidates: { "ludeon.rimworld": "BionicHeart", "example.bionicsfork": "BionicHeart" },
      },
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["example.bionicsfork"],
        confidence: 95,
        base: "bionic heart",
        candidates: { "ludeon.rimworld": "bionic heart", "example.bionicsfork": "synthetic heart" },
      },
      {
        path: "labelNoun",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["example.bionicsfork"],
        confidence: 95,
        base: "a bionic heart",
        candidates: {
          "ludeon.rimworld": "a bionic heart",
          "example.bionicsfork": "a synthetic heart",
        },
      },
      {
        path: "description",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["example.bionicsfork"],
        confidence: 95,
        base: "An installed bionic heart.",
        candidates: {
          "ludeon.rimworld": "An installed bionic heart.",
          "example.bionicsfork": "An installed synthetic heart.",
        },
      },
      {
        path: "defaultLabelColor",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["example.bionicsfork"],
        confidence: 95,
        base: "(0.6, 0.6, 1.0)",
        candidates: { "ludeon.rimworld": "(0.6, 0.6, 1.0)", "example.bionicsfork": "(188,39,242)" },
      },
      {
        path: "comps/li[@Class=ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust]",
        depth: 1,
        isListItem: true,
        diffClass: "oneSided",
        changedBy: ["example.bionicsfork"],
        confidence: 95,
        base: null,
        candidates: {
          "ludeon.rimworld": null,
          "example.bionicsfork":
            '<li Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust"><scaleAdjustment>0.20</scaleAdjustment></li>',
        },
      },
      ...bionicHeartUnchanged,
    ],
  });

  registerMergeFixture({
    key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
    defKey: { defType: "HeadTypeDef", defName: "HeadNormal" },
    kind: "defOverride",
    owners: [
      { modId: "ludeon.rimworld", name: "RimWorld", position: 0 },
      { modId: "example.bionicsfork", name: "Example Bionics Fork", position: 1 },
      { modId: "exampleanim.mod", name: "Example Animation", position: 2 },
    ],
    base: "ludeon.rimworld",
    winner: "exampleanim.mod",
    caveats: [],
    fields: [
      {
        path: "skinColorOverride",
        depth: 0,
        isListItem: false,
        diffClass: "conflict",
        changedBy: ["exampleanim.mod", "example.bionicsfork"],
        confidence: 0,
        base: null,
        candidates: {
          "ludeon.rimworld": null,
          "example.bionicsfork": "(1,0,0)",
          "exampleanim.mod": "(0,1,0)",
        },
      },
      {
        path: "eyeSize",
        depth: 0,
        isListItem: false,
        diffClass: "conflict",
        changedBy: ["exampleanim.mod", "example.bionicsfork"],
        confidence: 0,
        base: "1.0",
        candidates: {
          "ludeon.rimworld": "1.0",
          "example.bionicsfork": "1.2",
          "exampleanim.mod": "1.4",
        },
      },
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["exampleanim.mod"],
        confidence: 95,
        base: "human head",
        candidates: {
          "ludeon.rimworld": "human head",
          "example.bionicsfork": "human head",
          "exampleanim.mod": "expressive human head",
        },
      },
      {
        path: "hitInfo",
        depth: 0,
        isListItem: false,
        diffClass: "unchanged",
        changedBy: [],
        confidence: 0,
        base: "default",
        candidates: {
          "ludeon.rimworld": "default",
          "example.bionicsfork": "default",
          "exampleanim.mod": "default",
        },
      },
    ],
  });

  registerMergeFixture({
    key: "patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[examplebiomes.biomes,example.flora.core]",
    defKey: { defType: "BiomeDef", defName: "TemperateForest" },
    kind: "patchCollision",
    owners: [
      { modId: "example.flora.core", name: "Flora: Core", position: 0 },
      { modId: "examplebiomes.biomes", name: "Example Biomes", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "examplebiomes.biomes",
    caveats: [
      {
        kind: "modSettingDefault",
        modId: "example.flora.core",
        class: "ExampleSettingsFramework.PatchOperationModOption",
      },
      {
        kind: "modSettingDefault",
        modId: "examplebiomes.biomes",
        class: "ExampleVEF.PatchOperationToggableSequence",
      },
    ],
    fields: [
      {
        path: "plantDensity",
        depth: 0,
        isListItem: false,
        diffClass: "agreeing",
        changedBy: ["examplebiomes.biomes", "example.flora.core"],
        confidence: 95,
        base: "0.65",
        candidates: { "example.flora.core": "0.9", "examplebiomes.biomes": "0.9" },
      },
    ],
  });

  // A keyed-container case:
  // `BiomeDef/AridShrubland`'s `wildAnimals` is a tag-keyed
  // `Dictionary<PawnKindDef, float>`. Three mods each `PatchOperationAdd`
  // their own animals to it. `crates/rim-merge`'s own `collision_fields`
  // expands the container into one field per key rather than comparing it
  // as one `Value::Item` blob (a multi-KB one-line XML string rendered as a
  // single truncated cell), since most of the entries union with no real
  // disagreement — this fixture's own 10 keys mirror that engine's shape: 4
  // `Unchanged` (nobody touches them), 3 `OneSided` disjoint adds (one
  // animal each, auto-resolve with nothing to decide), one `Agreeing` key
  // (`Cobra`, `a.mod` and `b.mod` independently add the identical density —
  // the "list case" dedup extended to a map's own key agreement, `agreedBy`
  // names the later contributor), and two real `Conflict`s (`Raptor`,
  // `Boar` — all three mods disagree on each) — the pair a person
  // actually has to decide among the other eight that already resolved
  // on their own, and enough to exercise "accept the suggested winner for
  // every contested key in this container in one action" rather than one
  // click each.
  registerMergeFixture({
    key: "patch_collision:BiomeDef/AridShrubland:defName:wildAnimals:[wildlife.mod.a,wildlife.mod.b,wildlife.mod.c]",
    defKey: { defType: "BiomeDef", defName: "AridShrubland" },
    kind: "patchCollision",
    owners: [
      { modId: "wildlife.mod.a", name: "Wildlife Mod A", position: 0 },
      { modId: "wildlife.mod.b", name: "Wildlife Mod B", position: 1 },
      { modId: "wildlife.mod.c", name: "Wildlife Mod C", position: 2 },
    ],
    base: "ludeon.rimworld",
    winner: "wildlife.mod.c",
    caveats: [],
    fields: [
      ...["Wolf", "Squirrel", "Fox", "Deer"].map(
        (animal): MergeFixtureField => ({
          path: `wildAnimals/${animal}`,
          depth: 1,
          isListItem: false,
          container: "wildAnimals",
          diffClass: "unchanged",
          changedBy: [],
          confidence: 0,
          base: "0.4",
          candidates: {
            "wildlife.mod.a": "0.4",
            "wildlife.mod.b": "0.4",
            "wildlife.mod.c": "0.4",
          },
        }),
      ),
      {
        path: "wildAnimals/Allosaurus",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "oneSided",
        changedBy: ["wildlife.mod.a"],
        confidence: 95,
        base: null,
        candidates: { "wildlife.mod.a": "0.6" },
      },
      {
        path: "wildAnimals/Mammoth",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "oneSided",
        changedBy: ["wildlife.mod.b"],
        confidence: 95,
        base: null,
        candidates: { "wildlife.mod.b": "0.1" },
      },
      {
        path: "wildAnimals/Hyena",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "oneSided",
        changedBy: ["wildlife.mod.c"],
        confidence: 95,
        base: null,
        candidates: { "wildlife.mod.c": "0.2" },
      },
      {
        path: "wildAnimals/Cobra",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "agreeing",
        changedBy: ["wildlife.mod.a"],
        agreedBy: ["wildlife.mod.b"],
        confidence: 95,
        base: null,
        candidates: { "wildlife.mod.a": "0.2" },
      },
      {
        path: "wildAnimals/Raptor",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "conflict",
        changedBy: ["wildlife.mod.a", "wildlife.mod.b", "wildlife.mod.c"],
        confidence: 0,
        base: null,
        candidates: {
          "wildlife.mod.a": "0.3",
          "wildlife.mod.b": "0.3",
          "wildlife.mod.c": "0.5",
        },
      },
      {
        path: "wildAnimals/Boar",
        depth: 1,
        isListItem: false,
        container: "wildAnimals",
        diffClass: "conflict",
        changedBy: ["wildlife.mod.a", "wildlife.mod.b", "wildlife.mod.c"],
        confidence: 0,
        base: null,
        candidates: {
          "wildlife.mod.a": "0.4",
          "wildlife.mod.b": "0.6",
          "wildlife.mod.c": "0.6",
        },
      },
    ],
  });

  // A fifth hand-picked def override, backing the clean-merge Playwright
  // coverage:
  // a clean, two-owner override whose one changed field is a real,
  // non-empty op (`opCount: 1`, unlike `BionicHeart`'s own shape, whose
  // clean merge is a genuine no-op).
  //
  // `redecide_for_clean_merge`'s `Complete { op_count > 0 }` arm only ever
  // *leads with* `Merge` in the alternatives for a `defOverride` (never
  // sets it as the action), because an undecided field-merged def override
  // is a copy no author shipped, unlike a `patchCollision` merge, which
  // only mirrors patches every active mod already runs. The mock mirrors
  // that: `findings.set` below bakes `effective`/`suggestionAction:
  // accept`, `Merge` only as the leading alternative, and `mergeState:
  // null` (lazy, like `BionicHeart`'s own entry — no preview exists until
  // the merge editor actually opens one). The "auto Merge card" coverage
  // in `clean-merge.spec.ts` lives on the `TemperateForest`
  // `patchCollision` fixture instead, the one kind that still promotes to
  // `Merge` 85.
  registerMergeFixture({
    key: "def_override:ThingDef/CleanWall:[wall.core.mod,wall.addon.mod]",
    defKey: { defType: "ThingDef", defName: "CleanWall" },
    kind: "defOverride",
    owners: [
      { modId: "wall.core.mod", name: "Wall Core", position: 0 },
      { modId: "wall.addon.mod", name: "Wall Addon", position: 1 },
    ],
    base: "wall.core.mod",
    winner: "wall.addon.mod",
    caveats: [],
    fields: [
      {
        path: "description",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["wall.addon.mod"],
        confidence: 95,
        base: "a plain wall",
        candidates: { "wall.core.mod": "a plain wall", "wall.addon.mod": "a fancy wall" },
      },
    ],
  });

  // The structural guard: a
  // two-owner override, both fields `OneSided`, zero `Conflict` rows — the
  // guard's worked-example shape (`HediffDef/BionicHeart`'s real
  // one), deliberately a *different* def than `BionicHeart` itself: that
  // fixture backs several other specs
  // (`clean-merge.spec.ts`/`merge.spec.ts`) as a plain, ungated
  // "decide it empty via `m` and it resolves to Complete" case, and giving
  // it a guard here would silently break every one of those. `computeMergeState`
  // forces `needsFieldInput` for this fixture regardless of `choices` (see
  // its own doc comment) — the guard is per-def, not per-field, and no
  // field choice can ever clear it.
  registerMergeFixture({
    key: "def_override:ThingDef/GuardedGadget:[core.gadget.mod,guard.gadget.mod]",
    defKey: { defType: "ThingDef", defName: "GuardedGadget" },
    kind: "defOverride",
    owners: [
      { modId: "core.gadget.mod", name: "Gadget Core", position: 0 },
      { modId: "guard.gadget.mod", name: "Gadget Guard", position: 1 },
    ],
    base: "core.gadget.mod",
    winner: "guard.gadget.mod",
    caveats: [],
    structuralGuard: { field: "thingClass", by: "guard.gadget.mod" },
    fields: [
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["guard.gadget.mod"],
        confidence: 95,
        base: "a plain gadget",
        candidates: { "core.gadget.mod": "a plain gadget", "guard.gadget.mod": "a guarded gadget" },
      },
      {
        path: "description",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["guard.gadget.mod"],
        confidence: 95,
        base: "does gadget things",
        candidates: {
          "core.gadget.mod": "does gadget things",
          "guard.gadget.mod": "does guarded gadget things",
        },
      },
    ],
  });

  // A def whose own `defName` carries a space — a real RimWorld shape
  // (`RaidStrategyDef Name="Tribal Siege"`, the exact example `DefRef`'s
  // own doc comment uses), not modeled by any fixture above. Backs
  // `e2e/specs/defs.spec.ts`'s search-box round trip: the space must
  // survive `search_defs` -> the result link's route param -> the def
  // page's own `defRef` display unmangled (`vue-router`'s own param
  // encoder percent-encodes it as `%20` and decodes it back).
  registerMergeFixture({
    key: "def_override:RaidStrategyDef/Tribal Siege:[raidcore.mod,raidexpanded.mod]",
    defKey: { defType: "RaidStrategyDef", defName: "Tribal Siege" },
    kind: "defOverride",
    owners: [
      { modId: "raidcore.mod", name: "Raid Core", position: 0 },
      { modId: "raidexpanded.mod", name: "Raid Expanded", position: 1 },
    ],
    base: "raidcore.mod",
    winner: "raidexpanded.mod",
    caveats: [],
    fields: [
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["raidexpanded.mod"],
        confidence: 95,
        base: "tribal siege",
        candidates: {
          "raidcore.mod": "tribal siege",
          "raidexpanded.mod": "tribal siege (expanded)",
        },
      },
    ],
  });

  // Two more hand-picked `patchCollision` fixtures, backing
  // `get_def_conflict_view`'s own mock beyond what deriving generically
  // from the fixtures above already covers — see that command's own
  // handler further down.

  // A list merged from two mods, each with its own per-entry source: both
  // `widget.mod.a` and `widget.mod.b` patch `label` to different values
  // (the collision) *and* each add a distinct `li` item to `comps` — the
  // mock-tier twin of `rim_session::test_support::two_mod_list_fixture`.
  registerMergeFixture({
    key: "patch_collision:ThingDef/Widget:def_name:label:[widget.mod.a,widget.mod.b]",
    defKey: { defType: "ThingDef", defName: "Widget" },
    kind: "patchCollision",
    owners: [
      { modId: "widget.mod.a", name: "Widget Mod A", position: 0 },
      { modId: "widget.mod.b", name: "Widget Mod B", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "widget.mod.b",
    caveats: [],
    touchers: [
      { modId: "ludeon.rimworld", position: 0, isGenerated: false, role: "owner", opCount: 0 },
      { modId: "widget.mod.a", position: 1, isGenerated: false, role: "patcher", opCount: 2 },
      { modId: "widget.mod.b", position: 2, isGenerated: false, role: "patcher", opCount: 2 },
    ],
    fields: [
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "conflict",
        changedBy: ["widget.mod.a", "widget.mod.b"],
        confidence: 0,
        base: "widget",
        candidates: { "widget.mod.a": "alpha widget", "widget.mod.b": "beta widget" },
      },
      {
        path: "comps/li[@Class=CompProperties_Alpha]",
        depth: 1,
        isListItem: true,
        diffClass: "oneSided",
        changedBy: ["widget.mod.a"],
        confidence: 95,
        base: null,
        candidates: {
          "widget.mod.a": '<li Class="CompProperties_Alpha"/>',
        },
      },
      {
        path: "comps/li[@Class=CompProperties_Beta]",
        depth: 1,
        isListItem: true,
        diffClass: "oneSided",
        changedBy: ["widget.mod.b"],
        confidence: 95,
        base: null,
        candidates: {
          "widget.mod.b": '<li Class="CompProperties_Beta"/>',
        },
      },
    ],
  });

  // The "list case" dedup: `dup.mod.a` and
  // `dup.mod.b` each add the *exact same* `li` item under a colliding
  // identity — the mock-tier twin of
  // `rim_session::test_support::same_identity_agreeing_list_item_fixture`.
  // Unlike the differing-content `Widget`/`same_identity_list_item_fixture`
  // pair, the mock never needs to model the container-level demotion
  // itself (this tier lists the desired final rows directly, not a diff to
  // re-derive them from) — `agreedBy` alone is the whole fixture.
  registerMergeFixture({
    key: "patch_collision:ThingDef/Sprocket:def_name:comps:[dup.mod.a,dup.mod.b]",
    defKey: { defType: "ThingDef", defName: "Sprocket" },
    kind: "patchCollision",
    owners: [
      { modId: "dup.mod.a", name: "Dup Mod A", position: 0 },
      { modId: "dup.mod.b", name: "Dup Mod B", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "dup.mod.b",
    caveats: [],
    touchers: [
      { modId: "ludeon.rimworld", position: 0, isGenerated: false, role: "owner", opCount: 0 },
      { modId: "dup.mod.a", position: 1, isGenerated: false, role: "patcher", opCount: 1 },
      { modId: "dup.mod.b", position: 2, isGenerated: false, role: "patcher", opCount: 1 },
    ],
    fields: [
      {
        path: "comps/li[#0]",
        depth: 1,
        isListItem: true,
        diffClass: "oneSided",
        changedBy: ["dup.mod.a"],
        agreedBy: ["dup.mod.b"],
        confidence: 95,
        base: null,
        candidates: {
          "dup.mod.a": '<li Class="CompProperties_Shared"/>',
        },
      },
    ],
  });

  // An unknown custom patch class mid-sequence stops the fold: `wall.c.mod`
  // patches `description` (before the stopper, survives), `wall.x.mod`
  // ships the unsupported op (never one of this collision's own `mods` —
  // a third mod's patch stops the *def's* fold without being a party to
  // the collision itself), and `wall.d.mod`'s own `label` patch — the
  // collision's contested field — never runs. The mock-tier twin of
  // `rim_session::test_support::unsupported_op_patch_collision_fixture`.
  registerMergeFixture({
    key: "patch_collision:ThingDef/Wall:def_name:label:[wall.c.mod,wall.d.mod]",
    defKey: { defType: "ThingDef", defName: "Wall" },
    kind: "patchCollision",
    owners: [
      { modId: "wall.c.mod", name: "Wall C Mod", position: 0 },
      { modId: "wall.d.mod", name: "Wall D Mod", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "wall.d.mod",
    caveats: [],
    touchers: [
      { modId: "ludeon.rimworld", position: 0, isGenerated: false, role: "owner", opCount: 0 },
      { modId: "wall.c.mod", position: 1, isGenerated: false, role: "patcher", opCount: 1 },
      { modId: "wall.x.mod", position: 2, isGenerated: false, role: "patcher", opCount: 1 },
      { modId: "wall.d.mod", position: 3, isGenerated: false, role: "patcher", opCount: 1 },
    ],
    unsupportedOp: {
      modId: "wall.x.mod",
      opIndex: 1,
      class: "SomeThirdParty.WeirdOperation",
      xpath: 'Defs/ThingDef[defName="Wall"]/fillPercent',
      reason: "unsupported operation class SomeThirdParty.WeirdOperation",
      unreachedPaths: ["label"],
    },
    fields: [
      {
        path: "description",
        depth: 0,
        isListItem: false,
        diffClass: "oneSided",
        changedBy: ["wall.c.mod"],
        confidence: 95,
        base: "a plain wall",
        candidates: { "wall.c.mod": "a reinforced wall" },
      },
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "conflict",
        changedBy: [],
        confidence: 0,
        base: null,
        candidates: {},
      },
    ],
  });

  // A `PreferWinner` decision overriding a genuine conflict's own
  // preference (a `preference.kind === "decision"` row, which neither
  // fixture above exercises). `winner`
  // (`gadget2.mod.a`) is deliberately the *loser* load order would pick
  // (`gadget2.mod.b` loads last), so the row's own preference visibly
  // differs from what `loadOrder` alone would show.
  registerMergeFixture({
    key: "patch_collision:ThingDef/Gadget2:def_name:label:[gadget2.mod.a,gadget2.mod.b]",
    defKey: { defType: "ThingDef", defName: "Gadget2" },
    kind: "patchCollision",
    owners: [
      { modId: "gadget2.mod.a", name: "Gadget2 Mod A", position: 0 },
      { modId: "gadget2.mod.b", name: "Gadget2 Mod B", position: 1 },
    ],
    base: "ludeon.rimworld",
    winner: "gadget2.mod.b",
    caveats: [],
    decisionWinners: { label: "gadget2.mod.a" },
    fields: [
      {
        path: "label",
        depth: 0,
        isListItem: false,
        diffClass: "conflict",
        changedBy: ["gadget2.mod.a", "gadget2.mod.b"],
        confidence: 0,
        base: "gadget2",
        candidates: { "gadget2.mod.a": "alpha gadget2", "gadget2.mod.b": "beta gadget2" },
      },
    ],
  });

  // `generated: null` on every one (this pool never seeds a generated
  // mod) keeps the shape honest against `ModSummaryDto` — `ScopeEditor`
  // filters candidates on this field.
  const mods = Array.from({ length: MOD_COUNT }, (_, index) => ({
    modId: modId(index),
    name: modName(index),
    source: sourceOf(index),
    tags: [] as string[],
    hardDependents: index % 5,
    generated: null as null,
  }));

  /**
   * The Mods page's own Inactive-tab pool — ~10 discovered-but-inactive
   * mods, neutral ids
   * (`mod.9xx`), never part of the generic `mods`/`ASSIGNMENT_FIXTURE_MODS`
   * pools. `mod.905` declares `mod.906` as a `modDependencies` entry —
   * the one declared dependency chain `activateMods`'s own
   * "also activate dependencies" checkbox exercises.
   */
  const inactiveMods: {
    modId: string;
    name: string;
    source: "local" | "workshop";
    dependsOn: string[];
  }[] = Array.from({ length: 10 }, (_, index) => ({
    modId: `mod.9${(index + 1).toString().padStart(2, "0")}`,
    name: `Inactive Mod ${index + 1}`,
    source: index % 2 === 0 ? "local" : ("workshop" as const),
    dependsOn: index === 4 ? ["mod.906"] : [],
  }));

  /**
   * The patch maker's own fixture universe — mirrors `crates/rim-io/tests/fixtures/assign_game`:
   * a framework shipping five
   * `example.PartAssignmentDef` instances (a `speciesNames` target key, a `parts`
   * item slot, an `enabled` scalar), a parts mod shipping the five
   * `example.PartDef` items they reference, and a target mod shipping five
   * `ThingDef`s with a `<race>` marker — never part of the generic
   * 60-mod pool or `mergeFixtures`, so `list_mods`/`allModNames` gain a
   * small, dedicated extra-mods source for it (see both functions below).
   */
  const ASSIGNMENT_FIXTURE_MODS = [
    { modId: "fixture.framework", name: "Fixture Framework" },
    { modId: "fixture.parts", name: "Fixture Parts" },
    { modId: "fixture.target", name: "Fixture Target" },
  ] as const;
  const ASSIGNMENT_RACE_COUNT = 5;
  const assignmentRaceName = (index: number): string => `Race${index}`;
  const assignmentPartName = (index: number): string => `Part${index}`;
  const assignmentPartHint = (index: number): string =>
    ["EffectAlpha", "EffectBeta", "EffectGamma", "EffectDelta", "EffectEpsilon"][index] ??
    `Effect${index}`;
  const assignmentGroupDefName = (index: number): string => `Group_R${index}`;

  const currentOrder = mods.map((mod) => mod.modId);
  const suggestedOrder = [...currentOrder];
  for (const [x, y] of [
    [10, 15],
    [20, 22],
    [40, 45],
  ] as const) {
    const a = suggestedOrder[x];
    const b = suggestedOrder[y];
    if (a !== undefined && b !== undefined) {
      suggestedOrder[x] = b;
      suggestedOrder[y] = a;
    }
  }

  // The
  // `/startup` page's per-mod cost table, one row per `mods` entry —
  // `textureBytes` scales with `index` so the default (descending)
  // sort and a header-click re-sort produce genuinely different, testable
  // row orders.
  const startupCosts = mods.map((mod, index) => ({
    modId: mod.modId,
    patchOps: index % 4,
    slowXpathOps: index % 2,
    textureFiles: (index + 1) * 3,
    textureBytes: (index + 1) * 1_000_000,
    ddsFiles: index % 3,
    assemblyCount: index % 2,
    assemblyBytes: (index % 2) * 250_000,
    defCount: (index + 1) * 2,
    contentOnly: index % 2 === 0,
    overriddenTextureBytes: index === 0 ? 4096 : 0,
  }));

  // The `import_game_log` fixture: one patch failure, one DDS
  // failure, and one timer, all attributed to `mods[0]` — enough for the
  // `/startup` page's log-derived columns and the apply dialog's def-cache
  // note to have something real to join against. `loadEvents`
  // (its last event) matches `currentOrder` by default so the different-order warning
  // stays hidden unless a spec deliberately picks a mismatched order.
  const firstMod = mods[0];
  if (!firstMod) {
    throw new Error("installScenario: MOD_COUNT must be at least 1");
  }
  const IMPORT_GAME_LOG_DEFAULT: GameLogSummaryDto = {
    patchFailures: [
      {
        attribution: { kind: "mod", modId: firstMod.modId },
        operation: 'Verse.PatchOperationAdd(Defs/ThingDef[defName="Wall"])',
        sourceFile: `C:/RimWorld/Mods/${firstMod.modId}/Patches/wall.xml`,
        stackTrace: null,
      },
    ],
    extraStackTraces: [],
    crossReferences: [],
    ddsFailures: [
      {
        attribution: { kind: "mod", modId: firstMod.modId },
        path: `C:/RimWorld/Mods/${firstMod.modId}/Textures/bad.dds`,
        width: 130,
        height: 130,
        format: "BC7",
      },
    ],
    dependencyWarnings: [],
    timers: [
      {
        attribution: { kind: "mod", modId: firstMod.modId },
        label: "load",
        milliseconds: 250,
        pass: 2,
      },
    ],
    defCacheLines: [
      "DEFCACHE: <color=white>Cache created!</color> creating cache took <color=green>8 seconds</color>",
    ],
    loadEvents: [{ line: 1, kind: { type: "newGame" }, mods: currentOrder }],
    readStats: {
      linesRead: 0,
      linesTruncated: 0,
      linesWithInvalidUtf8: 0,
      stackBlockLinesDropped: 0,
      stackJoinsAbandoned: 0,
      passesFolded: 0,
      stackRefsDropped: 0,
      loggingGapsDropped: 0,
      crashReportPathsTruncated: 0,
    },
    coverage: {
      kind: "playerLog",
      passes: 2,
      patchPhase: "noneLogged",
      endState: { kind: "cleanExit" },
    },
    loggingGaps: [],
    lowerBound: null,
  };

  const findings = new Map<string, FindingEntry>();
  const handPicked: [FindingKindDto, number, number][] = [
    ["missingMod", 0, 10],
    ["edgeDropped", 1, 20],
    ["anyOfChoice", 2, 30],
  ];
  for (const [kind, repeat, confidence] of handPicked) {
    const entry = buildFindingForKind(kind, repeat, confidence);
    findings.set(entry.key, entry);
  }
  let repeat = 10;
  while (findings.size < 120) {
    for (const kind of ALL_KINDS) {
      if (findings.size >= 120) break;
      // Kept well above the three hand-picked findings' confidences
      // (10/20/30) above, so those three are always the first three the
      // default `needsInput`-sorted-ascending inbox view shows.
      const confidence = 35 + ((repeat * 13) % 65);
      const entry = buildFindingForKind(kind, repeat, confidence);
      findings.set(entry.key, entry);
    }
    repeat += 1;
  }

  // The three mergeable findings backing `e2e/specs/merge.spec.ts` — real
  // `defOverride`/`patchCollision` findings whose keys match
  // `mergeFixtures` above, low-confidence (so search alone finds them
  // reliably regardless of the generic findings' own confidences).
  findings.set("def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]", {
    key: "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]",
    kind: "defOverride",
    finding: {
      kind: "defOverride",
      key: { defType: "HediffDef", defName: "BionicHeart" },
      owners: ["ludeon.rimworld", "example.bionicsfork"],
      winner: "example.bionicsfork",
    },
    // Kept above the three `handPicked` findings' confidences (10/20/30
    // — see below) so this doesn't reorder `inbox.spec.ts`'s hardcoded
    // "first three by confidence" expectations.
    confidence: 42,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: {
          kind: "preferWinner",
          key: { defType: "HediffDef", defName: "BionicHeart" },
          winner: "ludeon.rimworld",
        },
        rationale: "prefer the framework's own values",
      },
      {
        action: {
          kind: "merge",
          key: { defType: "HediffDef", defName: "BionicHeart" },
          choices: {},
        },
        rationale:
          "merge field by field; keep the framework's values where the leaf mod didn't change them",
      },
    ],
    rationale: "the winner overrides a framework def",
    hasDecision: false,
    note: null,
    modIds: ["ludeon.rimworld", "example.bionicsfork"],
    mergeState: null,
  });
  findings.set(
    "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
    {
      key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
      kind: "defOverride",
      finding: {
        kind: "defOverride",
        key: { defType: "HeadTypeDef", defName: "HeadNormal" },
        owners: ["ludeon.rimworld", "example.bionicsfork", "exampleanim.mod"],
        winner: "exampleanim.mod",
      },
      confidence: 43,
      status: "needsInput",
      effective: { kind: "accept" },
      suggestionAction: { kind: "accept" },
      alternatives: [
        {
          action: {
            kind: "preferWinner",
            key: { defType: "HeadTypeDef", defName: "HeadNormal" },
            winner: "example.bionicsfork",
          },
          rationale: "prefer the middle owner instead",
        },
        {
          action: {
            kind: "merge",
            key: { defType: "HeadTypeDef", defName: "HeadNormal" },
            choices: {},
          },
          rationale: "merge field by field",
        },
      ],
      rationale: "three owners define this head with conflicting fields",
      hasDecision: false,
      note: null,
      modIds: ["ludeon.rimworld", "example.bionicsfork", "exampleanim.mod"],
      mergeState: null,
    },
  );
  // A clean, two-owner `patchCollision` with a real,
  // non-empty plan (`plantDensity` differs from the base — one real op)
  // bakes what a genuine auto-promotion looks like once it's already
  // happened: `status: "auto"`, `effective`/`suggestionAction` both
  // `merge`, `mergeState` populated (so the finding card's state pill
  // shows), no `hasDecision`. Confidence 85 matches
  // `redecide_for_clean_merge`'s own fixed value for this arm.
  // `patchCollision` is the *only* kind this shape is reachable for — a
  // `defOverride` with an identical clean, non-zero-op preview only ever
  // *leads with* `Merge` in the alternatives (see `CleanWall`'s own entry
  // above, and its doc comment for why this fixture — not `CleanWall` — is
  // what `clean-merge.spec.ts`'s "auto Merge card" spec exercises).
  findings.set(
    "patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[examplebiomes.biomes,example.flora.core]",
    {
      key: "patch_collision:BiomeDef/TemperateForest:defName:plantDensity:[examplebiomes.biomes,example.flora.core]",
      kind: "patchCollision",
      finding: {
        kind: "patchCollision",
        key: { defType: "BiomeDef", defName: "TemperateForest" },
        selector: "defName",
        subPath: "plantDensity",
        mods: ["example.flora.core", "examplebiomes.biomes"],
      },
      confidence: 85,
      status: "auto",
      effective: {
        kind: "merge",
        key: { defType: "BiomeDef", defName: "TemperateForest" },
        choices: {},
      },
      suggestionAction: {
        kind: "merge",
        key: { defType: "BiomeDef", defName: "TemperateForest" },
        choices: {},
      },
      alternatives: [{ action: { kind: "accept" }, rationale: "Keep the load-order winner." }],
      rationale: "The mods change different fields; merging keeps both.",
      hasDecision: false,
      note: null,
      modIds: ["example.flora.core", "examplebiomes.biomes"],
      mergeState: { kind: "complete", opCount: 1 },
    },
  );
  // Finding 12's own real case, matching the `wildAnimals` `mergeFixtures`
  // entry above.
  findings.set(
    "patch_collision:BiomeDef/AridShrubland:defName:wildAnimals:[wildlife.mod.a,wildlife.mod.b,wildlife.mod.c]",
    {
      key: "patch_collision:BiomeDef/AridShrubland:defName:wildAnimals:[wildlife.mod.a,wildlife.mod.b,wildlife.mod.c]",
      kind: "patchCollision",
      finding: {
        kind: "patchCollision",
        key: { defType: "BiomeDef", defName: "AridShrubland" },
        selector: "defName",
        subPath: "wildAnimals",
        mods: ["wildlife.mod.a", "wildlife.mod.b", "wildlife.mod.c"],
      },
      confidence: 45,
      status: "needsInput",
      effective: { kind: "accept" },
      suggestionAction: { kind: "accept" },
      alternatives: [
        {
          action: {
            kind: "merge",
            key: { defType: "BiomeDef", defName: "AridShrubland" },
            choices: {},
          },
          rationale: "merge field by field",
        },
      ],
      rationale: "contested patches apply additively in load order",
      hasDecision: false,
      note: null,
      modIds: ["wildlife.mod.a", "wildlife.mod.b", "wildlife.mod.c"],
      mergeState: null,
    },
  );
  // Two more `patchCollision` findings, matching the two extra
  // `mergeFixtures` entries above: `get_def_conflict_view`'s own mock
  // needs a live finding at each key (`Session::def_conflict_view`
  // always requires one under the hood, via `InspectDef`/`PlanMerge`).
  findings.set("patch_collision:ThingDef/Widget:def_name:label:[widget.mod.a,widget.mod.b]", {
    key: "patch_collision:ThingDef/Widget:def_name:label:[widget.mod.a,widget.mod.b]",
    kind: "patchCollision",
    finding: {
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Widget" },
      selector: "defName",
      subPath: "label",
      mods: ["widget.mod.a", "widget.mod.b"],
    },
    confidence: 50,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Widget" }, choices: {} },
        rationale: "merge field by field",
      },
    ],
    rationale: "contested patches apply additively in load order",
    hasDecision: false,
    note: null,
    modIds: ["widget.mod.a", "widget.mod.b"],
    mergeState: null,
  });
  findings.set("patch_collision:ThingDef/Sprocket:def_name:comps:[dup.mod.a,dup.mod.b]", {
    key: "patch_collision:ThingDef/Sprocket:def_name:comps:[dup.mod.a,dup.mod.b]",
    kind: "patchCollision",
    finding: {
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Sprocket" },
      selector: "defName",
      subPath: "comps",
      mods: ["dup.mod.a", "dup.mod.b"],
    },
    confidence: 90,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Sprocket" }, choices: {} },
        rationale: "merge field by field",
      },
    ],
    rationale: "contested patches apply additively in load order",
    hasDecision: false,
    note: null,
    modIds: ["dup.mod.a", "dup.mod.b"],
    mergeState: null,
  });
  findings.set("patch_collision:ThingDef/Wall:def_name:label:[wall.c.mod,wall.d.mod]", {
    key: "patch_collision:ThingDef/Wall:def_name:label:[wall.c.mod,wall.d.mod]",
    kind: "patchCollision",
    finding: {
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Wall" },
      selector: "defName",
      subPath: "label",
      mods: ["wall.c.mod", "wall.d.mod"],
    },
    confidence: 50,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Wall" }, choices: {} },
        rationale: "merge field by field",
      },
    ],
    rationale: "an unsupported patch operation stopped this def's replay partway through",
    hasDecision: false,
    note: null,
    modIds: ["wall.c.mod", "wall.d.mod"],
    mergeState: null,
  });
  // Backs `get_def_conflict_view`'s own `decisionWinners` fixture above —
  // a `PreferWinner` decision already exists on this finding, so its own
  // `Conflict` row's `preference` renders `{ kind: "decision" }`, not
  // `loadOrder`.
  findings.set("patch_collision:ThingDef/Gadget2:def_name:label:[gadget2.mod.a,gadget2.mod.b]", {
    key: "patch_collision:ThingDef/Gadget2:def_name:label:[gadget2.mod.a,gadget2.mod.b]",
    kind: "patchCollision",
    finding: {
      kind: "patchCollision",
      key: { defType: "ThingDef", defName: "Gadget2" },
      selector: "defName",
      subPath: "label",
      mods: ["gadget2.mod.a", "gadget2.mod.b"],
    },
    confidence: 50,
    status: "needsInput",
    effective: {
      kind: "preferWinner",
      key: { defType: "ThingDef", defName: "Gadget2" },
      winner: "gadget2.mod.a",
    },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Gadget2" }, choices: {} },
        rationale: "merge field by field",
      },
    ],
    rationale: "a decision forces gadget2.mod.a to win regardless of load order",
    hasDecision: true,
    note: null,
    modIds: ["gadget2.mod.a", "gadget2.mod.b"],
    mergeState: null,
  });
  // A `duplicateTemplateName` finding with no `mergeFixtures` entry at
  // all: `PlanMerge` never supports this kind, so `get_def_conflict_view`
  // renders touchers/problems only, straight off this finding's own
  // `owners` — the mock-tier twin of
  // `rim_session::test_support::duplicate_template_name_fixture`.
  findings.set("duplicate_template_name:GizmoBase:[gizmo.mod.a,gizmo.mod.b]", {
    key: "duplicate_template_name:GizmoBase:[gizmo.mod.a,gizmo.mod.b]",
    kind: "duplicateTemplateName",
    finding: {
      kind: "duplicateTemplateName",
      name: "GizmoBase",
      owners: ["gizmo.mod.a", "gizmo.mod.b"],
    },
    confidence: 50,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: {
          kind: "preferWinner",
          key: { defType: "template_name", defName: "GizmoBase" },
          winner: "gizmo.mod.a",
        },
        rationale: "Force one mod's template to win.",
      },
      {
        action: {
          kind: "preferWinner",
          key: { defType: "template_name", defName: "GizmoBase" },
          winner: "gizmo.mod.b",
        },
        rationale: "Force one mod's template to win.",
      },
    ],
    rationale: "two mods register the same template name",
    hasDecision: false,
    note: null,
    modIds: ["gizmo.mod.a", "gizmo.mod.b"],
    mergeState: null,
  });
  // A fourth hand-picked finding, alongside the three above: a
  // `textureOverride` with real `preferWinner`/`shipAsset` alternatives
  // (one per owner, in that order — mirrors
  // `rim_resolve::ledger::suggest::texture_override`'s own shape), backing
  // `e2e/specs/summary.spec.ts`'s "Use this one"/"Ship this file" button
  // clicks. The generic pool's own `textureOverride` findings
  // (`buildFindingForKind`) carry `alternatives: []`, so a hand-picked
  // one, same low-confidence convention as the other three, is the only
  // way to exercise a real `pickAlternative` click against a texture
  // finding. The synthetic `defType` below must match
  // `rim_resolve::domain::finding::TEXTURE_DEF_TYPE` (`"texture"`) exactly
  // — any other value (such as `"_texture_"`) would hide
  // `utils/format.ts::describeDefKey`'s real-def-vs-texture-def branch
  // from ever actually triggering in the mock tier.
  //
  // The owners are `mod.010`/`mod.015` on purpose: `suggestedOrder` swaps
  // exactly that pair, so the winner is `mod.015` under Current and
  // `mod.010` under Suggested while `owners` stays in scan order. The
  // `winner` literal below is a placeholder; `findingFor` recomputes it.
  findings.set("texture_override:Things/BionicWall:[mod.010,mod.015]", {
    key: "texture_override:Things/BionicWall:[mod.010,mod.015]",
    kind: "textureOverride",
    finding: {
      kind: "textureOverride",
      texturePath: "Things/BionicWall",
      owners: ["mod.010", "mod.015"],
      winner: "mod.015",
    },
    confidence: 44,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: {
          kind: "preferWinner",
          key: { defType: "texture", defName: "Things/BionicWall" },
          winner: "mod.010",
        },
        rationale: "Force one mod's texture to win.",
      },
      {
        action: {
          kind: "preferWinner",
          key: { defType: "texture", defName: "Things/BionicWall" },
          winner: "mod.015",
        },
        rationale: "Force one mod's texture to win.",
      },
      {
        action: { kind: "shipAsset", texturePath: "Things/BionicWall", from: "mod.010" },
        rationale: "Copy this mod's texture file into the merge mod, independent of load order.",
      },
      {
        action: { kind: "shipAsset", texturePath: "Things/BionicWall", from: "mod.015" },
        rationale: "Copy this mod's texture file into the merge mod, independent of load order.",
      },
    ],
    rationale: "a texture override is cosmetic and browsable in-game; it never blocks",
    hasDecision: false,
    note: null,
    modIds: ["mod.010", "mod.015"],
    mergeState: null,
  });

  // A fifth hand-picked finding, backing `clean-merge.spec.ts`'s
  // clean def-override coverage. An *undecided* def override never bakes
  // `status: "auto"` with `effective`/`suggestionAction` both `merge`:
  // `redecide_for_clean_merge`'s
  // `Complete { op_count > 0 }` arm only ever *leads with* `Merge` in the
  // alternatives for a `defOverride`, keeping the ledger's own original
  // action/confidence/rationale untouched (`lead_with_merge`) — a
  // field-merged def override is a copy no author shipped, so it must
  // never enter the merge mod without an explicit decision. This
  // fixture's own base suggestion (no strong signal — two owners,
  // neither declares a relation, no shared author, nothing vanilla)
  // would be `ledger::suggest::def_override`'s `Unknown` branch: `Accept`
  // 60, its own rationale, `PreferWinner` per owner plus a `Merge`
  // alternative. `redecide_for_clean_merge` then only replaces that
  // `Merge` alternative's own rationale and moves it to the front —
  // `action`/`confidence`/`rationale` are the *original* suggestion's,
  // verbatim. `mergeState: null` (lazy, matching `BionicHeart`'s/
  // `HeadNormal`'s own entries below — no preview exists until the merge
  // editor actually opens one; this mock never runs the real
  // `PlanMerge`).
  findings.set("def_override:ThingDef/CleanWall:[wall.core.mod,wall.addon.mod]", {
    key: "def_override:ThingDef/CleanWall:[wall.core.mod,wall.addon.mod]",
    kind: "defOverride",
    finding: {
      kind: "defOverride",
      key: { defType: "ThingDef", defName: "CleanWall" },
      owners: ["wall.core.mod", "wall.addon.mod"],
      winner: "wall.addon.mod",
    },
    confidence: 60,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: {
          kind: "merge",
          key: { defType: "ThingDef", defName: "CleanWall" },
          choices: {},
        },
        rationale: "The mods change different fields; merging would combine both.",
        rationaleCode: { kind: "mergeLeadDefOverrideCombine" },
      },
      {
        action: {
          kind: "preferWinner",
          key: { defType: "ThingDef", defName: "CleanWall" },
          winner: "wall.core.mod",
        },
        rationale: "Force a specific owner's def to win regardless of load order.",
        rationaleCode: { kind: "forceDefOverrideWinner" },
      },
      {
        action: {
          kind: "preferWinner",
          key: { defType: "ThingDef", defName: "CleanWall" },
          winner: "wall.addon.mod",
        },
        rationale: "Force a specific owner's def to win regardless of load order.",
        rationaleCode: { kind: "forceDefOverrideWinner" },
      },
    ],
    rationale: "Multiple mods define this def with no strong signal explaining the override.",
    rationaleCode: { kind: "defOverrideUnexplained" },
    hasDecision: false,
    note: null,
    modIds: ["wall.core.mod", "wall.addon.mod"],
    mergeState: null,
  });

  // The structural guard, matching
  // the `GuardedGadget` `mergeFixtures` entry above: mirrors
  // `redecide_for_clean_merge`'s own guarded rationale text (`rim-resolve`'s
  // "{field} differs between owners — field-level merging is unsafe;
  // confirm the load-order winner") and drops `Merge` from the
  // alternatives entirely, the same way the real ledger does once the
  // guard fires — `mergeState: null` (lazy, like `BionicHeart`'s own
  // entry above) rather than baking a result this mock never computes
  // until the merge editor actually opens.
  findings.set("def_override:ThingDef/GuardedGadget:[core.gadget.mod,guard.gadget.mod]", {
    key: "def_override:ThingDef/GuardedGadget:[core.gadget.mod,guard.gadget.mod]",
    kind: "defOverride",
    finding: {
      kind: "defOverride",
      key: { defType: "ThingDef", defName: "GuardedGadget" },
      owners: ["core.gadget.mod", "guard.gadget.mod"],
      winner: "guard.gadget.mod",
    },
    confidence: 55,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: {
          kind: "preferWinner",
          key: { defType: "ThingDef", defName: "GuardedGadget" },
          winner: "guard.gadget.mod",
        },
        rationale:
          "thingClass differs between owners — field-level merging is unsafe; confirm the load-order winner",
      },
    ],
    rationale:
      "thingClass differs between owners — field-level merging is unsafe; confirm the load-order winner",
    hasDecision: false,
    note: null,
    modIds: ["core.gadget.mod", "guard.gadget.mod"],
    mergeState: null,
  });

  // Four dedicated runtime-patch-collision/rule/placement fixtures, same
  // hand-picked-and-exact convention as the ones above
  // — real shapes straight off `rim_resolve::ledger::suggest`'s own
  // functions (`runtime_patch_collision`/`rule_overruled`/
  // `placement_overruled`/`placement_questioned`), not the generic pool's
  // formulaic ids, so a spec can search for one unambiguously and assert
  // an exact `decide` payload against it.
  findings.set(
    "runtime_patch_collision:Verse.Pawn:Kill:[alpha.runtimepatch.patcher,beta.runtimepatch.patcher,gamma.runtimepatch.patcher]",
    {
      key: "runtime_patch_collision:Verse.Pawn:Kill:[alpha.runtimepatch.patcher,beta.runtimepatch.patcher,gamma.runtimepatch.patcher]",
      kind: "runtimePatchCollision",
      finding: {
        kind: "runtimePatchCollision",
        targetType: "Verse.Pawn",
        targetMethod: "Kill",
        owners: [
          "alpha.runtimepatch.patcher",
          "beta.runtimepatch.patcher",
          "gamma.runtimepatch.patcher",
        ],
      },
      confidence: 85,
      status: "auto",
      effective: { kind: "accept" },
      suggestionAction: { kind: "accept" },
      alternatives: [
        {
          action: {
            kind: "preferWinner",
            key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
            winner: "alpha.runtimepatch.patcher",
          },
          rationale: "Force this mod's patch to run last, regardless of load order.",
        },
        {
          action: {
            kind: "preferWinner",
            key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
            winner: "beta.runtimepatch.patcher",
          },
          rationale: "Force this mod's patch to run last, regardless of load order.",
        },
        {
          action: {
            kind: "preferWinner",
            key: { defType: "runtime_target", defName: "Verse.Pawn::Kill" },
            winner: "gamma.runtimepatch.patcher",
          },
          rationale: "Force this mod's patch to run last, regardless of load order.",
        },
      ],
      rationale:
        "3 mods patch Verse.Pawn.Kill; gamma.runtimepatch.patcher loads last today, so its patch runs last.",
      hasDecision: false,
      note: null,
      modIds: [
        "alpha.runtimepatch.patcher",
        "beta.runtimepatch.patcher",
        "gamma.runtimepatch.patcher",
      ],
      mergeState: null,
    },
  );

  // A db (Steam Workshop) pair rule overruled by a `Declared` edge —
  // `winner.layer !== "hard"` so `Reorder` is offered, and `origin !==
  // "userDecision"` so `Promote` is too (`rule_overruled_confidence`'s
  // own table: 90 for a `Declared` winner). The matching imported rule
  // lives in `rules.pairs` below, so accepting the `Promote` alternative
  // (or deciding `promoteRule` straight from the inbox) has a real row to
  // copy.
  findings.set("rule_overruled:cabin.furniture.mod:cabin.core.mod:steam_db", {
    key: "rule_overruled:cabin.furniture.mod:cabin.core.mod:steam_db",
    kind: "ruleOverruled",
    finding: {
      kind: "ruleOverruled",
      after: "cabin.furniture.mod",
      before: "cabin.core.mod",
      origin: "steamDb",
      comment: null,
      winner: {
        after: "cabin.core.mod",
        before: "cabin.furniture.mod",
        layer: "declared",
        detail: "declares a modDependency on cabin.furniture.mod",
      },
      witnessCycle: [],
    },
    confidence: 90,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "reorder", after: "cabin.furniture.mod", before: "cabin.core.mod" },
        rationale: "Force this rule's own order instead, overriding the winner.",
      },
      {
        action: {
          kind: "promoteRule",
          rule: { kind: "pair", after: "cabin.furniture.mod", before: "cabin.core.mod" },
        },
        rationale:
          "Keep this rule as your own decision, so it survives even though it stays overruled.",
      },
    ],
    rationale:
      "cabin.furniture.mod loading after cabin.core.mod per a Steam Workshop dependency was overruled: an author declaration says cabin.core.mod loads after cabin.furniture.mod instead (declares a modDependency on cabin.furniture.mod).",
    hasDecision: false,
    note: null,
    modIds: ["cabin.furniture.mod", "cabin.core.mod"],
    mergeState: null,
  });

  // A community Bottom placement overruled by a load-time (`Hard`)
  // reference — `by.layer === "hard"` means no alternative can beat it
  // (`placement_overruled`'s own gate), so this is disclosure only; the
  // matching imported placement lives in `rules.placements` below.
  findings.set("placement_overruled:basement.optimizer.mod:bottom:rim_sort_community", {
    key: "placement_overruled:basement.optimizer.mod:bottom:rim_sort_community",
    kind: "placementOverruled",
    finding: {
      kind: "placementOverruled",
      modId: "basement.optimizer.mod",
      placement: "bottom",
      origin: "rimSortCommunity",
      by: {
        after: "load.time.framework.mod",
        before: "basement.optimizer.mod",
        layer: "hard",
        detail: "ships a load-time AssemblyRef",
      },
      landedAt: 7,
    },
    confidence: 95,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "basement.optimizer.mod is pinned at the bottom, but a hard (load-time) requirement overrules it: ships a load-time AssemblyRef.",
    hasDecision: false,
    note: null,
    modIds: ["basement.optimizer.mod", "load.time.framework.mod"],
    mergeState: null,
  });

  // A Top placement questioned by an `Awareness` (`usesType`) edge —
  // the placement wins; `Reorder` enforces the relation's own direction
  // (`mod after other` for a `Top` mod, per `placement_questioned`'s own
  // branch) as a user decision instead.
  findings.set("placement_questioned:priority.loader.mod:top:uses_type", {
    key: "placement_questioned:priority.loader.mod:top:uses_type",
    kind: "placementQuestioned",
    finding: {
      kind: "placementQuestioned",
      modId: "priority.loader.mod",
      placement: "top",
      other: "type.reference.mod",
      relationKind: "usesType",
      relationDetail: "type.reference.mod uses a type priority.loader.mod defines",
    },
    confidence: 85,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [
      {
        action: { kind: "reorder", after: "priority.loader.mod", before: "type.reference.mod" },
        rationale: "Enforce this relation explicitly as a user decision.",
      },
    ],
    rationale:
      "priority.loader.mod is pinned at the top; the type is resolved across every loaded assembly regardless of load order (type.reference.mod uses a type priority.loader.mod defines).",
    hasDecision: false,
    note: null,
    modIds: ["priority.loader.mod", "type.reference.mod"],
    mergeState: null,
  });

  // The rule-level promotes-dependents companion: a dedicated pin (own
  // mod id, own `rules.placements`
  // row below — deliberately *not* `basement.optimizer.mod` or `mod(0)`,
  // both already searched-for by their own dedicated single-result finding
  // fixtures above, which a same-substring key here would double-count)
  // with a real dependent count, so the rules page's "Promotes" column and
  // the add-rule flow's own e2e coverage have a genuine signal to assert
  // on — not every placement rule gets one (an uncontested pin, the
  // common case, deliberately has none; neither `mod(0)`'s own `Top` pin
  // nor `basement.optimizer.mod`'s own `Bottom` one gets a matching entry
  // here).
  findings.set("placement_promotes_dependents:crowded.framework.mod:bottom", {
    key: "placement_promotes_dependents:crowded.framework.mod:bottom",
    kind: "placementPromotesDependents",
    finding: {
      kind: "placementPromotesDependents",
      modId: "crowded.framework.mod",
      placement: "bottom",
      promoted: ["crowded.dependent.a", "crowded.dependent.b"],
    },
    confidence: 95,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "crowded.framework.mod is pinned at the bottom; crowded.dependent.a and crowded.dependent.b both declare a relation that would otherwise place them past it, so they are promoted alongside it.",
    hasDecision: false,
    note: null,
    modIds: ["crowded.framework.mod", "crowded.dependent.a", "crowded.dependent.b"],
    mergeState: null,
  });

  // Five more dedicated fixtures, same hand-picked-and-exact convention
  // as the block above: `undecodableTexture` (real shape off
  // `rim_resolve::ledger::suggest::assets::undecodable_texture`, never
  // given a dedicated row before this), the "content lost at load"/
  // "possible typo" kinds (`brokenInheritance`/`nearMissModReference`,
  // off `ledger::suggest::defs::broken_inheritance`/
  // `ledger::suggest::mods::near_miss_mod_reference`), the
  // deliberate-override finding (`discardedAddition`, off
  // `ledger::suggest::defs::discarded_addition`), and the dangling
  // def-reference finding (`danglingDefReference`, off
  // `ledger::suggest::defs::dangling_def_reference`). None of the five
  // is in `ALL_KINDS`, so without these the generic pool never
  // constructs one at all — see `findings.spec.ts` for the assertions.
  findings.set("undecodable_texture:example.giants.mod:giants/bodies/colossus_east", {
    key: "undecodable_texture:example.giants.mod:giants/bodies/colossus_east",
    kind: "undecodableTexture",
    finding: {
      kind: "undecodableTexture",
      modId: "example.giants.mod",
      path: "giants/bodies/colossus_east",
      width: 130,
      height: 130,
      fourcc: "DXT5",
      hasPngSibling: false,
    },
    confidence: 90,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "This .dds file won't decode — the base game shows the bad-texture placeholder in its place.",
    hasDecision: false,
    note: null,
    modIds: ["example.giants.mod"],
    mergeState: null,
  });
  findings.set("broken_inheritance:colony.additions.mod:missing:ExampleCreatureBase", {
    key: "broken_inheritance:colony.additions.mod:missing:ExampleCreatureBase",
    kind: "brokenInheritance",
    finding: {
      kind: "brokenInheritance",
      modId: "colony.additions.mod",
      parentName: "ExampleCreatureBase",
      child: { defType: "ThingDef", name: "ExampleCreatureBig", isTemplate: true },
      problem: { kind: "missingParent" },
      affected: [
        { defType: "ThingDef", defName: "ExampleCreature_Alpha" },
        { defType: "ThingDef", defName: "ExampleCreature_Beta" },
        { defType: "ThingDef", defName: "ExampleCreature_Gamma" },
      ],
      truncated: 0,
    },
    confidence: 90,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "This `ParentName` reference doesn't resolve the way the child's own element type expects — the def loads with content missing or wrong. Report this to the referencing mod's author.",
    hasDecision: false,
    note: null,
    modIds: ["colony.additions.mod"],
    mergeState: null,
  });
  findings.set("near_miss_mod_reference:drift.compat.mod:mayrequire:Example.CraftingFrameworkk", {
    key: "near_miss_mod_reference:drift.compat.mod:mayrequire:Example.CraftingFrameworkk",
    kind: "nearMissModReference",
    finding: {
      kind: "nearMissModReference",
      referrer: "drift.compat.mod",
      referenceKind: "mayRequireId",
      written: "Example.CraftingFrameworkk",
      candidate: "example.craftingframework",
      candidateName: "Example Crafting Framework",
      rule: "nearMiss",
      file: "1.6/Defs/Recipes/RecipeDefs_Misc.xml",
    },
    confidence: 45,
    status: "needsInput",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "This MayRequire id 'Example.CraftingFrameworkk' resolves to no active or installed mod, but closely resembles the active mod 'Example Crafting Framework' — possibly a typo. Report this to the referencing mod's author.",
    hasDecision: false,
    note: null,
    modIds: ["drift.compat.mod"],
    mergeState: null,
  });
  findings.set(
    "discarded_addition:example.biomecore.mod:example.plantsexpanded.mod:ThingDef/ExampleShrubland:wildPlants",
    {
      key: "discarded_addition:example.biomecore.mod:example.plantsexpanded.mod:ThingDef/ExampleShrubland:wildPlants",
      kind: "discardedAddition",
      finding: {
        kind: "discardedAddition",
        replacer: "example.biomecore.mod",
        adder: "example.plantsexpanded.mod",
        def: { defType: "ThingDef", defName: "ExampleShrubland" },
        path: "ThingDef/ExampleShrubland/wildPlants",
        adderPath: "ThingDef/ExampleShrubland/wildPlants",
      },
      confidence: 80,
      status: "auto",
      effective: { kind: "accept" },
      suggestionAction: { kind: "accept" },
      alternatives: [],
      rationale:
        "example.biomecore.mod declares it loads after example.plantsexpanded.mod and replaces 'ThingDef/ExampleShrubland/wildPlants': example.plantsexpanded.mod's own addition at 'ThingDef/ExampleShrubland/wildPlants' is discarded on purpose.",
      hasDecision: false,
      note: null,
      modIds: ["example.biomecore.mod", "example.plantsexpanded.mod"],
      mergeState: null,
    },
  );
  findings.set("dangling_def_reference:ExampleGhostResearch", {
    key: "dangling_def_reference:ExampleGhostResearch",
    kind: "danglingDefReference",
    finding: {
      kind: "danglingDefReference",
      name: "ExampleGhostResearch",
      referrers: [
        {
          referrer: {
            kind: "referencedFromDef",
            modId: "example.techtree.mod",
            defType: "ThingDef",
            defName: "ExampleWorkbench",
          },
          fieldPath: "researchPrerequisites/li",
        },
      ],
      truncatedReferrers: 0,
      cause: {
        kind: "onlyInUnloadedFolder",
        modId: "example.techtree.mod",
        folder: "1.6NotOdyssey",
      },
      likelySound: false,
    },
    confidence: 85,
    status: "auto",
    effective: { kind: "accept" },
    suggestionAction: { kind: "accept" },
    alternatives: [],
    rationale:
      "This name resolves to no active def of any type — it's defined only in example.techtree.mod's own '1.6NotOdyssey' folder, which this install doesn't load. Cross-references resolve after every mod's defs and patches have loaded, so this never depends on load order.",
    hasDecision: false,
    note: null,
    modIds: ["example.techtree.mod"],
    mergeState: null,
  });

  const tagsByMod = new Map<string, string[]>([
    [modId(2), ["framework"]],
    [modId(9), ["framework"]],
  ]);
  for (const [id, tags] of tagsByMod) {
    const mod = mods.find((m) => m.modId === id);
    if (mod) mod.tags = tags;
  }

  // `promotedFrom`/`alreadyPromoted` are never stored on a row directly —
  // like the real backend's `StoredRules`/`PairRule`, this mock's own
  // "on disk" shape carries only what a rule actually is; both fields are
  // (re)computed fresh by `withPromotionFields` below, over the *full*
  // set, every time a rules-page command responds (mirroring
  // `dto::rule::RuleSetDto::from(&StoredRules)`).
  type RawPairRule = Omit<PairRuleDto, "promotedFrom" | "alreadyPromoted">;
  type RawPlacementRule = Omit<PlacementRuleDto, "promotedFrom" | "alreadyPromoted">;
  interface RawRuleSet {
    pairs: RawPairRule[];
    placements: RawPlacementRule[];
    incompatibles: RuleSetDto["incompatibles"];
    warnings: string[];
  }

  let rules: RawRuleSet = {
    pairs: [
      {
        after: modId(3),
        before: modId(4),
        origin: "rimSortUser",
        comment: "known-good order",
        overridesDeclared: false,
      },
      // Backs the `ruleOverruled` dedicated fixture above's `promoteRule`
      // alternative — a real imported row to copy into a `userDecision`
      // one.
      {
        after: "cabin.furniture.mod",
        before: "cabin.core.mod",
        origin: "steamDb",
        comment: "steam db: furniture needs core",
        overridesDeclared: false,
      },
    ],
    placements: [
      { modId: modId(0), placement: "top", origin: "rimSortCommunity", comment: null },
      // Backs the `placementOverruled` dedicated fixture above — no
      // `promoteRule` alternative is offered for it (a `Hard` winner),
      // but the row still needs to exist for the rules page to show it
      // greyed/imported the same way any other community placement is.
      {
        modId: "basement.optimizer.mod",
        placement: "bottom",
        origin: "rimSortCommunity",
        comment: "keep performance mods last",
      },
      // Backs the `placementPromotesDependents` dedicated fixture above —
      // its own mod id, so a real dependent count has a real row to
      // render on.
      {
        modId: "crowded.framework.mod",
        placement: "bottom",
        origin: "userDecision",
        comment: null,
      },
    ],
    incompatibles: [],
    warnings: [],
  };

  /** See the `rules` declaration's own doc comment. */
  function withPromotionFields(raw: RawRuleSet): RuleSetDto {
    const pairs: PairRuleDto[] = raw.pairs.map((pair) => ({
      ...pair,
      promotedFrom:
        pair.origin === "userDecision"
          ? (raw.pairs.find(
              (other) =>
                other.after === pair.after &&
                other.before === pair.before &&
                other.origin !== "userDecision",
            )?.origin ?? null)
          : null,
      alreadyPromoted:
        pair.origin !== "userDecision" &&
        raw.pairs.some(
          (other) =>
            other.origin === "userDecision" &&
            other.after === pair.after &&
            other.before === pair.before,
        ),
    }));
    const placements: PlacementRuleDto[] = raw.placements.map((placement) => ({
      ...placement,
      promotedFrom:
        placement.origin === "userDecision"
          ? (raw.placements.find(
              (other) => other.modId === placement.modId && other.origin !== "userDecision",
            )?.origin ?? null)
          : null,
      alreadyPromoted:
        placement.origin !== "userDecision" &&
        raw.placements.some(
          (other) => other.origin === "userDecision" && other.modId === placement.modId,
        ),
    }));
    return { pairs, placements, incompatibles: raw.incompatibles, warnings: raw.warnings };
  }

  let settings: SettingsDto = {
    threshold: 80,
    enforceSoft: false,
    enforceAwareness: false,
    suggestMergeWhenClean: true,
    tieBreak: "rebuild",
    useImportedPairs: false,
    useImportedPlacements: true,
    enforceInferred: true,
    showDanglingDefReferences: false,
  };
  // Mirrors `rim_session::Settings::default()` — deliberately differs
  // from the scenario's own `settings` above (`useImportedPairs: false`)
  // so a "Reset to defaults" test can observe the refill.
  const DEFAULT_SETTINGS: SettingsDto = {
    threshold: 80,
    enforceSoft: false,
    enforceAwareness: false,
    suggestMergeWhenClean: true,
    tieBreak: "rebuild",
    useImportedPairs: true,
    useImportedPlacements: true,
    enforceInferred: true,
    showDanglingDefReferences: false,
  };
  // The app-global network/reminder preferences — mirrors
  // `rim_session::AppSettings::default()`. Separate from `settings` above
  // (a per-profile fixture): this state is app-global, not per profile.
  let appSettings: AppSettingsDto = {
    network: {
      allowNetwork: true,
      checkForUpdates: true,
      autoRefreshRuleDatabases: true,
      fetchCommunityRules: true,
      fetchSteamWorkshop: true,
      fetchRimmergeRules: true,
    },
    reminders: {
      ruleDatabasesStaleAfterDays: 30,
    },
  };
  const DEFAULT_NETWORK_POLICY: AppSettingsDto["network"] = {
    allowNetwork: true,
    checkForUpdates: true,
    autoRefreshRuleDatabases: true,
    fetchCommunityRules: true,
    fetchSteamWorkshop: true,
    fetchRimmergeRules: true,
  };
  let selected: OrderSourceDto = "current";

  // The working
  // active-mod set's own three-tier mock state, mirroring
  // `rim_session::Session::pending_changes`'s two diffs. Each tier is the
  // **full** set of currently-active ids (base pool ∪ any activated
  // `inactiveMods` entry), not a delta from the base pool — `working`
  // diverges from `scanned` via `activate_mods`/`deactivate_mods`
  // (`.add`/`.delete` directly on `workingActiveIds`); `scanned` catches
  // up to `working` via `rescan_project`; `file` catches up to `scanned`
  // via a successful `apply { writeModsConfig: true }` — the identical
  // three-state shape `Session.working`/`orders().current`/
  // `file_active_mods` models in the real backend.
  //
  // Tiers are full sets rather than delta pairs (ids newly activated from
  // the inactive pool, ids newly deactivated from the base pool) because
  // diffing deltas drops a real case: activating `mod.906`, rescanning,
  // then deactivating it again removes it from the working "activated"
  // delta without ever recording a removal, so an unscanned diff computed
  // only from the removal deltas never sees it. With full sets, `mod.906`
  // simply stops being in `workingActiveIds` while it's still in
  // `scannedActiveIds`, which is exactly what `removed` means.
  const baseActiveIds = new Set([...mods, ...extraModsFromFixtures()].map((mod) => mod.modId));
  const workingActiveIds = new Set(baseActiveIds);
  let scannedActiveIds = new Set(baseActiveIds);
  let fileActiveIds = new Set(baseActiveIds);
  // The file's own list, in order: what `Session::file_matches` compares an
  // order against (the Set above only models membership). It starts as the
  // current order and becomes whichever order a successful
  // `apply { writeModsConfig: true }` wrote.
  let fileOrder = [...currentOrder];

  // The Databases
  // card's own mock state — deliberately narrower than `RuleDatabaseViewDto`
  // itself: `enabled`/`needsReimport` are *derived* facts (from the live
  // `settings` toggles, and from comparing `cached.sha256` to
  // `importedSha256`), so storing them separately would let them drift
  // from what a real backend always recomputes fresh — see
  // `get_rule_databases` below. `community` starts already-cached and
  // matching what this profile last "imported" so `needsReimport` starts
  // `false`; `steam` starts never fetched, mirroring `Settings::default`.
  type RuleDatabaseMockState = Pick<
    RuleDatabaseViewDto,
    "database" | "cached" | "lastFailure" | "importedSha256"
  >;
  let ruleDatabases: RuleDatabaseMockState[] = [
    {
      database: "community",
      cached: {
        sha256: "b00fa152965643381ecedcf9043fc0d46b272cb9cf3270ec58651eddf68741ab",
        bytes: 393_897,
        fetchedAt: "2026-08-01T00:00:00Z",
      },
      lastFailure: null,
      importedSha256: "b00fa152965643381ecedcf9043fc0d46b272cb9cf3270ec58651eddf68741ab",
    },
    {
      database: "steam",
      cached: null,
      lastFailure: null,
      importedSha256: null,
    },
    // The third source. Never fetched, and
    // `importedSha256` stays `null` forever — this one is read straight
    // from the cache at `LoadProject` time, never imported into a
    // profile, so it can never report "needs re-import".
    {
      database: "rimmerge",
      cached: null,
      lastFailure: null,
      importedSha256: null,
    },
  ];

  /**
   * Which `NetworkPolicy` toggle governs one source's own fetch — mirrors
   * `rim_session::NetworkPolicy::fetches`.
   * A plain `switch` with no `assertNever`: this whole scenario is
   * serialized into the page by `addInitScript`, so it can only ever
   * reference values defined inside it.
   */
  function fetchEnabledFor(database: RuleDatabaseViewDto["database"]): boolean {
    switch (database) {
      case "community":
        return appSettings.network.fetchCommunityRules;
      case "steam":
        return appSettings.network.fetchSteamWorkshop;
      case "rimmerge":
        return appSettings.network.fetchRimmergeRules;
    }
  }

  /** A plausible refreshed size per source, for the mock only. */
  function mockRefreshedBytes(database: RuleDatabaseViewDto["database"]): number {
    switch (database) {
      case "community":
        return 400_000;
      case "steam":
        return 51_000_000;
      case "rimmerge":
        return 2_048;
    }
  }

  function sameOrderIgnoringMergeMod(a: string[], b: string[]): boolean {
    const strip = (ids: string[]): string[] => ids.filter((id) => id !== MERGE_MOD_PACKAGE_ID);
    const left = strip(a);
    const right = strip(b);
    return left.length === right.length && left.every((id, index) => id === right[index]);
  }

  /**
   * Mirrors `rim_resolve::preflight::hard_problems` for the finding kinds
   * this scenario models (a missing dependency, an incompatible pair, a
   * missing mod): the same inclusion rules (both sides in the written
   * order, the dependency absent from it), `acknowledged` = the finding's
   * own status is `userOverridden`, and the same ordering (kind, then
   * ids). The scenario has no hard-edge or any-of data, so those two
   * kinds never appear here. A missing mod is kept in the current order
   * (the file lists it) and removed from the suggested one.
   */
  function hardProblemsFor(source: OrderSourceDto): PreflightItemDto[] {
    const order = new Set(orderFor(source));
    const kindRank = {
      missingDependency: 0,
      incompatiblePair: 1,
      missingMod: 2,
    } as const;
    const ranked: { rank: number; ids: string; item: PreflightItemDto }[] = [];
    for (const entry of findings.values()) {
      const acknowledged = entry.status === "userOverridden";
      const finding = entry.finding;
      if (finding.kind === "missingDependency") {
        if (!order.has(finding.modId) || order.has(finding.dependency)) continue;
        ranked.push({
          rank: kindRank.missingDependency,
          ids: `${finding.modId}|${finding.dependency}`,
          item: {
            acknowledged,
            problem: {
              kind: "missingDependency",
              modId: finding.modId,
              dependency: finding.dependency,
              displayName: finding.displayName,
              availability: inactiveMods.some((mod) => mod.modId === finding.dependency)
                ? "installedInactive"
                : "notInstalled",
            },
          },
        });
      } else if (finding.kind === "incompatiblePair") {
        // The real report never pairs a mod with itself (the generated
        // fixture findings sometimes do).
        if (finding.a === finding.b) continue;
        if (!order.has(finding.a) || !order.has(finding.b)) continue;
        const [a, b] = finding.a <= finding.b ? [finding.a, finding.b] : [finding.b, finding.a];
        ranked.push({
          rank: kindRank.incompatiblePair,
          ids: `${a}|${b}`,
          item: { acknowledged, problem: { kind: "incompatiblePair", a, b } },
        });
      } else if (finding.kind === "missingMod") {
        ranked.push({
          rank: kindRank.missingMod,
          ids: finding.modId,
          item: {
            acknowledged,
            problem: {
              kind: "missingMod",
              modId: finding.modId,
              outcome: source === "current" ? "keptInActiveList" : "removedFromActiveList",
            },
          },
        });
      }
    }
    ranked.sort((x, y) => x.rank - y.rank || (x.ids < y.ids ? -1 : x.ids > y.ids ? 1 : 0));
    return ranked.map((row) => row.item);
  }

  function orderFor(source: OrderSourceDto): string[] {
    return source === "current" ? currentOrder : suggestedOrder;
  }

  function needsInputCountFor(id: string): number {
    let count = 0;
    for (const entry of findings.values()) {
      if (entry.status === "needsInput" && entry.modIds.includes(id)) count += 1;
    }
    return count;
  }

  /**
   * Copies the imported pair/placement rule matching `key` into a new
   * `userDecision`-origin rule at the same key, leaving the import in
   * place — a no-op when nothing imported matches or a `userDecision`
   * rule already occupies that key. Mirrors `Session::
   * promote_imported_rule`; shared by the dedicated `promote_imported_rule`
   * fixture handler (the rules page's own "Promote" button) and `decide` (an
   * `Action::PromoteRule` decided straight from the inbox:
   * `Decide::execute` persists the promotion too, not just the
   * decision).
   */
  function promoteRuleAtKey(key: RuleKeyDto): void {
    if (key.kind === "pair") {
      const alreadyPromoted = rules.pairs.some(
        (p) => p.origin === "userDecision" && p.after === key.after && p.before === key.before,
      );
      const imported = rules.pairs.find(
        (p) => p.after === key.after && p.before === key.before && p.origin !== "userDecision",
      );
      if (!alreadyPromoted && imported) {
        rules = {
          ...rules,
          pairs: [...rules.pairs, { ...imported, origin: "userDecision" as const }],
        };
      }
    } else if (key.kind === "placement") {
      const alreadyPromoted = rules.placements.some(
        (p) => p.origin === "userDecision" && p.modId === key.modId,
      );
      const imported = rules.placements.find(
        (p) => p.modId === key.modId && p.origin !== "userDecision",
      );
      if (!alreadyPromoted && imported) {
        rules = {
          ...rules,
          placements: [...rules.placements, { ...imported, origin: "userDecision" as const }],
        };
      }
    }
  }

  function stats() {
    let auto = 0;
    let needsInput = 0;
    let overridden = 0;
    for (const entry of findings.values()) {
      if (entry.status === "auto") auto += 1;
      else if (entry.status === "needsInput") needsInput += 1;
      else overridden += 1;
    }
    return { auto, needsInput, overridden, resolvedBySuggested: 0 };
  }

  /**
   * The finding as the backend would send it for the selected order. A
   * `textureOverride`'s `winner` is the owner loaded last in that order
   * (`ledger::findings::conflicts::last_loaded`: an owner the order does
   * not list never beats a listed one, and with none listed the last
   * owner stands in); `owners` stays in scan order.
   */
  function findingFor(entry: FindingEntry): FindingDto {
    const finding = entry.finding;
    if (finding.kind !== "textureOverride") return finding;
    return { ...finding, winner: lastLoaded(finding.owners, orderFor(selected), finding.winner) };
  }

  /**
   * Mirrors `ledger::findings::conflicts::last_loaded`: the owner loaded last in `order`.
   * An owner the order does not list never beats a listed one, and with none listed the
   * last owner stands in (`fallback` when there are none at all).
   */
  function lastLoaded(
    owners: readonly string[],
    order: readonly string[],
    fallback: string,
  ): string {
    let winner = owners[owners.length - 1] ?? fallback;
    let winnerPosition = -1;
    for (const owner of owners) {
      const position = order.indexOf(owner);
      if (position > winnerPosition) {
        winner = owner;
        winnerPosition = position;
      }
    }
    return winner;
  }

  function summaryOf(entry: FindingEntry) {
    return {
      key: entry.key,
      finding: findingFor(entry),
      status: entry.status,
      confidence: entry.confidence,
      effective: entry.effective,
      hasDecision: entry.hasDecision,
      mergeState: entry.mergeState,
      // No fixture in this pool
      // bakes a pre-existing decided merge on a guarded def, so this is
      // always `null` here — `FindingCard.vue`'s own guard-aware wording
      // has Vitest coverage instead (`FindingCard.test.ts`).
      structuralGuardField: null,
      // The real `ResolutionSummaryDto.scope` is `Option<ScopeMembershipDto>`,
      // which ts-rs/serde always serializes with the key present (`null`
      // for the profile inbox) — never omitted. Omitting it here (as this
      // literal did before) left `resolution.scope` `undefined` at
      // runtime, not `null`, which `FindingCard.vue`'s `scope !== null`
      // check for "is this a patch's own scoped inbox" can't tell apart
      // from a real `Full`/`Partial` membership.
      scope: null,
    };
  }

  /**
   * Every fixture in this file hand-writes its own English `rationale`
   * text rather than modeling the real backend's typed `Rationale`
   * variants, and `RationaleDto` has no generic/free-text variant (the
   * real backend's own transitional `Verbatim` bridge was removed once
   * every production site converted to a typed one) — so every mock
   * suggestion/alternative gets this single, fixed, otherwise-arbitrary
   * placeholder code instead. No spec in this suite asserts on rendered
   * rationale text (only presence, via each row's own `rationale`
   * string, kept unchanged above), so the placeholder's own content is
   * never wrong for a spec, only unrelated to the mock's own English —
   * exactly as unrelated as this file's `rationale` text already was to
   * the real backend's.
   */
  const GENERIC_RATIONALE_CODE = { kind: "keepCurrentWinner" as const };

  function detailOf(entry: FindingEntry) {
    return {
      key: entry.key,
      finding: findingFor(entry),
      suggestion: {
        action: entry.suggestionAction,
        confidence: entry.confidence,
        rationale: entry.rationale,
        rationaleCode: entry.rationaleCode ?? GENERIC_RATIONALE_CODE,
        alternatives: entry.alternatives.map((alternative) => ({
          ...alternative,
          rationaleCode: alternative.rationaleCode ?? GENERIC_RATIONALE_CODE,
        })),
      },
      status: entry.status,
      effective: entry.effective,
      note: entry.note,
      hasDecision: entry.hasDecision,
      resolvedBySuggested: selected === "current" ? false : null,
      mergeState: entry.mergeState,
      structuralGuardField: null,
      // See `summaryOf`'s own comment — the profile's own detail is
      // never scoped either.
      scope: null,
    };
  }

  // --- Merge mod page fixtures ----------------------------------------
  //
  // `get_merge_mod`/`preview_merge_mod_file` are derived dynamically from
  // `mergeFixtures`/`mergeChoicesByKey` above rather than seeded as their
  // own separate state: one entry per finding actually decided as a
  // `Merge` (mirroring the real backend's "one entry per Merge/ShipAsset
  // decision"), so a spec sees exactly the entries its own UI actions
  // (deciding `BionicHeart`/`HeadNormal` via `m`) produced — nothing is
  // pre-decided here, which would otherwise vanish those findings from
  // the inbox's default `needsInput` view before `merge.spec.ts`'s
  // earlier specs ever get to open them.
  const MERGE_MOD_PACKAGE_ID = "rimmerge.merge.3f9a1c2b7d5e";
  const MERGE_MOD_FOLDER = "rimmerge_merge_3f9a1c2b7d5e";
  const MERGE_MOD_PATH = `C:/RimWorld/Mods/${MERGE_MOD_FOLDER}`;
  // A real, minimal 1x1 PNG — `read_texture`'s mock returns this for any
  // mod/texture-path combination, since the mock tier never has real
  // per-mod texture files to distinguish (unlike the CDP smoke tier's
  // `merge_game` fixture, which ships two distinguishable ones).
  const ONE_PIXEL_PNG_DATA_URL =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAIAAAUAAen63NgAAAAASUVORK5CYII=";
  /** Flipped by `apply`'s own `writeMergeMod` handling below — `true` once an apply has actually rendered the folder, `false` again once one has removed it. */
  let mergeModExists = false;

  interface MergeModEntrySource {
    key: string;
    fixture: MergeFixture;
    state: MergeStateDto;
  }

  /**
   * `render_merge_mod`'s own
   * `matches!(entry.key, FindingKey::PatchCollision { .. })` filter,
   * mirrored here — an undecided `defOverride` never enters the merge
   * mod, however clean its preview, since `redecide_for_clean_merge`'s
   * `Complete { op_count > 0 }` arm only ever *leads with* `Merge` in the
   * alternatives for a `defOverride`, never sets it as the action. So
   * `entry.effective.kind === "merge"` alone is not sufficient for that
   * kind — it also needs
   * `hasDecision` (the mock's own stand-in for "an explicit `Action::Merge`
   * decision exists"). A `patchCollision` is unaffected: mirroring
   * RimWorld's own sequential patch composition, it still enters whether
   * or not it was explicitly decided.
   */
  function mergeModEntrySources(): MergeModEntrySource[] {
    const out: MergeModEntrySource[] = [];
    for (const [key, fixture] of mergeFixtures) {
      const entry = findings.get(key);
      if (entry?.effective.kind !== "merge") continue;
      if (!entry.hasDecision && fixture.kind !== "patchCollision") continue;
      const choices = mergeChoicesByKey.get(key) ?? {};
      out.push({ key, fixture, state: computeMergeState(fixture, choices) });
    }
    return out;
  }

  /**
   * Every mod id this scenario can name, mapped to its display name —
   * the generic 60-mod pool plus every hand-crafted owner named only
   * inside a `registerMergeFixture` call (`ludeon.rimworld`,
   * `example.bionicsfork`, `exampleanim.mod`, `example.flora.core`,
   * `examplebiomes.biomes`), which never appear in `mods`. Derived
   * from `mergeFixtures` rather than a separate hardcoded list so a new
   * fixture's owners are named automatically.
   */
  function allModNames(): Record<string, string> {
    const names = new Map(mods.map((mod) => [mod.modId, mod.name] as const));
    for (const fixture of mergeFixtures.values()) {
      for (const owner of fixture.owners) {
        if (!names.has(owner.modId)) names.set(owner.modId, owner.name);
      }
    }
    for (const mod of ASSIGNMENT_FIXTURE_MODS) {
      if (!names.has(mod.modId)) names.set(mod.modId, mod.name);
    }
    // The real `list_mod_names_inner` unions
    // `report.inactive_mods` in too, so `mod.9xx`'s own Mods-page
    // Inactive tab resolves through `useModLabel` like every other row
    // instead of falling back to its raw id — this mock mirrors that.
    for (const mod of inactiveMods) {
      if (!names.has(mod.modId)) names.set(mod.modId, mod.name);
    }
    return Object.fromEntries(names);
  }

  /**
   * Every hand-crafted `mergeFixtures` owner (`ludeon.rimworld`,
   * `example.bionicsfork`, ...) that never appears in the generic 60-mod
   * `mods` pool, shaped as `list_mods` rows — so `ScopeEditor`'s mod
   * search can actually find them to build a patch scope around the
   * existing `BionicHeart`/`HeadNormal` fixtures (the patches spec).
   * Derived from `mergeFixtures`, the same
   * "don't hardcode a second list" approach `allModNames` already uses.
   */
  function extraModsFromFixtures(): (typeof mods)[number][] {
    const seen = new Set(mods.map((mod) => mod.modId));
    const extra: (typeof mods)[number][] = [];
    for (const fixture of mergeFixtures.values()) {
      for (const owner of fixture.owners) {
        if (seen.has(owner.modId)) continue;
        seen.add(owner.modId);
        extra.push({
          modId: owner.modId,
          name: owner.name,
          source: "workshop",
          tags: [],
          hardDependents: 0,
          generated: null,
        });
      }
    }
    for (const mod of ASSIGNMENT_FIXTURE_MODS) {
      if (seen.has(mod.modId)) continue;
      seen.add(mod.modId);
      extra.push({ ...mod, source: "workshop", tags: [], hardDependents: 0, generated: null });
    }
    return extra;
  }

  /**
   * One `inactiveMods` entry shaped as a `list_mods`/`list_inactive_mods`
   * row.
   */
  function inactiveModRow(id: string): (typeof mods)[number] {
    const found = inactiveMods.find((mod) => mod.modId === id);
    if (!found) throw new Error(`inactiveModRow: unknown id "${id}"`);
    return {
      modId: found.modId,
      name: found.name,
      source: found.source,
      tags: [],
      hardDependents: 0,
      generated: null,
    };
  }

  /**
   * The active mod list as the *last scan* sees it (`list_mods`/
   * `get_pending_active_changes`'s own `unscanned`/`unapplied` diffs
   * both read off this) — every row (base pool ∪ `inactiveMods`) whose
   * id is in `scannedActiveIds`.
   */
  function scannedActiveModList(): (typeof mods)[number][] {
    const allRows = [
      ...mods,
      ...extraModsFromFixtures(),
      ...inactiveMods.map((mod) => inactiveModRow(mod.modId)),
    ];
    return allRows.filter((mod) => scannedActiveIds.has(mod.modId));
  }

  /** The Inactive tab's own list: every `inactiveMods` entry not currently (scanned-)active. */
  function scannedInactiveModList(): (typeof mods)[number][] {
    return inactiveMods
      .filter((mod) => !scannedActiveIds.has(mod.modId))
      .map((mod) => inactiveModRow(mod.modId));
  }

  /** `PendingActiveChangesDto`-shaped diff between two full active-id sets. */
  function activeSetDiff(
    newer: Set<string>,
    older: Set<string>,
  ): { added: string[]; removed: string[] } {
    return {
      added: [...newer].filter((id) => !older.has(id)),
      removed: [...older].filter((id) => !newer.has(id)),
    };
  }

  function pendingActiveChanges() {
    return {
      unscanned: activeSetDiff(workingActiveIds, scannedActiveIds),
      unapplied: activeSetDiff(scannedActiveIds, fileActiveIds),
    };
  }

  /**
   * Every other currently-(working-)active `inactiveMods` entry that
   * declares `id` in its own `dependsOn` — the mock's own `dependents_of`.
   */
  function dependentsOf(id: string): string[] {
    return inactiveMods
      .filter((mod) => workingActiveIds.has(mod.modId) && mod.dependsOn.includes(id))
      .map((mod) => mod.modId);
  }

  function planActivateMods(ids: string[], withDependencies: boolean) {
    const toAdd: string[] = [];
    const alreadyActive: string[] = [];
    const unresolvableDependencies: Record<string, string[]> = {};
    const scheduled = new Set<string>();
    for (const id of ids) {
      if (workingActiveIds.has(id) || scheduled.has(id)) {
        alreadyActive.push(id);
        continue;
      }
      if (withDependencies) {
        for (const dep of inactiveMods.find((mod) => mod.modId === id)?.dependsOn ?? []) {
          if (workingActiveIds.has(dep) || scheduled.has(dep)) continue;
          if (!inactiveMods.some((mod) => mod.modId === dep)) {
            unresolvableDependencies[id] = [...(unresolvableDependencies[id] ?? []), dep];
            continue;
          }
          scheduled.add(dep);
          toAdd.push(dep);
        }
      }
      scheduled.add(id);
      toAdd.push(id);
    }
    return { toAdd, unresolvableDependencies, alreadyActive };
  }

  function planDeactivateMods(ids: string[]) {
    const toRemove: string[] = [];
    const refused: string[] = [];
    const dependentsStillActive: Record<string, string[]> = {};
    for (const id of ids) {
      if (!workingActiveIds.has(id)) continue;
      toRemove.push(id);
    }
    for (const id of toRemove) {
      const dependents = dependentsOf(id);
      if (dependents.length > 0) dependentsStillActive[id] = dependents;
    }
    return { toRemove, dependentsStillActive, refused };
  }

  /** Every mod named as a source of the generated mod's content — every non-base owner of every decided entry. */
  function mergeSourceMods(): { modId: string; name: string }[] {
    const byId = new Map<string, string>();
    for (const { fixture } of mergeModEntrySources()) {
      for (const owner of fixture.owners) {
        if (owner.modId !== fixture.base) byId.set(owner.modId, owner.name);
      }
    }
    return [...byId.entries()].map(([id, name]) => ({ modId: id, name }));
  }

  /** Def types with at least one complete, non-empty entry — one `Patches/rimmerge_<DefType>.xml` per type. */
  function mergeModDefTypesWithOps(): string[] {
    const types = new Set<string>();
    for (const { fixture, state } of mergeModEntrySources()) {
      if (state.kind === "complete" && state.opCount > 0) types.add(fixture.defKey.defType);
    }
    return [...types];
  }

  function mergeModFiles(): string[] {
    const defTypes = mergeModDefTypesWithOps();
    if (defTypes.length === 0) return [];
    return [
      "About/About.xml",
      ...defTypes.map((defType) => `Patches/rimmerge_${defType}.xml`),
      "rimmerge.json",
    ];
  }

  function aboutXml(): string {
    const dependencyLines = mergeSourceMods()
      .map(
        (mod) =>
          `    <li><packageId>${mod.modId}</packageId><displayName>${mod.name}</displayName></li>`,
      )
      .join("\n");
    return [
      '<?xml version="1.0" encoding="utf-8"?>',
      "<ModMetaData>",
      `  <packageId>${MERGE_MOD_PACKAGE_ID}</packageId>`,
      "  <name>Rimmerge merge patch</name>",
      "  <author>Rimmerge</author>",
      "  <description>Generated by Rimmerge from your merge decisions. Regenerated on every apply — do not edit by hand.</description>",
      "  <supportedVersions><li>1.6</li></supportedVersions>",
      "  <modDependencies>",
      dependencyLines,
      "  </modDependencies>",
      "</ModMetaData>",
      "",
    ].join("\n");
  }

  function patchFileFor(defType: string): string {
    const operations = mergeModEntrySources()
      .filter(
        ({ fixture, state }) => fixture.defKey.defType === defType && state.kind === "complete",
      )
      .flatMap(({ fixture }) =>
        fixture.fields
          .filter((f) => f.diffClass !== "unchanged")
          .map(
            (f) =>
              `      <li Class="PatchOperationReplace"><xpath>Defs/${fixture.defKey.defType}[defName="${fixture.defKey.defName}"]/${f.path}</xpath></li>`,
          ),
      );
    return [
      '<?xml version="1.0" encoding="utf-8"?>',
      "<Patch>",
      '  <Operation Class="PatchOperationSequence">',
      "    <operations>",
      ...operations,
      "    </operations>",
      "  </Operation>",
      "</Patch>",
      "",
    ].join("\n");
  }

  function rimmergeJson(): string {
    return JSON.stringify(
      {
        generatedAt: "2026-09-05T00:00:00Z",
        profileHash: "3f9a1c2b7d5e",
        decisionsSha256: "0".repeat(64),
        rimmergeVersion: "1.0.0",
      },
      null,
      2,
    );
  }

  // --- Compat patch fixtures -------------------------------------------
  //
  // A small set of independently-stateful patch projects: each has its
  // own scope (mod ids from the generic pool) and its own decisions map,
  // mirroring `rim_resolve::domain::PatchProject`'s independence from the
  // profile's own decisions closely enough for the mock tier. Membership
  // uses the pair/owner-set rule every `FindingEntry` this scenario
  // builds actually needs (>= 2 of a finding's `modIds` in scope; any
  // others are "outside") — not the full per-`FindingKey`-variant table,
  // since only `defOverride`/`patchCollision`/pair-shaped findings are
  // ever exercised through the UI this backs.

  interface PatchProjectState {
    id: string;
    name: string;
    packageId: string;
    displayName: string;
    author: string;
    description: string;
    scope: string[];
    decisions: Map<string, { action: ActionDto; note: string | null }>;
    /**
     * A patch's own merge-editor choices, keyed by finding key then field
     * path — independent of the profile's own `mergeChoicesByKey` (a
     * module-level map further up), exactly as `rim_resolve::domain::
     * PatchProject`'s own decisions never share storage with the
     * profile's `DecisionSet`.
     * `get_merge_preview`/`set_merge_choices` read and write this map
     * whenever `request.patchId` is set, never `mergeChoicesByKey`.
     */
    mergeChoices: Map<string, Record<string, MergeChoiceDto | undefined>>;
    exportDir: string | null;
    createdAt: string;
    updatedAt: string;
  }

  const patches = new Map<string, PatchProjectState>();
  let nextPatchId = 1;

  function newPatchId(): string {
    const id = nextPatchId.toString(16).padStart(12, "0");
    nextPatchId += 1;
    return id;
  }

  function requirePatch(patchId: string): PatchProjectState {
    const patch = patches.get(patchId);
    if (!patch) {
      throw { code: "patch_not_found", message: `no patch project with id ${patchId}` };
    }
    return patch;
  }

  type ScopeMembershipDto = { kind: "full" } | { kind: "partial"; outside: string[] };

  /**
   * `null` (`Outside`, per `PatchScope::membership`) when fewer than two
   * of `modIds` are scope members. `CORE_MOD_ID` is never counted as
   * "outside" (Core is always the implicit base of any override it
   * owns) — it can still count toward
   * `insideCount` when it's actually in `scope`, but its absence from
   * `scope` never turns a would-be `Full` membership into `Partial`.
   * `CORE_MOD_ID` is declared further down this same closure, referenced
   * here safely: this function's body only ever runs once an IPC command
   * fires, long after every top-level `const` in `installScenario` has
   * initialized.
   */
  function scopeMembership(scope: string[], modIds: string[]): ScopeMembershipDto | null {
    const insideCount = modIds.filter((id) => scope.includes(id)).length;
    if (insideCount < 2) return null;
    const outside = modIds.filter((id) => !scope.includes(id) && id !== CORE_MOD_ID);
    return outside.length === 0 ? { kind: "full" } : { kind: "partial", outside };
  }

  /**
   * A scope member's display name for `PatchSummaryDto`/`PatchDetailDto`'s
   * `ModRefDto` rows. Delegates to `allModNames()` (not just the generic
   * `mods` pool) so a scope built from a hand-crafted `mergeFixtures`
   * owner (`ludeon.rimworld`, `example.bionicsfork`, ...) shows a real name in
   * `ScopeEditor`'s chips instead of falling back to the bare id.
   */
  function nameForModId(id: string): string {
    return allModNames()[id] ?? id;
  }

  /**
   * This fixture universe's implicit "Core" mod id for merge previews —
   * every `mergeFixtures` def-override's base owner. Always a diff
   * participant regardless of scope, mirroring `PatchScope`'s "Core is
   * an implicit base" rule —
   * without it a two-mod scope would degrade to a two-way diff that
   * attributes every vanilla-equal field to the winner.
   */
  const CORE_MOD_ID = "ludeon.rimworld";

  /**
   * `fixture`'s owners restricted to a patch's own scope plus the
   * implicit Core (the `owners ∩ (scope ∪ {Core})` rule) — the
   * profile (`patch === null`) keeps every owner unrestricted.
   */
  function scopedMergeOwners(
    fixture: MergeFixture,
    patch: PatchProjectState | null,
  ): MergeOwnerDto[] {
    if (!patch) return fixture.owners;
    return fixture.owners.filter(
      (owner) => owner.modId === CORE_MOD_ID || patch.scope.includes(owner.modId),
    );
  }

  /** The owners `scopedMergeOwners` excluded — `MergePreviewDto.outOfScopeOwners`, always `[]` for the profile. */
  function outOfScopeMergeOwnerIds(
    fixture: MergeFixture,
    patch: PatchProjectState | null,
  ): string[] {
    if (!patch) return [];
    const scoped = new Set(scopedMergeOwners(fixture, patch).map((owner) => owner.modId));
    return fixture.owners.filter((owner) => !scoped.has(owner.modId)).map((owner) => owner.modId);
  }

  /**
   * One field's diff restricted to `scopedIds`: drops an out-of-scope
   * contributor from `changedBy`/`candidates` and downgrades `diffClass`
   * accordingly (`conflict`/`agreeing` -> `oneSided` with one contributor
   * left, or `unchanged` with none) — the "diff participants"
   * rule applied field by field. A no-op for an already-`unchanged` field
   * or one whose contributors are all in scope.
   */
  function restrictFieldToScope(
    field: MergeFixtureField,
    scopedIds: Set<string>,
  ): MergeFixtureField {
    if (field.diffClass === "unchanged") return field;
    const changedBy = field.changedBy.filter((id) => scopedIds.has(id));
    if (changedBy.length === field.changedBy.length) return field;
    const candidates = Object.fromEntries(
      Object.entries(field.candidates).filter(([id]) => scopedIds.has(id)),
    );
    const diffClass: DiffClassDto =
      changedBy.length === 0 ? "unchanged" : changedBy.length === 1 ? "oneSided" : field.diffClass;
    return { ...field, changedBy, candidates, diffClass };
  }

  /** `fixture` with its fields restricted to `patch`'s own scope (see {@link restrictFieldToScope}) — `fixture` itself, unmodified, for the profile. */
  function scopedMergeFixture(
    fixture: MergeFixture,
    patch: PatchProjectState | null,
  ): MergeFixture {
    if (!patch) return fixture;
    const scopedIds = new Set(scopedMergeOwners(fixture, patch).map((owner) => owner.modId));
    return {
      ...fixture,
      fields: fixture.fields.map((field) => restrictFieldToScope(field, scopedIds)),
    };
  }

  /** Base (earliest) and winner (latest) among `owners`, by selected-order `position`. Falls back to `fixture`'s own values when `owners` is empty. */
  function baseAndWinnerOf(
    fixture: MergeFixture,
    owners: MergeOwnerDto[],
  ): { base: string; winner: string } {
    if (owners.length === 0) return { base: fixture.base, winner: fixture.winner };
    const byPosition = [...owners].sort((a, b) => a.position - b.position);
    return {
      base: byPosition[0]?.modId ?? fixture.base,
      winner: byPosition.at(-1)?.modId ?? fixture.winner,
    };
  }

  /** A patch's own choices for `key` — never `mergeChoicesByKey`, the profile's. */
  function patchMergeChoicesFor(
    patch: PatchProjectState,
    key: string,
  ): Record<string, MergeChoiceDto | undefined> {
    return patch.mergeChoices.get(key) ?? {};
  }

  /** A patch's own merge state for one of its findings — `null` when the finding isn't mergeable at all. */
  function patchMergeStateFor(patch: PatchProjectState, entry: FindingEntry): MergeStateDto | null {
    const fixture = mergeFixtures.get(entry.key);
    if (!fixture) return null;
    return computeMergeState(
      scopedMergeFixture(fixture, patch),
      patchMergeChoicesFor(patch, entry.key),
    );
  }

  /** Every live finding this patch's scope admits (mirrors `ledger::scoped`'s own filter). */
  function scopedEntries(
    patch: PatchProjectState,
  ): { entry: FindingEntry; membership: ScopeMembershipDto }[] {
    const out: { entry: FindingEntry; membership: ScopeMembershipDto }[] = [];
    for (const entry of findings.values()) {
      const membership = scopeMembership(patch.scope, entry.modIds);
      if (membership) out.push({ entry, membership });
    }
    return out;
  }

  /**
   * Why a scoped entry's suggestion is always `Ignore` absent a patch
   * decision of its own — mirrors `rim_resolve::ledger::scoped`'s own
   * `NOT_ADDRESSED_RATIONALE`: a
   * compat patch never auto-applies anything; it only ever *ships*
   * `Merge`/`ShipAsset` or stays silent.
   */
  const NOT_ADDRESSED_RATIONALE = "Not addressed by this patch; load order decides as today.";

  /**
   * Rewrites a profile finding's suggestion into the patch's own
   * vocabulary, exactly as the real `ledger::scoped` does: `Ignore` at
   * the same confidence (so the scoped inbox's own ordering still
   * surfaces the least certain conflicts first), offering only the
   * `merge`/`shipAsset` alternatives a patch could actually publish.
   */
  function rewritePatchSuggestion(entry: FindingEntry): {
    action: ActionDto;
    confidence: number;
    rationale: string;
    rationaleCode: { kind: "notAddressedByPatch" };
    alternatives: {
      action: ActionDto;
      rationale: string;
      rationaleCode: typeof GENERIC_RATIONALE_CODE;
    }[];
  } {
    return {
      action: { kind: "ignore" },
      confidence: entry.confidence,
      rationale: NOT_ADDRESSED_RATIONALE,
      rationaleCode: { kind: "notAddressedByPatch" },
      alternatives: entry.alternatives
        .filter(
          (alternative) =>
            alternative.action.kind === "merge" || alternative.action.kind === "shipAsset",
        )
        .map((alternative) => ({
          ...alternative,
          rationaleCode: GENERIC_RATIONALE_CODE,
        })),
    };
  }

  /** A kept entry's status, derived from the patch's own decision alone — never the profile's. */
  function patchStatusFor(patch: PatchProjectState, entry: FindingEntry): ResolutionStatusDto {
    if (patch.decisions.has(entry.key)) return "userOverridden";
    return entry.confidence >= settings.threshold ? "auto" : "needsInput";
  }

  function patchLedgerStats(patch: PatchProjectState) {
    let auto = 0;
    let overridden = 0;
    let needsInput = 0;
    for (const { entry } of scopedEntries(patch)) {
      const status = patchStatusFor(patch, entry);
      if (status === "userOverridden") overridden += 1;
      else if (status === "auto") auto += 1;
      else needsInput += 1;
    }
    return { auto, needsInput, overridden, resolvedBySuggested: 0 };
  }

  /**
   * Keys of `patch`'s own decisions that `scope` (default: the patch's
   * *current* scope) no longer admits — kept on file, never deleted.
   * Takes an explicit `scope` so `update_patch` can compute this against
   * the *proposed* new scope, before `patch.scope` is actually mutated,
   * without duplicating this filter inline.
   */
  function orphanedKeysOf(patch: PatchProjectState, scope: string[] = patch.scope): string[] {
    return [...patch.decisions.keys()].filter((key) => {
      const entry = findings.get(key);
      return scopeMembership(scope, entry?.modIds ?? []) === null;
    });
  }

  function patchSummaryOf(patch: PatchProjectState) {
    const stats = patchLedgerStats(patch);
    return {
      id: patch.id,
      name: patch.name,
      packageId: patch.packageId,
      displayName: patch.displayName,
      scope: patch.scope.map((id) => ({ modId: id, name: nameForModId(id) })),
      decisionCount: patch.decisions.size,
      completeCount: stats.auto + stats.overridden,
      needsInputCount: stats.needsInput,
      exportDir: patch.exportDir,
      updatedAt: patch.updatedAt,
    };
  }

  function patchDetailOf(patch: PatchProjectState) {
    return {
      ...patchSummaryOf(patch),
      author: patch.author,
      description: patch.description,
      folderName: patch.packageId.replaceAll(".", "_"),
      orphaned: orphanedKeysOf(patch),
      stats: patchLedgerStats(patch),
    };
  }

  function patchFindingSummary(
    patch: PatchProjectState,
    entry: FindingEntry,
    membership: ScopeMembershipDto,
  ) {
    const decision = patch.decisions.get(entry.key);
    const suggestion = rewritePatchSuggestion(entry);
    return {
      key: entry.key,
      finding: entry.finding,
      status: patchStatusFor(patch, entry),
      confidence: entry.confidence,
      effective: decision ? decision.action : suggestion.action,
      hasDecision: decision !== undefined,
      mergeState: patchMergeStateFor(patch, entry),
      scope: membership,
    };
  }

  function patchFindingDetail(
    patch: PatchProjectState,
    entry: FindingEntry,
    membership: ScopeMembershipDto,
  ) {
    const decision = patch.decisions.get(entry.key);
    const suggestion = rewritePatchSuggestion(entry);
    return {
      key: entry.key,
      finding: entry.finding,
      suggestion,
      status: patchStatusFor(patch, entry),
      effective: decision ? decision.action : suggestion.action,
      note: decision?.note ?? null,
      hasDecision: decision !== undefined,
      resolvedBySuggested: null,
      mergeState: patchMergeStateFor(patch, entry),
      scope: membership,
    };
  }

  // -- def inspector mock --------------------
  // Derived straight from `mergeFixtures`/`findings` rather than a second,
  // parallel `SourceIndex` model: every fixture already carries real
  // owner/field/resolved-XML data `get_merge_preview`/`ChangeSummary`
  // already show for the same defs, so `inspect_def`/`list_mod_changes`/
  // `search_defs` reading the same source keeps every view of one def
  // consistent. A `patchCollision`-kind fixture's own `owners` field is
  // its *patchers* (see `registerMergeFixture`'s own call sites) — the
  // def's real, single owner is `base` — while a `defOverride`-kind
  // fixture's `owners` are genuine owners and it has no patchers at all;
  // this mock never invents patch data `get_merge_preview` doesn't
  // already carry.

  function defRefOf(fixture: MergeFixture): string {
    return `${fixture.defKey.defType}/${fixture.defKey.defName}`;
  }

  function fixtureByDefRef(defRef: string): MergeFixture | null {
    for (const fixture of mergeFixtures.values()) {
      if (defRefOf(fixture) === defRef) return fixture;
    }
    return null;
  }

  // ---- def graphics (`resolve_def_graphic` / `read_def_texture`) ----------
  //
  // Mirrors the backend's semantics, not just its shapes: a def the scan
  // never indexed is `def_not_found`; an indexed def with no graphic shows
  // nothing; `read_def_texture` re-resolves the def and refuses (as
  // `invalid_input`) any key its graphic did not produce, however
  // well-formed; a malformed key is refused before the def is even looked
  // at. No response ever carries a path.

  type FaceFacing = "north" | "east" | "south" | "west";

  function graphicFace(
    textureKey: string,
    availability: FaceAvailabilityDto,
    facing: FaceFacing | null = null,
    isMirrored = false,
  ): GraphicFaceDto {
    return { textureKey, facing, isMirrored, availability };
  }

  function loose(owner: string): FaceAvailabilityDto {
    return { kind: "loose", owner };
  }

  function facesLocated(faces: GraphicFacesDto): boolean {
    const all =
      faces.kind === "single" ? [faces.face] : [faces.north, faces.east, faces.south, faces.west];
    return all.some((face) => face.availability.kind === "loose");
  }

  function graphicVariant(label: VariantLabelDto, faces: GraphicFacesDto): GraphicVariantDto {
    return { label, faces, isLocated: facesLocated(faces) };
  }

  function singleFaces(face: GraphicFaceDto): GraphicFacesDto {
    return { kind: "single", face };
  }

  function graphicSlot(
    source: SlotSourceDto,
    graphicClass: string | null,
    variants: GraphicVariantDto[],
    defaultVariant = 0,
    truncated = 0,
  ): GraphicSlotDto {
    return {
      source,
      graphicClass,
      isLayoutInferred: false,
      variants,
      defaultVariant,
      truncated,
      isLocated: variants.some((variant) => variant.isLocated),
    };
  }

  function resolvedGraphic(slots: GraphicSlotDto[], defaultSlot = 0): DefGraphicDto {
    const slot = slots[defaultSlot];
    return {
      kind: "resolved",
      slots,
      defaultView: { slot: defaultSlot, variant: slot?.defaultVariant ?? 0 },
      truncatedSlots: 0,
    };
  }

  /** Defs that show something, by canonical ref. A def that is indexed (a merge fixture) but absent here shows nothing. */
  const DEF_GRAPHICS = new Map<string, DefGraphicDto>([
    [
      "ThingDef/MockStatue",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/statue", loose("mod.002"))),
          ),
        ]),
      ]),
    ],
    [
      "ThingDef/MockChair",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Multi", [
          graphicVariant(
            { kind: "only" },
            {
              kind: "multi",
              north: graphicFace("mock/chair_north", loose("mod.002"), "north"),
              east: graphicFace("mock/chair_east", loose("mod.002"), "east"),
              south: graphicFace("mock/chair_south", loose("mod.002"), "south"),
              west: graphicFace("mock/chair_east", loose("mod.002"), "west", true),
            },
          ),
        ]),
      ]),
    ],
    [
      "ThingDef/MockCloak",
      resolvedGraphic(
        [
          graphicSlot({ kind: "worn" }, "Graphic_Multi", [
            graphicVariant(
              { kind: "bodyType", name: "Thin" },
              singleFaces(graphicFace("mock/cloak_thin", loose("mod.003"))),
            ),
            graphicVariant(
              { kind: "bodyType", name: "Fat" },
              singleFaces(graphicFace("mock/cloak_fat", { kind: "notFound" })),
            ),
          ]),
          graphicSlot({ kind: "icon" }, null, [
            graphicVariant(
              { kind: "field", name: "uiIconPath" },
              singleFaces(graphicFace("mock/cloak_icon", { kind: "baseGameOrBundle" })),
            ),
          ]),
        ],
        0,
      ),
    ],
    [
      "ThingDef/MockDdsThing",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Collection", [
          graphicVariant(
            { kind: "member", index: 0, count: 4, name: "a" },
            singleFaces(graphicFace("mock/sibling", loose("mod.004"))),
          ),
          graphicVariant(
            { kind: "member", index: 1, count: 4, name: "b" },
            singleFaces(graphicFace("mock/undecodable", loose("mod.004"))),
          ),
          graphicVariant(
            { kind: "member", index: 2, count: 4, name: "c" },
            singleFaces(graphicFace("mock/dds", loose("mod.004"))),
          ),
          graphicVariant(
            { kind: "member", index: 3, count: 4, name: "d" },
            singleFaces(graphicFace("mock/huge", loose("mod.004"))),
          ),
        ]),
      ]),
    ],
    [
      "ThingDef/MockUnseen",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/maybe_builtin", { kind: "unknown" })),
          ),
        ]),
      ]),
    ],
    ["PawnKindDef/MockColonist", { kind: "composedAtRuntime" }],
    // The patch maker's coverage targets (`ThingDef/Race0` ... `Race4`), one
    // of each outcome the queue thumbnails and the row editor viewer show.
    [
      "ThingDef/Race0",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Multi", [
          graphicVariant(
            { kind: "only" },
            {
              kind: "multi",
              north: graphicFace("mock/race0_north", loose("fixture.target"), "north"),
              east: graphicFace("mock/race0_east", loose("fixture.target"), "east"),
              south: graphicFace("mock/race0_south", loose("fixture.target"), "south"),
              west: graphicFace("mock/race0_east", loose("fixture.target"), "west", true),
            },
          ),
        ]),
        graphicSlot({ kind: "icon" }, null, [
          graphicVariant(
            { kind: "field", name: "uiIconPath" },
            singleFaces(graphicFace("mock/race0_icon", loose("fixture.target"))),
          ),
        ]),
      ]),
    ],
    [
      "ThingDef/Race1",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/race1", loose("mod.002"))),
          ),
        ]),
      ]),
    ],
    [
      "ThingDef/Race2",
      resolvedGraphic([
        graphicSlot({ kind: "worn" }, "Graphic_Multi", [
          graphicVariant(
            { kind: "bodyType", name: "Thin" },
            singleFaces(graphicFace("mock/race2_bundle", { kind: "baseGameOrBundle" })),
          ),
          graphicVariant(
            { kind: "bodyType", name: "Fat" },
            singleFaces(graphicFace("mock/race2_fat", loose("fixture.target"))),
          ),
          graphicVariant(
            { kind: "bodyType", name: "Hulk" },
            singleFaces(graphicFace("mock/race2_hulk", { kind: "notFound" })),
          ),
        ]),
      ]),
    ],
    ["ThingDef/Race3", { kind: "noGraphic" }],
    // The patch maker's item picker entries (`example.PartDef`).
    [
      "example.PartDef/Part0",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/part0", loose("fixture.parts"))),
          ),
        ]),
      ]),
    ],
    [
      "example.PartDef/Part1",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/part1", loose("fixture.parts"))),
          ),
        ]),
      ]),
    ],
    [
      "example.PartDef/Part2",
      resolvedGraphic([
        graphicSlot({ kind: "graphic" }, "Graphic_Single", [
          graphicVariant(
            { kind: "only" },
            singleFaces(graphicFace("mock/part2_bundle", { kind: "baseGameOrBundle" })),
          ),
        ]),
      ]),
    ],
    ["example.PartDef/Part3", { kind: "noGraphic" }],
    ["example.PartDef/Part4", { kind: "composedAtRuntime" }],
    ["ThingDef/Race4", { kind: "composedAtRuntime" }],
  ]);

  const spriteCache = new Map<string, string>();

  /**
   * A small generated sprite for a texture key, so a screenshot or a spec can
   * tell textures (and directions) apart: a colour from the key's hash and,
   * when the key ends in a direction, an arrow pointing that way.
   */
  function spriteDataUrl(key: string): string {
    const cached = spriteCache.get(key);
    if (cached) return cached;
    const canvas = document.createElement("canvas");
    canvas.width = 64;
    canvas.height = 64;
    const context = canvas.getContext("2d");
    if (!context) return ONE_PIXEL_PNG_DATA_URL;
    let hash = 0;
    for (const character of key) hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
    context.fillStyle = `hsl(${hash % 360} 55% 52%)`;
    context.fillRect(10, 10, 44, 44);
    context.fillStyle = "#ffffff";
    context.beginPath();
    const direction = /_(north|east|south|west)$/.exec(key)?.[1];
    if (direction === "north") {
      context.moveTo(32, 16);
      context.lineTo(46, 40);
      context.lineTo(18, 40);
    } else if (direction === "south") {
      context.moveTo(32, 48);
      context.lineTo(46, 24);
      context.lineTo(18, 24);
    } else if (direction === "east") {
      context.moveTo(48, 32);
      context.lineTo(24, 18);
      context.lineTo(24, 46);
    } else if (direction === "west") {
      context.moveTo(16, 32);
      context.lineTo(40, 18);
      context.lineTo(40, 46);
    } else {
      context.arc(32, 32, 12, 0, Math.PI * 2);
    }
    context.fill();
    const url = canvas.toDataURL("image/png");
    spriteCache.set(key, url);
    return url;
  }

  /** What a loose file reads back as, by key, when it isn't a plain image: the engine-visible cases the viewer must word differently. */
  const DEF_TEXTURE_OUTCOMES = new Map<string, (owner: string) => DefTextureDto>([
    [
      "mock/sibling",
      (owner) => ({
        kind: "image",
        dataUrl: spriteDataUrl("mock/sibling"),
        format: "png",
        bytes: 68,
        owner,
        from: "pngSibling",
      }),
    ],
    ["mock/undecodable", (owner) => ({ kind: "undecodableInGame", owner })],
    ["mock/dds", (owner) => ({ kind: "ddsNotPreviewable", owner })],
    ["mock/huge", () => ({ kind: "unreadable", reason: "tooLarge" })],
  ]);

  /** Mirrors `TextureKey::parse`: normalizes, then refuses what could climb out of `Textures/` or isn't a path. */
  function parseTextureKey(raw: string): string {
    const key = raw.replaceAll("\\", "/").toLowerCase();
    const refusal = (reason: string) => ({
      code: "invalid_input",
      message: `${reason}`,
    });
    if (key.length === 0) throw refusal("texture key is empty");
    if (key.length > 256) throw refusal(`texture key is ${key.length} bytes long`);
    if (key.startsWith("/")) throw refusal("texture key starts with a slash");
    if (key.includes("{") || key.includes("[")) {
      throw refusal("texture key looks like a format string");
    }
    for (const segment of key.split("/")) {
      if (segment === "") throw refusal("texture key has an empty segment");
      if (segment === "." || segment === "..") throw refusal("texture key has a relative segment");
    }
    return key;
  }

  /**
   * Texture keys that more than one mod ships (the `mod.010`/`mod.015` pair
   * `suggestedOrder` swaps, as in the texture-override finding): the file the
   * game loads, and so the face's owner, is the last-loaded shipper's under the
   * *selected* order. Every other face keeps the single owner its entry names.
   */
  const FACE_SHIPPERS = new Map<string, string[]>([["mock/race0_south", ["mod.010", "mod.015"]]]);

  function mapFaces(
    graphic: DefGraphicDto,
    change: (face: GraphicFaceDto) => GraphicFaceDto,
  ): DefGraphicDto {
    if (graphic.kind !== "resolved") return graphic;
    return {
      ...graphic,
      slots: graphic.slots.map((slot) => ({
        ...slot,
        variants: slot.variants.map((variant) => ({
          ...variant,
          faces:
            variant.faces.kind === "single"
              ? { kind: "single" as const, face: change(variant.faces.face) }
              : {
                  ...variant.faces,
                  north: change(variant.faces.north),
                  east: change(variant.faces.east),
                  south: change(variant.faces.south),
                  west: change(variant.faces.west),
                },
        })),
      })),
    };
  }

  function shippedUnderSelectedOrder(graphic: DefGraphicDto): DefGraphicDto {
    const order = orderFor(selected);
    return mapFaces(graphic, (face) => {
      const shippers = FACE_SHIPPERS.get(face.textureKey);
      if (!shippers) return face;
      return { ...face, availability: loose(lastLoaded(shippers, order, "")) };
    });
  }

  function resolveMockGraphic(defRef: string): DefGraphicDto {
    if (!/^[A-Za-z0-9_.]+\/\S.*$/.test(defRef)) {
      throw { code: "invalid_input", message: `${defRef} is not a def reference` };
    }
    const graphic = DEF_GRAPHICS.get(defRef);
    if (graphic) return shippedUnderSelectedOrder(graphic);
    if (fixtureByDefRef(defRef)) return { kind: "noGraphic" };
    throw {
      code: "def_not_found",
      message: `${defRef} is not indexed as a def or template`,
    };
  }

  function allFaces(graphic: DefGraphicDto): GraphicFaceDto[] {
    if (graphic.kind !== "resolved") return [];
    return graphic.slots.flatMap((slot) =>
      slot.variants.flatMap((variant) =>
        variant.faces.kind === "single"
          ? [variant.faces.face]
          : [variant.faces.north, variant.faces.east, variant.faces.south, variant.faces.west],
      ),
    );
  }

  function readMockDefTexture(request: ReadDefTextureRequestDto): DefTextureDto {
    const key = parseTextureKey(request.textureKey);
    const face = allFaces(resolveMockGraphic(request.defRef)).find(
      (candidate) => candidate.textureKey === key,
    );
    if (!face) {
      throw {
        code: "invalid_input",
        message: `${key} is not a texture of this def's graphic`,
      };
    }
    switch (face.availability.kind) {
      case "loose": {
        const owner = face.availability.owner;
        const outcome = DEF_TEXTURE_OUTCOMES.get(key);
        if (outcome) return outcome(owner);
        return {
          kind: "image",
          dataUrl: spriteDataUrl(key),
          format: "png",
          bytes: 68,
          owner,
          from: "direct",
        };
      }
      case "baseGameOrBundle":
        return { kind: "notViewable", isUncertain: false };
      case "notFound":
        return { kind: "notFound" };
      case "unknown":
        return { kind: "notViewable", isUncertain: true };
    }
  }

  /** The mod whose value `field` resolves to under the real game's pipeline (last contributor wins) — ignores any stored merge choice, unlike `mergeFieldResult`, since the effective def is the game's own result, not rimmerge's proposal. */
  function effectiveFieldOwner(fixture: MergeFixture, field: MergeFixtureField): string {
    return field.changedBy.at(-1) ?? fixture.base;
  }

  function effectiveFieldValue(fixture: MergeFixture, field: MergeFixtureField): string | null {
    if (field.diffClass === "unchanged") return field.base;
    const by = effectiveFieldOwner(fixture, field);
    return field.candidates[by] ?? null;
  }

  /** One field's provenance pill — `Owner` for a `defOverride` fixture's fields (no patch ops modeled there) and unchanged fields anywhere, `Patch` for a `patchCollision` fixture's changed fields. */
  function effectiveFieldProvenance(fixture: MergeFixture, field: MergeFixtureField) {
    if (field.diffClass === "unchanged" || fixture.kind === "defOverride") {
      return { kind: "owner" as const, modId: effectiveFieldOwner(fixture, field) };
    }
    return { kind: "patch" as const, modId: effectiveFieldOwner(fixture, field), opIndex: 0 };
  }

  /** Mirrors `buildResolvedXml`'s shape, but unconditionally (the effective def always has a rendering, unlike a merge preview's `resolvedXml`, which is only ever present for a `Complete` `defOverride`). */
  function buildEffectiveXml(fixture: MergeFixture): string {
    const topLevel: string[] = [];
    const containers = new Map<string, string[]>();
    for (const field of fixture.fields) {
      const result = effectiveFieldValue(fixture, field);
      if (result === null) continue;
      const segments = field.path.split("/");
      const tag = segments[0];
      if (tag === undefined) continue;
      if (segments.length === 1) {
        topLevel.push(`  <${tag}>${escapeXmlText(result)}</${tag}>`);
        continue;
      }
      const items = containers.get(tag) ?? [];
      items.push(`    ${result}`);
      containers.set(tag, items);
    }
    const containerBlocks = Array.from(containers.entries(), ([tag, items]) =>
      [`  <${tag}>`, ...items, `  </${tag}>`].join("\n"),
    );
    return [
      `<${fixture.defKey.defType}>`,
      ...topLevel,
      ...containerBlocks,
      `</${fixture.defKey.defType}>`,
    ].join("\n");
  }

  function defFindingsFor(fixture: MergeFixture) {
    const entry = findings.get(fixture.key);
    return entry ? [{ key: entry.key, status: entry.status }] : [];
  }

  interface DefChangeRow {
    kind: "ownsDef" | "patchesDef";
    defRef: string;
    assetPath: null;
    otherTouchers: number;
    opCount: number;
    findingKeys: string[];
  }

  /** Every `mergeFixtures` row naming `modId` as an owner or patcher, shaped like `Session::changes`' own rows. */
  function defChangesFor(modId: string): DefChangeRow[] {
    const rows: DefChangeRow[] = [];
    for (const fixture of mergeFixtures.values()) {
      const findingKeys = defFindingsFor(fixture).map((f) => f.key);
      if (fixture.kind === "defOverride") {
        if (!fixture.owners.some((o) => o.modId === modId)) continue;
        rows.push({
          kind: "ownsDef",
          defRef: defRefOf(fixture),
          assetPath: null,
          otherTouchers: fixture.owners.length - 1,
          opCount: 0,
          findingKeys,
        });
        continue;
      }
      // `patchCollision`: `base` is the def's real (sole) owner, every
      // `owners` entry a patcher — see this section's own doc comment.
      if (fixture.base === modId) {
        rows.push({
          kind: "ownsDef",
          defRef: defRefOf(fixture),
          assetPath: null,
          otherTouchers: fixture.owners.length,
          opCount: 0,
          findingKeys,
        });
      } else if (fixture.owners.some((o) => o.modId === modId)) {
        rows.push({
          kind: "patchesDef",
          defRef: defRefOf(fixture),
          assetPath: null,
          otherTouchers: fixture.owners.length,
          opCount: 1,
          findingKeys,
        });
      }
    }
    return rows;
  }

  // -- `get_def_conflict_view` mock --
  // Derived from the same `mergeFixtures` the merge editor and def
  // inspector mocks already read (see that section's own doc comment
  // above) — never a third, parallel model of the same def.

  /** A fixture's own touchers table: `fixture.touchers` when given, else the same owner/patcher split `inspect_def` derives. */
  function defConflictTouchers(fixture: MergeFixture) {
    if (fixture.touchers) return fixture.touchers;
    if (fixture.kind === "defOverride") {
      return fixture.owners.map((o) => ({
        modId: o.modId,
        position: o.position,
        isGenerated: false,
        role: "owner" as const,
        opCount: 0,
      }));
    }
    return [
      { modId: fixture.base, position: 0, isGenerated: false, role: "owner" as const, opCount: 0 },
      ...fixture.owners.map((o) => ({
        modId: o.modId,
        position: o.position + 1,
        isGenerated: false,
        role: "patcher" as const,
        opCount: 1,
      })),
    ];
  }

  /** One row's kind, mirroring `def_conflict_view::classify_row`. */
  function defConflictRowKind(
    field: MergeFixtureField,
  ): "conflict" | "cleanMerge" | "listEntry" | "mapEntry" | "unchanged" {
    if (field.diffClass === "conflict") return "conflict";
    if (field.diffClass === "unchanged") return "unchanged";
    if (field.container !== undefined) return "mapEntry";
    return field.isListItem ? "listEntry" : "cleanMerge";
  }

  /** Whether the fold reached `path` at all — `false` for every path `fixture.unsupportedOp` names as never reached. */
  function defConflictReachable(fixture: MergeFixture, path: string): boolean {
    return !(fixture.unsupportedOp?.unreachedPaths.includes(path) ?? false);
  }

  /** Which mod a complete merge would attribute `field`'s result to, mirroring `def_conflict_view::after_merge_mod`. */
  function defConflictAfterMergeOwner(
    field: MergeFixtureField,
    choice: MergeChoiceDto | undefined,
    winnerId: string,
    diffBaseId: string,
  ): string | null {
    if (choice) {
      if (choice.choice === "from") return choice.modId;
      if (choice.choice === "value") return winnerId;
      return null; // drop
    }
    switch (field.diffClass) {
      case "unchanged":
        return diffBaseId;
      case "oneSided":
      case "agreeing":
        return field.changedBy[0] ?? null;
      case "conflict":
        return null;
    }
  }

  function get_def_conflict_view(payload: unknown) {
    const request = (payload as { request: DefConflictViewRequestDto }).request;
    const fixture = mergeFixtures.get(request.key);
    if (!fixture) {
      // `duplicateTemplateName` has no `MergePreview` at all (`PlanMerge`
      // never supports it), so it never gets a `mergeFixtures` entry —
      // touchers/problems only, straight off the finding's own `owners`,
      // mirroring `rim_session::test_support::duplicate_template_name_fixture`.
      const duplicateFinding = findings.get(request.key);
      if (duplicateFinding && duplicateFinding.finding.kind === "duplicateTemplateName") {
        const { name, owners } = duplicateFinding.finding;
        return {
          defRef: `@${name}`,
          kind: { kind: "duplicateTemplateName" as const },
          touchers: owners.map((modId, position) => ({
            modId,
            position,
            isGenerated: false,
            role: "owner" as const,
            opCount: 0,
          })),
          fields: [],
          fieldsTotal: 0,
          problems: [],
          effectiveCompleteness: { kind: "complete" as const },
          injectedNodeRelations: [],
        };
      }
      throw {
        code: "invalid_input",
        message: `${request.key} is not a def-shaped finding in this fixture`,
      };
    }
    const isPatchCollision = fixture.kind === "patchCollision";
    // Against a compat patch's own scope and decisions when
    // `request.patchId` is set, the profile's own otherwise — mirrors
    // `get_merge_preview`'s own `patchId` handling exactly. Unlike the
    // merge preview's own DTO, this
    // one's `touchers`/`problems`/`effectiveCompleteness` stay unscoped
    // regardless — they come from `InspectDef` on the real backend, which
    // has no patch-scoping concept at all (only `PlanMerge`'s own diff
    // does): only `values`/`kind`/`afterMerge`/`preference` (all
    // diff-derived) and `inGame` (always the real, unscoped fold's own
    // result, on both the real backend and here) are computed below.
    const patch = request.patchId ? requirePatch(request.patchId) : null;
    const scopedFixture = scopedMergeFixture(fixture, patch);
    const { base: diffBaseId, winner: diffWinnerId } = patch
      ? baseAndWinnerOf(fixture, scopedMergeOwners(fixture, patch))
      : isPatchCollision
        ? baseAndWinnerOf(fixture, fixture.owners)
        : { base: fixture.base, winner: fixture.winner };
    const choices = patch
      ? patchMergeChoicesFor(patch, request.key)
      : (mergeChoicesByKey.get(request.key) ?? {});
    const state = computeMergeState(scopedFixture, choices);
    const unsupportedOp = fixture.unsupportedOp ?? null;

    const fields = fixture.fields.map((field, index) => {
      // `scopedField` shares `field`'s own `path`/`isListItem` — only
      // `changedBy`/`candidates`/`diffClass` narrow under a patch's own
      // scope (`restrictFieldToScope`) — same length/order as
      // `fixture.fields`, so pairing by index is safe.
      const scopedField = scopedFixture.fields[index] ?? field;
      const kind = defConflictRowKind(scopedField);
      const reachable = defConflictReachable(fixture, field.path);
      // `inGame` is always the real, unscoped fold's own result — a
      // patch's own scope restricts which mods count as *diff
      // participants*, never which mods are actually active/patching.
      const inGame = reachable
        ? { modId: effectiveFieldOwner(fixture, field), value: effectiveFieldValue(fixture, field) }
        : null;
      const choice = choices[field.path];
      const result = reachable ? mergeFieldResult(scopedField, choice) : null;
      const afterMergeOwner =
        result !== null && state.kind === "complete"
          ? defConflictAfterMergeOwner(scopedField, choice, diffWinnerId, diffBaseId)
          : null;
      const afterMerge = afterMergeOwner ? { modId: afterMergeOwner, value: result } : null;
      // Precedence mirrors `def_conflict_view::preference_for`: a stored
      // merge choice wins over a `PreferWinner` decision, which wins over
      // plain load order. `decisionWinners` models only a *profile*-level
      // `PreferWinner` decision (a patch's own decisions never carry one
      // in this mock — only its own scoped merge choices do), so it never
      // applies once `patch` is set.
      const decisionWinner = patch ? undefined : fixture.decisionWinners?.[field.path];
      const preference =
        kind === "conflict"
          ? choice
            ? { kind: "mergeChoice" as const, choice }
            : decisionWinner
              ? { kind: "decision" as const, winner: decisionWinner }
              : { kind: "loadOrder" as const, winner: diffWinnerId }
          : { kind: "none" as const };
      return {
        path: field.path,
        kind,
        values: scopedField.changedBy.map((modId) => ({
          modId,
          value: scopedField.candidates[modId] ?? null,
        })),
        agreedBy: scopedField.agreedBy ?? [],
        inGame,
        afterMerge,
        preference,
      };
    });

    const matching = request.filter.onlyChanged
      ? fields.filter((f) => f.kind !== "unchanged")
      : fields;
    const fieldsTotal = matching.length;
    const limit = Math.min(request.filter.limit, 200);
    const page = matching.slice(request.filter.offset, request.filter.offset + limit);

    const problems = unsupportedOp
      ? [
          {
            kind: "unsupportedOp" as const,
            modId: unsupportedOp.modId,
            opIndex: unsupportedOp.opIndex,
            class: unsupportedOp.class,
            xpath: unsupportedOp.xpath,
            reason: unsupportedOp.reason,
          },
        ]
      : [];

    const finding = findings.get(request.key);
    const subPath =
      finding && finding.finding.kind === "patchCollision" ? finding.finding.subPath : null;

    return {
      defRef: defRefOf(fixture),
      kind: isPatchCollision
        ? { kind: "patchCollision" as const, subPath }
        : { kind: "defOverride" as const },
      touchers: defConflictTouchers(fixture),
      fields: page,
      fieldsTotal,
      problems,
      effectiveCompleteness: unsupportedOp
        ? {
            kind: "partial" as const,
            stoppedAt: {
              kind: "replay" as const,
              modId: unsupportedOp.modId,
              opIndex: unsupportedOp.opIndex,
              error: unsupportedOp.reason,
            },
          }
        : { kind: "complete" as const },
      // The mock fixture has no `rim_analyzer`-shaped edge data to derive
      // this from —
      // always empty here, real coverage is Vitest-only
      // (`DefConflictView.test.ts`), the same split this mock already
      // draws for other engine-only signals.
      injectedNodeRelations: [],
    };
  }

  // -----------------------------------------------------------------
  // The patch maker (assignments) — an independent project kind from
  // `patches` above (mirroring
  // `rim-session`'s own independence, `crate::assignment_refs`), built
  // entirely over `ASSIGNMENT_FIXTURE_MODS` — never `mergeFixtures`,
  // which this feature has no use for (an assignment project has no
  // `Merge`/`ShipAsset` decisions of its own).
  // -----------------------------------------------------------------

  interface AssignmentRowState {
    target: { keyField: string; def: { defType: string; defName: string } };
    row: { values: Record<string, unknown>; defName: string; note: string | null };
  }

  /** One section's own mutable state — mirrors `AssignmentSectionDto`, one per def type a project carries. */
  interface AssignmentSectionState {
    defType: string;
    schema: AssignmentSchemaDto;
    /** Whether this section has no `TargetKey` field — decided once, from the fixed fixture schema, exactly the way `Section::is_standalone()` decides it structurally rather than via a separate flag. */
    isStandalone: boolean;
    /** Target-keyed rows, by `rowKey(target)`. Always empty when `isStandalone`. */
    rows: Map<string, AssignmentRowState>;
    /** Free-standing rows, by their own `defName`. Always empty otherwise. */
    standaloneRows: Map<string, AssignmentRowState["row"]>;
  }

  interface AssignmentProjectState {
    id: string;
    name: string;
    packageId: string;
    displayName: string;
    refs: string[];
    excludedRefs: string[];
    targets: string[];
    exportDir: string | null;
    author: string;
    description: string;
    /** Every section, by def type — insertion order doesn't matter, `sortedSections` always reads them back in `BTreeMap` (lexicographic) order, matching the real backend. */
    sections: Map<string, AssignmentSectionState>;
    createdAt: string;
    updatedAt: string;
  }

  const assignments = new Map<string, AssignmentProjectState>();
  let nextAssignmentId = 1;

  function newAssignmentId(): string {
    const id = (0xa00000 + nextAssignmentId).toString(16).padStart(12, "0");
    nextAssignmentId += 1;
    return id;
  }

  function requireAssignment(assignmentId: string): AssignmentProjectState {
    const assignment = assignments.get(assignmentId);
    if (!assignment) {
      throw {
        code: "assignment_not_found",
        message: `no assignment project with id ${assignmentId}`,
      };
    }
    return assignment;
  }

  function rowKey(target: { def: { defType: string; defName: string } }): string {
    return `${target.def.defType}/${target.def.defName}`;
  }

  /** Every section, in the same `BTreeMap`-lexicographic order the real backend serializes `AssignmentDetailDto.sections` in. */
  function sortedSections(assignment: AssignmentProjectState): AssignmentSectionState[] {
    return [...assignment.sections.keys()]
      .sort()
      .map((defType) => assignment.sections.get(defType))
      .filter((section): section is AssignmentSectionState => section !== undefined);
  }

  function hasTargetKeyField(schema: AssignmentSchemaDto): boolean {
    return Object.values(schema.fields).some((spec) => spec?.role.type === "targetKey");
  }

  /**
   * Resolves which section a command addresses, mirroring `commands::assignments::resolve_section`:
   * `requested` names a def type explicitly; omitted falls back to the
   * project's own single section, erroring once it genuinely has more
   * than one — the mock replicates the fallback/refusal shape exactly,
   * even though it never needs to exercise the real inference behind it.
   */
  function resolveSection(
    assignment: AssignmentProjectState,
    requested: string | null,
  ): AssignmentSectionState {
    if (requested !== null) {
      const section = assignment.sections.get(requested);
      if (!section) {
        throw {
          code: "invalid_input",
          message: `no section for def type ${requested}`,
        };
      }
      return section;
    }
    const sections = sortedSections(assignment);
    if (sections.length === 0) {
      throw { code: "invalid_input", message: "this assignment project has no sections" };
    }
    if (sections.length > 1) {
      throw {
        code: "invalid_input",
        message: `this assignment project has ${sections.length} sections; specify a section`,
      };
    }
    const [only] = sections;
    if (!only) {
      throw { code: "invalid_input", message: "this assignment project has no sections" };
    }
    return only;
  }

  /**
   * Mirrors `rim_resolve::domain::AssignmentProject::set_row`/`clear_row`'s
   * own `RowKeyMismatch` guard: a `RowKey::Target` key against a
   * free-standing section, or a `RowKey::Own` key against a target-keyed
   * one, is refused — never silently accepted into the wrong map.
   */
  function requireRowKeyMatches(section: AssignmentSectionState, hasTarget: boolean): void {
    if (hasTarget === section.isStandalone) {
      throw {
        code: "invalid_input",
        message: `row key shape does not match section ${JSON.stringify(section.defType)}'s own kind (target-keyed vs. free-standing)`,
      };
    }
  }

  /** The fixed schema every `list_assignment_candidates`/`infer_assignment_candidate`/`add_assignment_section` call for this def type uses — mirrors `crates/rim-io/tests/fixtures/assign_game`'s own real inference result. `example.PartAssignmentDef` is target-keyed (owned by `fixture.framework`); `example.PartDef` is free-standing, no `TargetKey` field at all (owned by `fixture.parts`). */
  function assignmentSectionSchemaFor(defType: string): AssignmentSchemaDto | null {
    if (defType === "example.PartAssignmentDef") {
      return {
        defType: "example.PartAssignmentDef",
        refs: ["fixture.framework"],
        fields: {
          speciesNames: {
            role: { type: "targetKey", defType: "ThingDef" },
            cardinality: "list",
            observed: [ASSIGNMENT_RACE_COUNT, ASSIGNMENT_RACE_COUNT],
            inferredRole: null,
          },
          parts: {
            role: { type: "itemSlot", defType: "example.PartDef" },
            cardinality: "list",
            observed: [ASSIGNMENT_RACE_COUNT, ASSIGNMENT_RACE_COUNT],
            inferredRole: null,
          },
          enabled: {
            role: { type: "scalar", kind: { type: "bool" }, default: "true" },
            cardinality: "scalar",
            observed: [ASSIGNMENT_RACE_COUNT, ASSIGNMENT_RACE_COUNT],
            inferredRole: null,
          },
        },
        targetShapes: {
          speciesNames: { defType: "ThingDef", requiredChildren: ["race"] },
        },
      };
    }
    if (defType === "example.PartDef") {
      return {
        defType: "example.PartDef",
        refs: ["fixture.parts"],
        fields: {
          effect: {
            role: { type: "scalar", kind: { type: "text" }, default: "EffectAlpha" },
            cardinality: "scalar",
            observed: [ASSIGNMENT_RACE_COUNT, ASSIGNMENT_RACE_COUNT],
            inferredRole: null,
          },
        },
        targetShapes: {},
      };
    }
    return null;
  }

  function newSection(defType: string, schema: AssignmentSchemaDto): AssignmentSectionState {
    return {
      defType,
      schema,
      isStandalone: !hasTargetKeyField(schema),
      rows: new Map(),
      standaloneRows: new Map(),
    };
  }

  function assignmentModRefs(ids: string[]): { modId: string; name: string }[] {
    return ids.map((id) => ({ modId: id, name: nameForModId(id) }));
  }

  /** Every target-keyed section's own coverage rows summed together — `null` (not `0`) when the project has no target-keyed section at all, mirroring `AssignmentSummaryDto.uncoveredCount`/`isStandalone`. */
  function uncoveredCountOf(assignment: AssignmentProjectState): number | null {
    const targetKeyed = sortedSections(assignment).filter((section) => !section.isStandalone);
    if (targetKeyed.length === 0) {
      return null;
    }
    return targetKeyed.reduce(
      (sum, section) =>
        sum +
        assignmentCoverageRows(section).filter((row) => row.intent === "cover" && !row.hasRow)
          .length,
      0,
    );
  }

  function assignmentSummaryOf(assignment: AssignmentProjectState) {
    const sections = sortedSections(assignment);
    const uncoveredCount = uncoveredCountOf(assignment);
    return {
      id: assignment.id,
      name: assignment.name,
      packageId: assignment.packageId,
      displayName: assignment.displayName,
      defTypes: sections.map((section) => section.defType),
      refs: assignmentModRefs(assignment.refs),
      targets: assignmentModRefs(assignment.targets),
      rowCount: sections.reduce(
        (sum, section) => sum + section.rows.size + section.standaloneRows.size,
        0,
      ),
      isStandalone: uncoveredCount === null,
      uncoveredCount,
      exportDir: assignment.exportDir,
      updatedAt: assignment.updatedAt,
    };
  }

  function assignmentDetailOf(assignment: AssignmentProjectState) {
    return {
      ...assignmentSummaryOf(assignment),
      excludedRefs: assignmentModRefs(assignment.excludedRefs),
      author: assignment.author,
      description: assignment.description,
      folderName: assignment.packageId.replaceAll(".", "_"),
      sections: sortedSections(assignment).map((section) => ({
        defType: section.defType,
        schema: section.schema,
        isStandalone: section.isStandalone,
        rows: [...section.rows.values()],
        standaloneRows: [...section.standaloneRows.values()].map((row) => ({ row })),
      })),
      createdAt: assignment.createdAt,
    };
  }

  /**
   * Every candidate target (the five fixture races, all owned by
   * `fixture.target`) with its coverage state, for one target-keyed
   * section — no external existing instances modeled in this mock, so
   * `matches` is always empty and `winner` always `null` (`Unverified`,
   * exactly as the real `example.PartAssignmentDef` fixture has no built-in
   * precedence rule).
   */
  function assignmentCoverageRows(section: AssignmentSectionState) {
    return Array.from({ length: ASSIGNMENT_RACE_COUNT }, (_, index) => {
      const target = {
        keyField: "speciesNames",
        def: { defType: "ThingDef", defName: assignmentRaceName(index) },
      };
      const hasRow = section.rows.has(rowKey(target));
      return {
        target,
        owner: "fixture.target",
        matches: [] as { owner: string; instanceDefName: string; keyField: string }[],
        intent: "cover" as const,
        winner: null,
        hasRow,
      };
    });
  }

  /** Every row, in a section other than `removedDefType`, whose own `ItemSlot` value names one of `removedDefType`'s own free-standing rows — `remove_assignment_section`'s own in-use check, mirroring `SectionError::SectionInUse`. */
  function referencingRows(
    assignment: AssignmentProjectState,
    removedDefType: string,
  ): SectionReferenceDto[] {
    const removed = assignment.sections.get(removedDefType);
    if (!removed || removed.standaloneRows.size === 0) {
      return [];
    }
    const ownNames = new Set(removed.standaloneRows.keys());
    const references: SectionReferenceDto[] = [];
    for (const section of sortedSections(assignment)) {
      if (section.defType === removedDefType) {
        continue;
      }
      const itemSlotPaths = Object.entries(section.schema.fields)
        .filter(
          ([, spec]) =>
            spec !== undefined &&
            spec.role.type === "itemSlot" &&
            spec.role.defType === removedDefType,
        )
        .map(([path]) => path);
      if (itemSlotPaths.length === 0) {
        continue;
      }
      const checkValues = (row: string, values: Record<string, unknown>): void => {
        for (const path of itemSlotPaths) {
          const value = values[path] as { kind?: string; names?: string[] } | undefined;
          const names = value?.kind === "names" ? (value.names ?? []) : [];
          if (names.some((name) => ownNames.has(name))) {
            references.push({ defType: section.defType, row, path });
          }
        }
      };
      for (const [key, entry] of section.rows) {
        checkValues(key, entry.row.values);
      }
      for (const [defName, row] of section.standaloneRows) {
        checkValues(defName, row.values);
      }
    }
    return references;
  }

  window.__DECIDE_CALLS__ = [];
  window.__REVERT_CALLS__ = [];
  window.__APPLY_CALLS__ = [];
  window.__VERIFY_ORDER_CALLS__ = [];
  window.__UPSERT_RULE_CALLS__ = [];
  window.__RESOLVE_DEF_GRAPHIC_CALLS__ = [];
  window.__READ_DEF_TEXTURE_CALLS__ = [];
  window.__SAVE_APP_CONFIG_CALLS__ = [];
  window.__DELETE_RULE_CALLS__ = [];
  window.__PROMOTE_RULE_CALLS__ = [];
  window.__SET_MANUAL_TAG_CALLS__ = [];
  window.__SET_SETTINGS_CALLS__ = [];
  window.__IMPORT_RIMSORT_CALLS__ = [];
  window.__SET_MERGE_CHOICES_CALLS__ = [];
  window.__CREATE_PATCH_CALLS__ = [];
  window.__UPDATE_PATCH_CALLS__ = [];
  window.__DECIDE_PATCH_CALLS__ = [];
  window.__EXPORT_PATCH_CALLS__ = [];
  window.__CREATE_ASSIGNMENT_CALLS__ = [];
  window.__UPDATE_ASSIGNMENT_CALLS__ = [];
  window.__SET_ASSIGNMENT_ROW_CALLS__ = [];
  window.__EXPORT_ASSIGNMENT_CALLS__ = [];
  window.__ADD_ASSIGNMENT_SECTION_CALLS__ = [];
  window.__REMOVE_ASSIGNMENT_SECTION_CALLS__ = [];
  window.__ACTIVATE_CALLS__ = [];
  window.__DEACTIVATE_CALLS__ = [];
  window.__RESCAN_CALLS__ = 0;
  window.__OPEN_MOD_LINK_CALLS__ = [];
  window.__OPEN_APP_LINK_CALLS__ = [];
  window.__DISMISS_NOTIFICATION_CALLS__ = [];
  window.__REFRESH_RULE_DATABASES_CALLS__ = [];
  window.__MUTE_NOTIFICATION_CALLS__ = [];
  window.__COMPLETE_WELCOME_CALLS__ = 0;
  window.__RESET_SETTINGS_CALLS__ = 0;
  window.__CHECK_FOR_UPDATE_CALLS__ = 0;
  window.__RUN_LAUNCH_NETWORK_CHECKS_CALLS__ = 0;
  window.__UPDATE_APP_SETTINGS_CALLS__ = [];

  // Notification state — mirrors `rim_session::ports::NotificationState`
  // (app-global `<base>/notifications.json`) closely enough to exercise
  // the frontend's own gating and dismissal wiring, not to reproduce
  // every `evaluate()` rule (`RuleDatabasesStale` is never driven here,
  // matching `get_rule_databases`'s own "`isStale` always `false`, no
  // mock spec needs it" note — that rule is covered by
  // `rim-session`'s own pure tests instead).
  let welcomeCompletedAt: string | null = null;
  const dismissedFingerprints = new Map<NotificationKindDto, Set<string>>();
  const mutedNotificationKinds = new Set<NotificationKindDto>();
  let updateCheckLastSuccessVersion: string | null = null;
  // The per-profile game-version acknowledgement (`<profile>/notifications.json`
  // in the real backend) — starts equal to `CURRENT_GAME_MAJOR_MINOR` so
  // a fresh scenario never shows a spurious `GameVersionChanged` notice.
  // No spec currently drives a divergent value (that condition is
  // covered by `rim-session`'s own pure `evaluate()` tests instead); a
  // future spec can still exercise the dismiss/acknowledge wiring
  // itself by asserting the `dismiss_notification` payload for a
  // `gameVersionChanged` key.
  const CURRENT_GAME_MAJOR_MINOR = "1.6";
  const RUNNING_APP_VERSION = "1.0.0";
  let acknowledgedGameVersion: string | null = CURRENT_GAME_MAJOR_MINOR;
  let launchChecksRanThisSession = false;

  function computeNotifications(): NotificationDto[] {
    const candidates: NotificationDto[] = [];

    if (welcomeCompletedAt === null) {
      candidates.push({
        key: { kind: "welcome", fingerprint: "welcome" },
        severity: "info",
        actions: ["keepNetworkSettings", "turnOffNetwork", "applyRecommendedSettings"],
        dismissal: "occurrence",
        data: {
          kind: "welcome",
          network: appSettings.network,
          settingsMatchRecommended: JSON.stringify(settings) === JSON.stringify(DEFAULT_SETTINGS),
        },
      });
    }

    if (acknowledgedGameVersion !== null && acknowledgedGameVersion !== CURRENT_GAME_MAJOR_MINOR) {
      candidates.push({
        key: { kind: "gameVersionChanged", fingerprint: CURRENT_GAME_MAJOR_MINOR },
        severity: "warn",
        actions: ["openPatches"],
        dismissal: "occurrence",
        data: {
          kind: "gameVersionChanged",
          acknowledged: acknowledgedGameVersion,
          current: CURRENT_GAME_MAJOR_MINOR,
        },
      });
    }

    if (
      updateCheckLastSuccessVersion !== null &&
      updateCheckLastSuccessVersion !== RUNNING_APP_VERSION &&
      appSettings.network.allowNetwork &&
      appSettings.network.checkForUpdates
    ) {
      candidates.push({
        key: { kind: "updateAvailable", fingerprint: updateCheckLastSuccessVersion },
        severity: "info",
        actions: ["showReleasePage", "openSettings"],
        dismissal: "occurrence",
        data: {
          kind: "updateAvailable",
          running: RUNNING_APP_VERSION,
          latestVersion: updateCheckLastSuccessVersion,
          latestPublishedAt: "2026-01-01T00:00:00Z",
        },
      });
    }

    // `ImportedRulesOutdated` is derived live from `ruleDatabases`, the
    // exact same "cache holds bytes this profile hasn't imported" rule
    // `get_rule_databases` itself computes — never a separately tracked
    // flag that could drift from it.
    const outdated = ruleDatabases.filter(
      (view) => view.cached !== null && view.cached.sha256 !== view.importedSha256,
    );
    if (outdated.length > 0) {
      // Mirrors the real backend's own fingerprint
      // (`imported_rules_outdated_fingerprint`): keyed by each source's
      // own cached sha, not merely which sources are outdated, so
      // dismissing today's stale copy doesn't permanently hide a later,
      // differently-shaped one too.
      const sortedOutdated = outdated.toSorted((a, b) => a.database.localeCompare(b.database));
      candidates.push({
        key: {
          kind: "importedRulesOutdated",
          fingerprint: sortedOutdated
            .map((view) => `${view.database}@${(view.cached?.sha256 ?? "").slice(0, 12)}`)
            .join(";"),
        },
        severity: "info",
        actions: ["openRuleDatabases"],
        dismissal: "occurrenceOrMute",
        data: {
          kind: "importedRulesOutdated",
          sources: Object.fromEntries(
            sortedOutdated.map((view) => [view.database, view.cached?.sha256 ?? ""]),
          ),
        },
      });
    }

    // `RuleDatabasesStale`: lists exactly the sources a spec marked stale
    // (`__STALE_RULE_DATABASES__`), never the enabled ones it did not.
    const staleDatabases = window.__STALE_RULE_DATABASES__ ?? [];
    if (
      staleDatabases.length > 0 &&
      welcomeCompletedAt !== null &&
      appSettings.network.allowNetwork
    ) {
      const staleKey = {
        community: "community_rules",
        steam: "steam_workshop",
        rimmerge: "rimmerge_rules",
      } as const;
      const sortedStale = staleDatabases.toSorted((a, b) => a.localeCompare(b));
      candidates.push({
        key: {
          kind: "ruleDatabasesStale",
          fingerprint: sortedStale.map((database) => `${staleKey[database]}@never`).join(";"),
        },
        severity: "info",
        actions: ["refreshRuleDatabases", "openSettings"],
        dismissal: "occurrenceOrMute",
        data: {
          kind: "ruleDatabasesStale",
          sources: Object.fromEntries(
            sortedStale.map((database) => [
              database,
              {
                freshness: { kind: "neverFetched" },
                refresh: database === "steam" ? "manual" : "automatic",
                lastFailure: null,
              },
            ]),
          ),
        },
      });
    }

    // `RecommendedSourcesIncomplete`, mirroring `evaluate`'s rules: never
    // before Welcome is answered, never while internet access is off; a
    // disabled source is `off`; an enabled, never-cached source is
    // `notDownloaded` only when the automatic refresh does not cover it
    // (Steam always, or any source while automatic refresh is off).
    if (welcomeCompletedAt !== null && appSettings.network.allowNetwork) {
      const sourceKey = {
        community: "community_rules",
        steam: "steam_workshop",
        rimmerge: "rimmerge_rules",
      } as const;
      const setups: [RuleDatabaseViewDto["database"], "off" | "notDownloaded"][] = [];
      for (const view of ruleDatabases) {
        if (!fetchEnabledFor(view.database)) {
          setups.push([view.database, "off"]);
          continue;
        }
        const isManualOnly =
          view.database === "steam" || !appSettings.network.autoRefreshRuleDatabases;
        if (view.cached === null && isManualOnly) {
          setups.push([view.database, "notDownloaded"]);
        }
      }
      if (setups.length > 0) {
        candidates.push({
          key: {
            kind: "recommendedSourcesIncomplete",
            fingerprint: setups
              .map(
                ([database, setup]) =>
                  `${sourceKey[database]}@${setup === "off" ? "off" : "not_downloaded"}`,
              )
              .join(";"),
          },
          severity: "info",
          actions: ["enableRecommendedSources", "openRuleDatabases"],
          dismissal: "occurrenceOrMute",
          data: {
            kind: "recommendedSourcesIncomplete",
            sources: Object.fromEntries(setups),
          },
        });
      }
    }

    return candidates.filter((notification) => {
      const dismissed = dismissedFingerprints.get(notification.key.kind);
      if (dismissed?.has(notification.key.fingerprint)) {
        return false;
      }
      return !mutedNotificationKinds.has(notification.key.kind);
    });
  }

  const fixtures: IpcFixtures = {
    get_default_paths: {
      gameDir: "C:/RimWorld",
      workshopDir: "C:/RimWorld/workshop",
      modsConfig: "C:/Profile/ModsConfig.xml",
      profileDir: "C:/Profile/rimmerge",
      warning: null,
      warningCode: null,
      error: null,
      errorCode: null,
    },
    save_app_config: (payload: unknown) => {
      window.__SAVE_APP_CONFIG_CALLS__?.push((payload as { config: AppConfigDto }).config);
      return null;
    },
    get_default_rimsort_paths: {
      userRules: "C:/RimSort/dbs/userRules.json",
      communityRules: "C:/RimSort/dbs/Community-Rules-Database/communityRules.json",
      steamDb: "C:/RimSort/dbs/Steam-Workshop-Database/steamDB.json",
    },
    load_project: async () => {
      const progress = window.__LOAD_PROJECT_PROGRESS__;
      if (progress) {
        for (const payload of progress) {
          await window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {
            event: "project://progress",
            payload,
          });
        }
        await new Promise<void>((resolve) => {
          window.__RELEASE_LOAD_PROJECT__ = resolve;
        });
      }
      // Mirrors the real `load_project`: a freshly loaded session opens on
      // the suggested order and says so in its summary.
      selected = "suggested";
      return {
        modCount: MOD_COUNT,
        gameVersion: "1.6.4871",
        elapsedMs: 900,
        warnings: [],
        ruleWarnings: [],
        selected,
      };
    },
    get_dashboard: () => {
      const needsInputByKind: Record<string, number> = {
        edgeDropped: 0,
        anyOfChoice: 0,
        defOverride: 0,
        patchCollision: 0,
        textureOverride: 0,
        duplicateAssembly: 0,
        likelyDuplicateMod: 0,
        missingMod: 0,
        missingDependency: 0,
        incompatiblePair: 0,
        unsupportedVersion: 0,
        undeclaredHardDependency: 0,
        lazyReferenceViolated: 0,
        declarationQuestioned: 0,
        duplicateTemplateName: 0,
        keyedTranslationCollision: 0,
        soundOverride: 0,
        undeclaredTypeDependency: 0,
        runtimePatchCollision: 0,
        transpilerCollision: 0,
        tagInferred: 0,
        ruleOverruled: 0,
        placementOverruled: 0,
        placementQuestioned: 0,
      };
      for (const entry of findings.values()) {
        if (entry.status === "needsInput") {
          needsInputByKind[entry.kind] = (needsInputByKind[entry.kind] ?? 0) + 1;
        }
      }
      const currentStats = stats();
      return {
        modCount: mods.length,
        edgesByStrength: { hard: 40, declared: 60, soft: 12, awareness: 8 },
        edgesViolatedBySource: {
          assemblyRef: 0,
          forceLoadAfter: 0,
          forceLoadBefore: 0,
          loadAfter: 2,
          loadBefore: 0,
          modDependency: 0,
          findMod: 0,
          ifModActive: 0,
          patchTargetsDef: 0,
          mayRequire: 0,
        },
        conflictsByKind: {
          defOverride: 4,
          patchCollision: 1,
          textureOverride: 0,
          duplicateAssembly: 0,
          likelyDuplicateMod: 0,
        },
        ledgerStats: { current: currentStats, suggested: currentStats },
        needsInputByKind,
        movedMods: 3,
        selected,
        // Mirrors `Session::file_matches`: same sequence, with the
        // generated merge mod's package id ignored on both sides.
        fileMatchesSuggested: sameOrderIgnoringMergeMod(suggestedOrder, fileOrder),
        sortProvenance: {
          tieBreak: settings.tieBreak,
          useImportedPairs: settings.useImportedPairs,
          useImportedPlacements: settings.useImportedPlacements,
        },
      };
    },
    select_order: (payload: unknown) => {
      selected = (payload as { source: OrderSourceDto }).source;
      return stats();
    },
    get_apply_preflight: (payload: unknown): ApplyPreflightDto => {
      const source = (payload as { source: OrderSourceDto }).source;
      const items = hardProblemsFor(source);
      return {
        source,
        items,
        requiresConfirmation: items.some((item) => !item.acknowledged),
      };
    },
    list_order: (payload: unknown) => {
      const source = (payload as { source: OrderSourceDto }).source;
      const order = orderFor(source);
      const currentIndex = new Map(currentOrder.map((id, index) => [id, index]));
      return order.map((id, position) => {
        const mod = mods.find((m) => m.modId === id);
        const previousPosition = source === "current" ? null : (currentIndex.get(id) ?? null);
        return {
          modId: id,
          name: mod?.name ?? id,
          position,
          previousPosition: previousPosition === position ? null : previousPosition,
          tier: position < 2 ? "core" : "body",
          tags: mod?.tags ?? [],
          hardDependents: mod?.hardDependents ?? 0,
          needsInputCount: needsInputCountFor(id),
        };
      });
    },
    explain_placement: (payload: unknown) => {
      const id = (payload as { modId: string }).modId;
      const position = suggestedOrder.indexOf(id);
      const previousIndex = currentOrder.indexOf(id);
      const otherId = suggestedOrder[(position + 1) % suggestedOrder.length] ?? modId(0);
      return {
        modId: id,
        position: position < 0 ? 0 : position,
        previousPosition: previousIndex < 0 ? null : previousIndex,
        tier: "body",
        tierReason: { kind: "body" },
        becameReadyAfter: {
          after: id,
          before: otherId,
          layer: "declared",
          kind: "loadAfter",
          provenance: { kind: "engine" },
          detail: "loadAfter",
        },
        lowerBounds: [
          {
            after: id,
            before: otherId,
            layer: "declared",
            kind: "loadAfter",
            provenance: { kind: "engine" },
            detail: "loadAfter",
          },
        ],
        upperBounds: [],
        // `mod.005` alone gets
        // a dropped edge with a named winner, so the why-panel's
        // "overruled by" line has something real to render — every other
        // mod keeps an empty list.
        dropped:
          id === modId(5)
            ? [
                {
                  edge: {
                    after: id,
                    before: modId(2),
                    layer: "soft",
                    kind: "assemblyRef",
                    provenance: { kind: "engine" },
                    detail: "AssemblyRef Bar.dll",
                  },
                  witnessCycle: [id, modId(2)],
                  winner: {
                    after: modId(2),
                    before: id,
                    layer: "hard",
                    kind: "assemblyRef",
                    provenance: { kind: "engine" },
                    detail: "AssemblyRef Foo.dll",
                  },
                },
              ]
            : [],
        advisory: [],
        tieBreak: {
          currentPosition: previousIndex < 0 ? null : previousIndex,
          effectiveKey: position < 0 ? 0 : position,
          pulledForwardBy: null,
          modsPreferredAhead: 0,
        },
      };
    },
    list_findings: (payload: unknown) => {
      const filter = (
        payload as {
          filter: {
            status: ResolutionStatusDto | null;
            kinds: FindingKindDto[] | null;
            modId: string | null;
            search: string | null;
            offset: number;
            limit: number;
          };
        }
      ).filter;
      let matching = [...findings.values()];
      if (filter.status) matching = matching.filter((entry) => entry.status === filter.status);
      if (filter.kinds && filter.kinds.length > 0)
        matching = matching.filter((entry) => filter.kinds?.includes(entry.kind));
      if (filter.modId)
        matching = matching.filter((entry) => entry.modIds.includes(filter.modId as string));
      if (filter.search)
        matching = matching.filter((entry) => entry.key.includes(filter.search as string));
      matching.sort((a, b) => a.confidence - b.confidence || a.key.localeCompare(b.key));
      const total = matching.length;
      const limit = Math.min(filter.limit, 200);
      const items = matching.slice(filter.offset, filter.offset + limit).map(summaryOf);
      return { total, items };
    },
    get_finding: (payload: unknown) => {
      const key = (payload as { key: string }).key;
      const entry = findings.get(key);
      if (!entry) throw { code: "finding_not_found", message: `${key} is not a live finding` };
      return detailOf(entry);
    },
    decide: (payload: unknown) => {
      const request = (payload as { request: DecideRequestDto }).request;
      window.__DECIDE_CALLS__?.push(request);
      const entry = findings.get(request.key);
      if (entry) {
        entry.effective = request.action;
        entry.hasDecision = true;
        entry.note = request.note;
        if (request.action.kind === "merge") {
          const fixture = mergeFixtures.get(request.key);
          const choices = mergeChoicesByKey.get(request.key) ?? {};
          Object.assign(choices, request.action.choices);
          mergeChoicesByKey.set(request.key, choices);
          const state: MergeStateDto = fixture
            ? computeMergeState(fixture, choices)
            : { kind: "cannotMerge", reason: "unknown finding" };
          entry.mergeState = state;
          entry.status = state.kind === "complete" ? "userOverridden" : "needsInput";
        } else {
          entry.status = "userOverridden";
        }
        if (request.action.kind === "promoteRule") {
          const rule = request.action.rule;
          promoteRuleAtKey(
            rule.kind === "pair"
              ? { kind: "pair", after: rule.after, before: rule.before }
              : { kind: "placement", modId: rule.modId },
          );
        }
      }
      return { stats: stats(), resorted: false, movedMods: 0 };
    },
    revert_decision: (payload: unknown) => {
      const key = (payload as { key: string }).key;
      window.__REVERT_CALLS__?.push(key);
      const entry = findings.get(key);
      if (entry) {
        entry.status = entry.confidence >= settings.threshold ? "auto" : "needsInput";
        entry.effective = entry.suggestionAction;
        entry.hasDecision = false;
        entry.note = null;
        entry.mergeState = null;
        if (mergeFixtures.has(key)) mergeChoicesByKey.set(key, {});
      }
      return { stats: stats(), resorted: false, movedMods: 0 };
    },
    get_merge_preview: (payload: unknown) => {
      const request = (payload as { request: MergePreviewRequestDto }).request;
      const fixture = mergeFixtures.get(request.key);
      if (!fixture) {
        throw {
          code: "invalid_input",
          message: `${request.key} is not a mergeable finding in this fixture`,
        };
      }
      // Against a compat patch's own scope and decisions when
      // `request.patchId` is set, the profile's own otherwise — never
      // both at once, mirroring the real backend.
      const patch = request.patchId ? requirePatch(request.patchId) : null;
      const scopedFixture = scopedMergeFixture(fixture, patch);
      const choices = patch
        ? patchMergeChoicesFor(patch, request.key)
        : (mergeChoicesByKey.get(request.key) ?? {});
      const owners = scopedMergeOwners(fixture, patch);
      const { base, winner } = baseAndWinnerOf(fixture, owners);
      const allFields = scopedFixture.fields.map((f) =>
        buildMergeFieldDto(scopedFixture, f, choices[f.path]),
      );
      let matching = allFields;
      if (request.filter.onlyConflicts) matching = matching.filter((f) => f.class === "conflict");
      if (request.filter.search) {
        const needle = request.filter.search.toLowerCase();
        matching = matching.filter((f) => f.path.toLowerCase().includes(needle));
      }
      const total = matching.length;
      const limit = Math.min(request.filter.limit, 200);
      const page = matching.slice(request.filter.offset, request.filter.offset + limit);
      const totals = {
        fields: allFields.length,
        unchanged: allFields.filter((f) => f.class === "unchanged").length,
        auto: allFields.filter((f) => f.class === "oneSided" || f.class === "agreeing").length,
        conflicts: allFields.filter((f) => f.class === "conflict").length,
        unresolved: allFields.filter((f) => f.class === "conflict" && !choices[f.path]).length,
      };
      return {
        key: fixture.key,
        defKey: fixture.defKey,
        kind: fixture.kind,
        owners,
        base,
        winner,
        totals,
        state: computeMergeState(scopedFixture, choices),
        structuralGuard: scopedFixture.structuralGuard ?? null,
        caveats: fixture.caveats,
        fields: page,
        total,
        resolvedXml: buildResolvedXml(scopedFixture, choices),
        outOfScopeOwners: outOfScopeMergeOwnerIds(fixture, patch),
        defRef: defRefOf(fixture),
      };
    },
    set_merge_choices: (payload: unknown) => {
      const request = (payload as { request: SetMergeChoicesRequestDto }).request;
      window.__SET_MERGE_CHOICES_CALLS__?.push(request);
      const fixture = mergeFixtures.get(request.key);
      if (!fixture) {
        throw {
          code: "invalid_input",
          message: `${request.key} is not a mergeable finding in this fixture`,
        };
      }
      // Writes into the patch's own decisions when `request.patchId` is
      // set — never `mergeChoicesByKey`/`findings`, the profile's own
      // state — and vice versa: a profile call never touches a patch.
      if (request.patchId) {
        const patch = requirePatch(request.patchId);
        patch.mergeChoices.set(request.key, { ...request.choices });
        const state = computeMergeState(scopedMergeFixture(fixture, patch), request.choices);
        patch.decisions.set(request.key, {
          action: {
            kind: "merge",
            key: fixture.defKey,
            choices: request.choices as Record<string, MergeChoiceDto>,
          },
          note: patch.decisions.get(request.key)?.note ?? null,
        });
        patch.updatedAt = new Date().toISOString();
        return state;
      }
      mergeChoicesByKey.set(request.key, { ...request.choices });
      const state = computeMergeState(fixture, request.choices);
      const entry = findings.get(request.key);
      if (entry) {
        entry.mergeState = state;
        entry.status = state.kind === "complete" ? "userOverridden" : "needsInput";
        if (entry.effective.kind === "merge") {
          entry.effective = {
            ...entry.effective,
            choices: request.choices as Record<string, MergeChoiceDto>,
          };
        }
      }
      return state;
    },
    get_merge_mod: () => {
      const entries: MergeModEntryDto[] = mergeModEntrySources().map(({ key, fixture, state }) => ({
        key,
        defKey: fixture.defKey,
        kind: fixture.kind,
        state,
        opCount: state.kind === "complete" ? state.opCount : 0,
        dependsOn: fixture.owners
          .filter((owner) => owner.modId !== fixture.base)
          .map((o) => o.modId),
        patchFile:
          state.kind === "complete" && state.opCount > 0
            ? `Patches/rimmerge_${fixture.defKey.defType}.xml`
            : null,
        structuralGuardField: fixture.structuralGuard?.field ?? null,
      }));
      return {
        packageId: MERGE_MOD_PACKAGE_ID,
        folderName: MERGE_MOD_FOLDER,
        modsPath: MERGE_MOD_PATH,
        exists: mergeModExists,
        entries,
        files: mergeModFiles(),
        sourceMods: mergeSourceMods(),
      };
    },
    preview_merge_mod_file: (payload: unknown) => {
      const request = (payload as { request: PreviewMergeModFileRequestDto }).request;
      const files = mergeModFiles();
      if (!files.includes(request.relativePath)) {
        throw {
          code: "invalid_input",
          message: `${request.relativePath}: not a file this render produces`,
        };
      }
      if (request.relativePath === "About/About.xml") return { content: aboutXml() };
      if (request.relativePath === "rimmerge.json") return { content: rimmergeJson() };
      const defType = request.relativePath.replace(/^Patches\/rimmerge_/, "").replace(/\.xml$/, "");
      return { content: patchFileFor(defType) };
    },
    read_texture: (payload: unknown) => {
      const request = payload as { modId: string; texturePath: string };
      // Mirrors the backend's refusal of a file that isn't a PNG/JPEG (a
      // `.dds`): its own code, not a generic `invalid_input`.
      if (request.texturePath.startsWith("mock/dds")) {
        throw {
          code: "texture_unsupported_format",
          message: `${request.texturePath}: not a supported texture format (expected PNG or JPEG)`,
        };
      }
      return {
        dataUrl: ONE_PIXEL_PNG_DATA_URL,
        format: "png",
        bytes: 68,
        path: `C:/RimWorld/Mods/${request.modId}/Textures/${request.texturePath}.png`,
      } satisfies TextureDto;
    },
    list_rules: (payload: unknown) => {
      const origin = (payload as { filter: { origin: string | null } }).filter.origin;
      // Computed over the full rule set, then filtered — never the other
      // way around — so a filtered tab still shows the right
      // `promotedFrom`/`alreadyPromoted` for a row whose promoted/
      // imported counterpart the filter itself excludes.
      const full = withPromotionFields(rules);
      if (!origin) return full;
      return {
        pairs: full.pairs.filter((r) => r.origin === origin),
        placements: full.placements.filter((r) => r.origin === origin),
        incompatibles: full.incompatibles.filter((r) => r.origin === origin),
        warnings: full.warnings,
      };
    },
    upsert_rule: (payload: unknown) => {
      const rule = (payload as { rule: RuleDto }).rule;
      window.__UPSERT_RULE_CALLS__?.push(rule);
      if (rule.kind === "pair") {
        rules = {
          ...rules,
          pairs: [
            ...rules.pairs.filter((p) => !(p.after === rule.after && p.before === rule.before)),
            rule,
          ],
        };
      } else if (rule.kind === "placement") {
        rules = {
          ...rules,
          placements: [...rules.placements.filter((p) => p.modId !== rule.modId), rule],
        };
      } else {
        rules = {
          ...rules,
          incompatibles: [
            ...rules.incompatibles.filter((p) => !(p.a === rule.a && p.b === rule.b)),
            rule,
          ],
        };
      }
      return withPromotionFields(rules);
    },
    delete_rule: (payload: unknown) => {
      const request = (payload as { request: DeleteRuleRequestDto }).request;
      window.__DELETE_RULE_CALLS__?.push(request);
      const key = request.key;
      if (key.kind === "pair") {
        rules = {
          ...rules,
          pairs: rules.pairs.filter((p) => !(p.after === key.after && p.before === key.before)),
        };
      } else if (key.kind === "placement") {
        rules = { ...rules, placements: rules.placements.filter((p) => p.modId !== key.modId) };
      } else {
        rules = {
          ...rules,
          incompatibles: rules.incompatibles.filter((p) => !(p.a === key.a && p.b === key.b)),
        };
      }
      return withPromotionFields(rules);
    },
    // Mirrors `Session::promote_imported_rule` — see `promoteRuleAtKey`'s
    // own doc comment, shared with `decide`'s `promoteRule` handling
    // below.
    promote_imported_rule: (payload: unknown) => {
      const key = (payload as { key: RuleKeyDto }).key;
      window.__PROMOTE_RULE_CALLS__?.push(key);
      promoteRuleAtKey(key);
      return withPromotionFields(rules);
    },
    import_rimsort: (payload: unknown) => {
      window.__IMPORT_RIMSORT_CALLS__?.push((payload as { paths: RimSortPathsDto }).paths);
      return {
        userRules: 2,
        communityRules: 1,
        steamDependencies: 3,
        skippedInactiveRules: 0,
        skippedInactiveSteam: 0,
      };
    },
    list_orphaned_decisions: () => [],
    // `enabled`/`needsReimport` are computed fresh from the live
    // `settings` toggles and the cached-vs-imported sha comparison on
    // every call — matching the real backend
    // (`RefreshRuleDatabases::status` builds both from current state,
    // never a separately-tracked copy), so toggling a fetch switch or
    // refreshing the cache is reflected the moment this is re-read.
    // `isStale` is always `false` here — no mock spec drives a 30-days-old
    // fixture; that state is Vitest-only (`RuleDatabasesCard.test.ts`).
    // `bundledSha256` is likewise a pure function of `database` — the real
    // backend's embedded snapshot has one fixed identity per build, never
    // per-scenario state — so it's derived here rather than added to
    // `RuleDatabaseMockState`.
    get_rule_databases: (): RuleDatabaseViewDto[] =>
      ruleDatabases.map((view) => ({
        ...view,
        enabled: fetchEnabledFor(view.database),
        isStale: false,
        needsReimport: view.cached !== null && view.cached.sha256 !== view.importedSha256,
        bundledSha256:
          view.database === "rimmerge"
            ? "e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1"
            : null,
      })),
    refresh_rule_databases: (payload: unknown) => {
      // Mirrors `commands::rules_databases::refresh_rule_databases`: no
      // request means every source; a request means exactly its sources,
      // each once, in first-occurrence order; an empty list is refused.
      const request =
        (payload as { request?: RefreshRuleDatabasesRequestDto } | undefined)?.request ?? null;
      window.__REFRESH_RULE_DATABASES_CALLS__?.push(request);
      if (request !== null && request.sources.length === 0) {
        throw {
          code: "invalid_input",
          message: "a refresh request must name at least one rule database",
        };
      }
      const requestedDatabases =
        request === null
          ? ruleDatabases.map((view) => view.database)
          : [...new Set(request.sources)];
      const requestedViews = requestedDatabases.flatMap((database) =>
        ruleDatabases.filter((view) => view.database === database),
      );

      if (!appSettings.network.allowNetwork) {
        const results: RuleDatabaseRefreshResultDto[] = requestedViews.map((view) => ({
          database: view.database,
          outcome: { kind: "skipped", reason: "networkDisabled" },
        }));
        return results;
      }

      const forceFail = window.__RULE_DATABASE_REFRESH_FORCE_FAIL__ === true;
      const results: RuleDatabaseRefreshResultDto[] = requestedViews.map((view) => {
        const enabled = fetchEnabledFor(view.database);
        if (!enabled) {
          return {
            database: view.database,
            outcome: { kind: "skipped", reason: "sourceDisabled" },
          };
        }
        if (forceFail && view.database === "community") {
          return {
            database: view.database,
            outcome: {
              kind: "failed",
              failure: { cause: { kind: "transport" }, detail: "connection timed out" },
            },
          };
        }
        return {
          database: view.database,
          outcome: {
            kind: "updated",
            sha256: `${view.database}-refreshed-sha256`,
            bytes: mockRefreshedBytes(view.database),
          },
        };
      });

      // Applies each outcome to the persisted mock state: `updated`
      // replaces the cached facts and clears any previous failure;
      // `failed` records the failure but — mirroring the real
      // `RuleDatabaseFetcher` contract — leaves
      // whatever was already cached completely untouched, so the
      // "cached copy is still in use" banner has real, unmoved cached
      // facts underneath it.
      ruleDatabases = ruleDatabases.map((view) => {
        const outcome = results.find((result) => result.database === view.database)?.outcome;
        if (outcome?.kind === "updated") {
          return {
            ...view,
            cached: {
              sha256: outcome.sha256,
              bytes: outcome.bytes,
              fetchedAt: new Date().toISOString(),
            },
            lastFailure: null,
          };
        }
        if (outcome?.kind === "failed") {
          return { ...view, lastFailure: outcome.failure };
        }
        return view;
      });

      return results;
    },
    list_tags: () => ({
      assignments: [...tagsByMod.entries()].flatMap(([id, tags]) =>
        tags.map((tag) => ({
          modId: id,
          tag,
          provenance: {
            kind: "inferred" as const,
            matched: [{ kind: "urlContains" as const, needle: "framework" }],
            confidence: 90,
          },
        })),
      ),
    }),
    set_manual_tag: (payload: unknown) => {
      const request = (payload as { request: SetManualTagRequestDto }).request;
      window.__SET_MANUAL_TAG_CALLS__?.push(request);
      const current = tagsByMod.get(request.modId) ?? [];
      const next =
        request.mode === "add"
          ? [...new Set([...current, request.tag])]
          : current.filter((tag) => tag !== request.tag);
      tagsByMod.set(request.modId, next);
      const mod = mods.find((m) => m.modId === request.modId);
      if (mod) mod.tags = next;
      return {
        assignments: [...tagsByMod.entries()].flatMap(([id, tags]) =>
          tags.map((tag) => ({
            modId: id,
            tag,
            provenance: {
              kind: "inferred" as const,
              matched: [{ kind: "urlContains" as const, needle: "framework" }],
              confidence: 90,
            },
          })),
        ),
      };
    },
    list_mods: (payload: unknown) => {
      const filter = (
        payload as {
          filter: {
            search: string | null;
            tag: string | null;
            source: string | null;
            offset: number;
            limit: number;
          };
        }
      ).filter;
      let matching = scannedActiveModList();
      if (filter.search) {
        const needle = filter.search.toLowerCase();
        matching = matching.filter(
          (mod) =>
            mod.modId.toLowerCase().includes(needle) || mod.name.toLowerCase().includes(needle),
        );
      }
      if (filter.tag) matching = matching.filter((mod) => mod.tags.includes(filter.tag as string));
      if (filter.source) matching = matching.filter((mod) => mod.source === filter.source);
      const total = matching.length;
      const items = matching.slice(filter.offset, filter.offset + filter.limit);
      return { total, items };
    },
    list_mod_names: () => allModNames(),
    // The Inactive tab
    // and the working active-mod set.
    list_inactive_mods: (payload: unknown) => {
      const filter = (
        payload as {
          filter: { search: string | null; source: string | null; offset: number; limit: number };
        }
      ).filter;
      let matching = scannedInactiveModList();
      if (filter.search) {
        const needle = filter.search.toLowerCase();
        matching = matching.filter(
          (mod) =>
            mod.modId.toLowerCase().includes(needle) || mod.name.toLowerCase().includes(needle),
        );
      }
      if (filter.source) matching = matching.filter((mod) => mod.source === filter.source);
      const total = matching.length;
      const items = matching.slice(filter.offset, filter.offset + filter.limit);
      return { total, items };
    },
    plan_activate_mods: (payload: unknown) => {
      const request = (payload as { request: ActivateRequestDto }).request;
      return planActivateMods(request.ids, request.withDependencies);
    },
    activate_mods: (payload: unknown) => {
      const request = (payload as { request: ActivateRequestDto }).request;
      window.__ACTIVATE_CALLS__?.push(request);
      const plan = planActivateMods(request.ids, request.withDependencies);
      for (const id of plan.toAdd) {
        workingActiveIds.add(id);
      }
      return pendingActiveChanges();
    },
    plan_deactivate_mods: (payload: unknown) => {
      const request = (payload as { request: DeactivateRequestDto }).request;
      return planDeactivateMods(request.ids);
    },
    deactivate_mods: (payload: unknown) => {
      const request = (payload as { request: DeactivateRequestDto }).request;
      window.__DEACTIVATE_CALLS__?.push(request);
      const plan = planDeactivateMods(request.ids);
      for (const id of plan.toRemove) {
        workingActiveIds.delete(id);
      }
      return pendingActiveChanges();
    },
    get_pending_active_changes: () => pendingActiveChanges(),
    rescan_project: () => {
      window.__RESCAN_CALLS__ = (window.__RESCAN_CALLS__ ?? 0) + 1;
      scannedActiveIds = new Set(workingActiveIds);
      return {
        modCount: scannedActiveModList().length,
        gameVersion: "1.6.4871",
        elapsedMs: 900,
        warnings: [],
        ruleWarnings: [],
        // Mirrors the real `rescan_project`: the swapped-in session keeps
        // the selection the one it replaces had.
        selected,
      };
    },
    get_mod: (payload: unknown) => {
      const id = (payload as { modId: string }).modId;
      // `[...mods, ...extraModsFromFixtures()]`, mirroring `list_mods`
      // above — a fixture-derived owner (`ludeon.rimworld`,
      // `example.bionicsfork`, ...) is a real, clickable `list_mods` row, so
      // its own detail page must resolve too (the def inspector's own
      // spec navigates through exactly this path).
      const mod = [...mods, ...extraModsFromFixtures()].find((m) => m.modId === id);
      if (!mod) throw { code: "mod_not_found", message: `${id} is not active` };
      const findingKeys = [...findings.values()]
        .filter((entry) => entry.modIds.includes(id))
        .map((entry) => entry.key);
      return {
        modId: mod.modId,
        name: mod.name,
        authors: ["Some Author"],
        url: null,
        source: mod.source,
        supportedVersions: ["1.6"],
        declared: {
          loadAfter: [],
          loadBefore: [],
          forceLoadAfter: [],
          forceLoadBefore: [],
          dependencies: [],
          incompatibleWith: [],
        },
        tags: mod.tags,
        hardDependents: mod.hardDependents,
        isFrameworkCandidate: false,
        edgesIn: [],
        edgesOut: [],
        findingKeys,
      };
    },
    // The mod info panel's own info call — active mods come from the
    // `mods` pool (mirrors `list_mods`' own shape), inactive ones from
    // `inactiveMods` (mirrors `list_inactive_mods`). `about` mirrors the
    // real backend's own outcome for a normally-scanned mod (`read`,
    // description modeled only for `mod.020` (one word) and `mod.021`
    // (forty lines), which `mods.spec.ts` uses to check the description's
    // Show more toggle against a real clamp) — never `notOnDisk`, which the real
    // backend only ever reports for a mod whose files are actually gone.
    get_mod_info: (payload: unknown) => {
      const id = (payload as { modId: string }).modId;
      // `[...mods, ...extraModsFromFixtures()]`, mirroring `get_mod`
      // above — a fixture-derived owner (`ludeon.rimworld`,
      // `example.bionicsfork`, ...) must resolve here too, since the
      // def inspector's own spec reaches it through the panel's own
      // "All details" link.
      const activePool = [...mods, ...extraModsFromFixtures()];
      const active = activePool.find((m) => m.modId === id);
      if (active) {
        const plainRun = (text: string) => ({ text, bold: false, italic: false });
        const description =
          id === "mod.020"
            ? { runs: [plainRun("Continued")], truncated: false }
            : id === "mod.021"
              ? {
                  runs: [
                    plainRun(
                      Array.from({ length: 40 }, (_, line) => `Description line ${line + 1}`).join(
                        String.fromCharCode(10),
                      ),
                    ),
                  ],
                  truncated: false,
                }
              : null;
        const position = activePool.findIndex((m) => m.modId === id);
        return {
          kind: "active",
          modId: active.modId,
          name: active.name,
          authors: ["Some Author"],
          homepage: { kind: "openable", url: "https://example.com/workshop" },
          source: active.source,
          supportedVersions: ["1.6"],
          supportsGameVersion: true,
          declared: {
            loadAfter: [],
            loadBefore: [],
            forceLoadAfter: [],
            forceLoadBefore: [],
            dependencies: [],
            incompatibleWith: [],
          },
          root: `Mods/${active.modId}`,
          loadedFolders: [],
          cost: null,
          tags: active.tags,
          hardDependents: active.hardDependents,
          softDependents: 0,
          awarenessDependents: 0,
          isFrameworkCandidate: false,
          generated: active.generated,
          workshopId: active.source === "workshop" ? 123456789 : null,
          position,
          previousPosition: null,
          tier: "body",
          findingsTotal: [...findings.values()].filter((entry) => entry.modIds.includes(id)).length,
          needsInputCount: 0,
          pending: null,
          about: {
            kind: "read",
            modVersion: null,
            modIconPath: null,
            description,
            homepage: null,
          },
        };
      }
      const inactive = inactiveMods.find((m) => m.modId === id);
      if (inactive) {
        return {
          kind: "inactive",
          modId: inactive.modId,
          name: inactive.name,
          authors: ["Some Author"],
          source: inactive.source,
          supportedVersions: ["1.6"],
          declared: {
            loadAfter: [],
            loadBefore: [],
            forceLoadAfter: [],
            forceLoadBefore: [],
            dependencies: [],
            incompatibleWith: [],
          },
          root: `Mods/${inactive.modId}`,
          workshopId: inactive.source === "workshop" ? 123456789 : null,
          generated: null,
          pending: null,
          about: {
            kind: "read",
            modVersion: null,
            modIconPath: null,
            description: null,
            homepage: null,
          },
        };
      }
      throw { code: "mod_not_found", message: `${id} is not a known mod` };
    },
    read_mod_preview: () => ({ kind: "absent" }),
    read_mod_icon: () => ({ kind: "absent" }),
    open_mod_link: (payload: unknown) => {
      window.__OPEN_MOD_LINK_CALLS__?.push(payload as { modId: string; link: ModLinkKindDto });
      return null;
    },
    open_app_link: (payload: unknown) => {
      window.__OPEN_APP_LINK_CALLS__?.push((payload as { target: AppLinkTargetDto }).target);
      return null;
    },
    get_app_version: () => "1.0.0",
    list_mod_changes: (payload: unknown) => {
      const request = payload as {
        modId: string;
        filter: { kinds: string[] | null; search: string | null; offset: number; limit: number };
      };
      let rows = defChangesFor(request.modId);
      if (request.filter.search) {
        const needle = request.filter.search.toLowerCase();
        rows = rows.filter((row) => row.defRef.toLowerCase().includes(needle));
      }
      const kindCounts = {
        ownsDef: rows.filter((r) => r.kind === "ownsDef").length,
        ownsTemplate: 0,
        patchesDef: rows.filter((r) => r.kind === "patchesDef").length,
        overridesTexture: 0,
        overridesSound: 0,
        overridesKeyedTranslation: 0,
      };
      const matching = request.filter.kinds
        ? rows.filter((row) => request.filter.kinds?.includes(row.kind))
        : rows;
      const total = matching.length;
      const items = matching.slice(
        request.filter.offset,
        request.filter.offset + request.filter.limit,
      );
      return { total, items, kindCounts };
    },
    inspect_def: (payload: unknown) => {
      const request = (payload as { request: InspectDefRequestDto }).request;
      const fixture = fixtureByDefRef(request.defRef);
      if (!fixture) {
        throw {
          code: "def_not_found",
          message: `${request.defRef} is not indexed as a def or template`,
        };
      }
      const isPatchCollision = fixture.kind === "patchCollision";
      const owners = isPatchCollision
        ? [{ modId: fixture.base, position: 0, isGenerated: false }]
        : fixture.owners.map((o) => ({ modId: o.modId, position: o.position, isGenerated: false }));
      const patchers = isPatchCollision
        ? fixture.owners.map((o) => ({
            modId: o.modId,
            position: o.position + 1,
            isGenerated: false,
            ops: [
              {
                class: "PatchOperationReplace",
                xpath: `Defs/${fixture.defKey.defType}[defName="${fixture.defKey.defName}"]/${fixture.fields[0]?.path ?? ""}`,
                subPath: null,
                findModContext: [],
                mayRequire: [],
                mayRequireAnyOf: [],
                locatorFile: `${o.modId}/Patches/patch.xml`,
                isWrapped: false,
              },
            ],
            replayError: null,
            reached: true,
            caveats: [],
          }))
        : [];
      let fields = fixture.fields.map((field) => ({
        path: field.path,
        depth: field.depth,
        isListItem: field.isListItem,
        value: effectiveFieldValue(fixture, field),
        provenance: effectiveFieldProvenance(fixture, field),
      }));
      if (request.filter.onlyPatched) {
        fields = fields.filter((field) => field.provenance.kind !== "owner");
      }
      return {
        defRef: request.defRef,
        source: selected,
        owners,
        winner: isPatchCollision ? fixture.base : fixture.winner,
        // No fixture models a duplicated `[@Name]` template registration
        // — `null` matches what a
        // real, unambiguous inspection always sends.
        templateAmbiguity: null,
        patchers,
        parents: [],
        children: [],
        completeness: { kind: "complete" },
        caveats: fixture.caveats,
        fields: fields.slice(request.filter.offset, request.filter.offset + request.filter.limit),
        fieldsTotal: fields.length,
        resolvedXml: buildEffectiveXml(fixture),
        findings: defFindingsFor(fixture),
      };
    },
    search_defs: (payload: unknown) => {
      const request = payload as { query: string; limit: number };
      const needle = request.query.toLowerCase();
      if (needle.length === 0) return [];
      return [...mergeFixtures.values()]
        .filter(
          (fixture) =>
            fixture.defKey.defName.toLowerCase().includes(needle) ||
            fixture.defKey.defType.toLowerCase().includes(needle),
        )
        .map((fixture) => ({
          defRef: defRefOf(fixture),
          owners: fixture.kind === "patchCollision" ? 1 : fixture.owners.length,
        }))
        .slice(0, request.limit);
    },
    resolve_def_graphic: (payload: unknown) => {
      const request = (payload as { request: ResolveDefGraphicRequestDto }).request;
      window.__RESOLVE_DEF_GRAPHIC_CALLS__?.push(request);
      return resolveMockGraphic(request.defRef);
    },
    read_def_texture: (payload: unknown) => {
      const request = (payload as { request: ReadDefTextureRequestDto }).request;
      window.__READ_DEF_TEXTURE_CALLS__?.push(request);
      return readMockDefTexture(request);
    },
    get_def_conflict_view,
    get_settings: () => settings,
    get_default_settings: () => DEFAULT_SETTINGS,
    set_settings: (payload: unknown) => {
      settings = (payload as { settings: SettingsDto }).settings;
      window.__SET_SETTINGS_CALLS__?.push(settings);
      return settings;
    },
    get_app_settings: (): AppSettingsResponseDto => ({
      settings: appSettings,
      loadStatus: "loaded",
    }),
    update_app_settings: (payload: unknown) => {
      appSettings = (payload as { settings: AppSettingsDto }).settings;
      window.__UPDATE_APP_SETTINGS_CALLS__?.push(appSettings);
      return appSettings;
    },
    // Mirrors `EnableRecommendedSources`: every source's fetch toggle on,
    // nothing else touched (internet access in particular).
    enable_recommended_rule_databases: () => {
      window.__ENABLE_RECOMMENDED_CALLS__ = (window.__ENABLE_RECOMMENDED_CALLS__ ?? 0) + 1;
      appSettings = {
        ...appSettings,
        network: {
          ...appSettings.network,
          fetchCommunityRules: true,
          fetchSteamWorkshop: true,
          fetchRimmergeRules: true,
        },
      };
      return appSettings;
    },
    reset_network_policy: () => {
      appSettings = { ...appSettings, network: { ...DEFAULT_NETWORK_POLICY } };
      return appSettings;
    },
    apply: (payload: unknown) => {
      const request = (payload as { request: ApplyRequestDto }).request;
      window.__APPLY_CALLS__?.push(request);
      // Mirrors the real request DTO, whose `source` is required
      // (`deny_unknown_fields`, no default): a caller that forgets it is
      // rejected rather than silently writing whichever order is selected.
      if (request.source !== "current" && request.source !== "suggested") {
        throw { code: "invalid_input", message: "apply request is missing a valid source" };
      }
      if (window.__APPLY_FORCE_GAME_RUNNING__ && !request.force) {
        throw {
          code: "rimworld_running",
          message: "RimWorldWin64.exe is running; close the game or retry with force",
        };
      }
      // Mirrors the real backend's
      // `ApplyError::StaleActiveSet` refusal — a non-empty `unscanned`
      // diff means the working set has pending changes no rescan has
      // picked up yet.
      if (request.writeModsConfig) {
        const { unscanned } = pendingActiveChanges();
        if (unscanned.added.length > 0 || unscanned.removed.length > 0) {
          throw {
            code: "stale_active_set",
            message: `the working active-mod set has ${unscanned.added.length + unscanned.removed.length} pending change(s) no rescan has picked up yet`,
          };
        }
        fileActiveIds = new Set(scannedActiveIds);
        fileOrder = [...orderFor(request.source)];
      }
      const hasMergeContent = mergeModFiles().length > 0;
      const mergeModPath = request.writeMergeMod && hasMergeContent ? MERGE_MOD_PATH : null;
      // Mirrors the real `MergeModWriter`: a `writeMergeMod: true` apply
      // either renders the folder (`mergeModPath` present) or removes a
      // stale one (per `MergeModPath: null`) — `get_merge_mod.exists`
      // should reflect whichever just happened. An apply that never asked
      // to write the merge mod leaves it untouched either way.
      if (request.writeMergeMod) {
        mergeModExists = mergeModPath !== null;
      }
      return {
        modsConfigPath: "C:/RimWorld/ModsConfig.xml",
        backupPath: request.writeModsConfig
          ? "C:/RimWorld/ModsConfig.xml.bak-2026-09-05T00-00-00Z"
          : null,
        decisionsPath: "C:/Profile/rimmerge/decisions.json",
        rulesPath: "C:/Profile/rimmerge/rules.json",
        mergeModPath,
        mergeModBackupPath: null,
        skippedMerges: request.writeMergeMod
          ? mergeModEntrySources()
              .filter(({ state }) => state.kind !== "complete")
              .map(({ key }) => key)
          : [],
      };
    },
    // A fixed, deliberately small predicted-failure
    // set — this mock never actually replays anything (that verification
    // is real-install-only, `apps/desktop/src-tauri/src/real_install_timing.rs`'s
    // own job), just exercises the apply dialog's own summary/grouping UI
    // against real DTO shapes. `example.bionicsfork`/`ludeon.rimworld` are
    // already-active mods elsewhere in this fixture, so `useModLabel`
    // resolves them to real display names too.
    verify_order: (payload: unknown) => {
      const request = (payload as { request: VerifyRequestDto }).request;
      window.__VERIFY_ORDER_CALLS__?.push(request);
      if (window.__VERIFY_ORDER_OVERRIDE__) {
        return window.__VERIFY_ORDER_OVERRIDE__;
      }
      // Two distinct operations, each already the shape the real
      // `verify_order` command sends post-grouping (one entry per real
      // `(mod, operation)` pair, `defs` naming every affected def target)
      // — this mock never groups anything itself, it just returns the
      // grouped shape directly.
      const operations: VerifyOperationDto[] = [
        {
          modId: "example.bionicsfork",
          operation: 'Verse.PatchOperationAdd(Defs/HediffDef[defName="BionicHeart"]/comps)',
          defs: [
            {
              defKey: { defType: "HediffDef", defName: "BionicHeart" },
              selector: "defName",
              leafXpath: null,
              cause: { kind: "removedBy", modId: "ludeon.rimworld" },
              reorderKind: { kind: "content" },
              // The real backend computes this by
              // calling `rim_resolve::ledger::suggest` on the finding
              // and taking its `Reorder` alternative — a `RemovedBy`
              // cause always orients the *remover* after the mod whose
              // operation failed. Hardcoded here (this mock replays
              // nothing) so the apply dialog's own "Create pair rule"
              // button has something to act on; `SyntheticLung`'s
              // `deadTarget` row keeps `reorder: null`, which is what
              // makes the button's own "only on order-fixable rows"
              // rule observable in the same fixture.
              // This row's reorder also
              // carries one conflict, so the same fixture exercises the
              // "Create pair rule anyway" relabeling and the conflict
              // line alongside the create-rule flow the apply spec
              // drives through this exact row — never a
              // second operation, which would change every count
              // assertion in `apply.spec.ts` that keys off "2
              // operations".
              reorder: {
                after: "ludeon.rimworld",
                before: "example.bionicsfork",
                rationale:
                  "Load example.bionicsfork before ludeon.rimworld, so its operation runs while the node still exists.",
                rationaleCode: {
                  kind: "reorderBeforeRemover",
                  modId: "example.bionicsfork",
                  remover: "ludeon.rimworld",
                },
                conflicts: [
                  {
                    kind: "loadAfter",
                    detail: "example.bionicsfork declares loadAfter ludeon.rimworld",
                    status: "satisfied",
                    direction: "reverses",
                  },
                ],
              },
            },
          ],
        },
        {
          modId: "example.bionicsfork",
          operation: 'Verse.PatchOperationReplace(Defs/HediffDef[defName="SyntheticLung"]/label)',
          defs: [
            {
              defKey: { defType: "HediffDef", defName: "SyntheticLung" },
              selector: "defName",
              leafXpath: 'Defs/HediffDef[defName="SyntheticLung"]/label',
              cause: { kind: "deadTarget" },
              reorderKind: null,
              reorder: null,
            },
          ],
        },
      ];
      const report: VerifyReportDto = {
        source: request.source,
        defsChecked: 42,
        operationsTotal: operations.length,
        defTargetsTotal: operations.reduce((sum, operation) => sum + operation.defs.length, 0),
        operations,
        skipped: [],
        counterfactual: {
          jobs: 0,
          resolved: 0,
          demotedDeadTargets: 0,
          skippedTooManyMods: 0,
          skippedCoOwner: 0,
          rejectedForRegression: 0,
        },
      };
      return report;
    },
    list_patches: () => [...patches.values()].map(patchSummaryOf),
    get_patch: (payload: unknown) => {
      const patch = requirePatch((payload as { patchId: string }).patchId);
      return patchDetailOf(patch);
    },
    create_patch: (payload: unknown) => {
      const request = (payload as { request: CreatePatchRequestDto }).request;
      window.__CREATE_PATCH_CALLS__?.push(request);
      if (request.scope.length < 2) {
        throw {
          code: "invalid_input",
          message: "a patch scope needs at least two distinct mods",
        };
      }
      const now = new Date().toISOString();
      const patch: PatchProjectState = {
        id: newPatchId(),
        name: request.name,
        packageId: request.packageId,
        displayName: request.displayName,
        author: "Rimmerge",
        description: `Compatibility patch for ${request.scope.map(nameForModId).join(", ")}, generated by Rimmerge from field-level merge decisions.`,
        scope: [...request.scope],
        decisions: new Map(),
        mergeChoices: new Map(),
        exportDir: null,
        createdAt: now,
        updatedAt: now,
      };
      patches.set(patch.id, patch);
      return patchDetailOf(patch);
    },
    update_patch: (payload: unknown) => {
      const request = (payload as { request: UpdatePatchRequestDto }).request;
      window.__UPDATE_PATCH_CALLS__?.push(request);
      const patch = requirePatch(request.patchId);
      if (request.scope !== null && request.scope.length < 2) {
        throw {
          code: "invalid_input",
          message: "a patch scope needs at least two distinct mods",
        };
      }
      if (request.name !== null) patch.name = request.name;
      if (request.packageId !== null) patch.packageId = request.packageId;
      if (request.displayName !== null) patch.displayName = request.displayName;
      if (request.author !== null) patch.author = request.author;
      if (request.description !== null) patch.description = request.description;
      let scopeChange = null;
      if (request.scope !== null) {
        const nowOrphaned = orphanedKeysOf(patch, request.scope);
        patch.scope = [...request.scope];
        scopeChange = { nowOrphaned, choicesNamingRemoved: [] };
      }
      patch.updatedAt = new Date().toISOString();
      return { patch: patchDetailOf(patch), scopeChange };
    },
    delete_patch: (payload: unknown) => {
      const patchId = (payload as { patchId: string }).patchId;
      requirePatch(patchId);
      patches.delete(patchId);
      return null;
    },
    list_patch_findings: (payload: unknown) => {
      const request = (
        payload as {
          request: {
            patchId: string;
            filter: {
              status: ResolutionStatusDto | null;
              kinds: FindingKindDto[] | null;
              modId: string | null;
              search: string | null;
              offset: number;
              limit: number;
            };
          };
        }
      ).request;
      const patch = requirePatch(request.patchId);
      let matching = scopedEntries(patch);
      const { filter } = request;
      if (filter.status) {
        matching = matching.filter(({ entry }) => patchStatusFor(patch, entry) === filter.status);
      }
      if (filter.kinds && filter.kinds.length > 0) {
        matching = matching.filter(({ entry }) => filter.kinds?.includes(entry.kind));
      }
      if (filter.modId) {
        matching = matching.filter(({ entry }) => entry.modIds.includes(filter.modId as string));
      }
      if (filter.search) {
        matching = matching.filter(({ entry }) => entry.key.includes(filter.search as string));
      }
      matching.sort(
        (a, b) => a.entry.confidence - b.entry.confidence || a.entry.key.localeCompare(b.entry.key),
      );
      const total = matching.length;
      const limit = Math.min(filter.limit, 200);
      const items = matching
        .slice(filter.offset, filter.offset + limit)
        .map(({ entry, membership }) => patchFindingSummary(patch, entry, membership));
      return { total, items };
    },
    get_patch_finding: (payload: unknown) => {
      const { patchId, key } = payload as { patchId: string; key: string };
      const patch = requirePatch(patchId);
      const entry = findings.get(key);
      const membership = entry ? scopeMembership(patch.scope, entry.modIds) : null;
      if (!entry || !membership) {
        throw { code: "finding_not_found", message: `${key} is not a live finding` };
      }
      return patchFindingDetail(patch, entry, membership);
    },
    decide_patch: (payload: unknown) => {
      const request = (payload as { request: DecidePatchRequestDto }).request;
      window.__DECIDE_PATCH_CALLS__?.push(request);
      const patch = requirePatch(request.patchId);
      const entry = findings.get(request.key);
      const membership = entry ? scopeMembership(patch.scope, entry.modIds) : null;
      if (!entry || !membership) {
        throw { code: "finding_not_found", message: `${request.key} is not a live finding` };
      }
      patch.decisions.set(request.key, { action: request.action, note: request.note });
      patch.updatedAt = new Date().toISOString();
      return { stats: patchLedgerStats(patch), resorted: false, movedMods: 0 };
    },
    revert_patch_decision: (payload: unknown) => {
      const { patchId, key } = payload as { patchId: string; key: string };
      const patch = requirePatch(patchId);
      if (!patch.decisions.has(key)) {
        throw { code: "finding_not_found", message: `${key} is not a live finding` };
      }
      patch.decisions.delete(key);
      patch.updatedAt = new Date().toISOString();
      return { stats: patchLedgerStats(patch), resorted: false, movedMods: 0 };
    },
    import_profile_decisions: (payload: unknown) => {
      const request = (payload as { request: { patchId: string; keys: string[] | null } }).request;
      const patch = requirePatch(request.patchId);
      const candidateKeys = request.keys ?? scopedEntries(patch).map(({ entry }) => entry.key);
      const imported: string[] = [];
      const skipped: SkippedImportDto[] = [];
      for (const key of candidateKeys) {
        const entry = findings.get(key);
        if (!entry?.hasDecision) continue;
        if (entry.effective.kind !== "merge" && entry.effective.kind !== "shipAsset") continue;
        const membership = scopeMembership(patch.scope, entry.modIds);
        if (!membership) {
          // Mirrors `PatchDecisionError::OutOfScope`, the rejection the real
          // `ImportProfileDecisions` reports for a key outside the scope.
          skipped.push({ key, cause: { kind: "outOfScope" } });
          continue;
        }
        patch.decisions.set(key, { action: entry.effective, note: null });
        imported.push(key);
      }
      patch.updatedAt = new Date().toISOString();
      return { imported, skipped };
    },
    prune_patch_decisions: (payload: unknown) => {
      const patch = requirePatch((payload as { patchId: string }).patchId);
      const liveKeys = new Set(scopedEntries(patch).map(({ entry }) => entry.key));
      for (const key of [...patch.decisions.keys()]) {
        if (!liveKeys.has(key)) patch.decisions.delete(key);
      }
      return patchDetailOf(patch);
    },
    get_patch_render: (payload: unknown) => {
      const patch = requirePatch((payload as { patchId: string }).patchId);
      return {
        packageId: patch.packageId,
        folderName: patch.packageId.replaceAll(".", "_"),
        displayName: patch.displayName,
        dependencies: patch.scope.map((id) => ({ modId: id, name: nameForModId(id) })),
        entries: [],
        files: patch.decisions.size > 0 ? ["About/About.xml", "rimmerge.json"] : [],
        skipped: [],
        decisionsSha256: "0".repeat(64),
        caveats: [],
      };
    },
    preview_patch_file: (payload: unknown) => {
      const { patchId, relativePath } = payload as { patchId: string; relativePath: string };
      const patch = requirePatch(patchId);
      if (relativePath === "About/About.xml") {
        return {
          content: [
            '<?xml version="1.0" encoding="utf-8"?>',
            "<ModMetaData>",
            `  <packageId>${patch.packageId}</packageId>`,
            `  <name>${patch.displayName}</name>`,
            `  <author>${patch.author}</author>`,
            "</ModMetaData>",
            "",
          ].join("\n"),
        };
      }
      throw { code: "invalid_input", message: `${relativePath}: not a file this render produces` };
    },
    export_patch: (payload: unknown) => {
      const request = (payload as { request: ExportPatchRequestDto }).request;
      window.__EXPORT_PATCH_CALLS__?.push(request);
      const patch = requirePatch(request.patchId);
      patch.exportDir = request.outDir;
      patch.updatedAt = new Date().toISOString();
      const folderName = patch.packageId.replaceAll(".", "_");
      return {
        exportPath: `${request.outDir}/${folderName}`,
        installedPath: request.install ? `C:/RimWorld/Mods/${folderName}` : null,
        modsConfigBackupPath: request.install
          ? "C:/RimWorld/ModsConfig.xml.bak-2026-09-05T00-00-00Z"
          : null,
        skipped: [],
        files: ["About/About.xml", "rimmerge.json"],
        decisionsSha256: "0".repeat(64),
      };
    },
    list_assignments: () => [...assignments.values()].map(assignmentSummaryOf),
    get_assignment: (payload: unknown) =>
      assignmentDetailOf(requireAssignment((payload as { assignmentId: string }).assignmentId)),
    list_assignment_candidates: (payload: unknown) => {
      const request = (
        payload as { request: { refs: string[]; excludedRefs: string[]; targets: string[] } }
      ).request;
      const candidates: { defType: string; instanceCount: number; owners: unknown[] }[] = [];
      if (request.refs.includes("fixture.framework")) {
        candidates.push({
          defType: "example.PartAssignmentDef",
          instanceCount: ASSIGNMENT_RACE_COUNT,
          owners: assignmentModRefs(["fixture.framework"]),
        });
      }
      // A standalone ("new def") candidate — `example.PartDef` has no
      // `TargetKey` field at all (a plain `effect` scalar), exactly the
      // shape the no-target-key gate is for. Listed
      // whenever `fixture.parts` is selected, regardless of T — the
      // schema table's own "standalone" note only shows once the user
      // actually infers it with an empty target set.
      if (request.refs.includes("fixture.parts")) {
        candidates.push({
          defType: "example.PartDef",
          instanceCount: ASSIGNMENT_RACE_COUNT,
          owners: assignmentModRefs(["fixture.parts"]),
        });
      }
      return { effectiveRefs: assignmentModRefs(request.refs), candidates };
    },
    infer_assignment_candidate: (payload: unknown) => {
      const request = (
        payload as {
          request: { refs: string[]; excludedRefs: string[]; targets: string[]; defType: string };
        }
      ).request;
      if (
        request.defType === "example.PartAssignmentDef" &&
        request.refs.includes("fixture.framework")
      ) {
        return {
          defType: "example.PartAssignmentDef",
          schema: assignmentSectionSchemaFor("example.PartAssignmentDef"),
          unreadableTargets: [],
          excludedTargets: [],
        };
      }
      if (
        request.defType === "example.PartDef" &&
        request.refs.includes("fixture.parts") &&
        request.targets.length === 0
      ) {
        return {
          defType: "example.PartDef",
          schema: assignmentSectionSchemaFor("example.PartDef"),
          unreadableTargets: [],
          excludedTargets: [],
        };
      }
      throw {
        code: "invalid_input",
        message: `${request.defType} is not a valid candidate for this reference/target selection`,
      };
    },
    create_assignment: (payload: unknown) => {
      const request = (payload as { request: CreateAssignmentRequestDto }).request;
      window.__CREATE_ASSIGNMENT_CALLS__?.push(request);
      const now = new Date().toISOString();
      const assignment: AssignmentProjectState = {
        id: newAssignmentId(),
        name: request.name,
        packageId: request.packageId,
        displayName: request.displayName,
        refs: [...request.refs],
        excludedRefs: [...request.excludedRefs],
        targets: [...request.targets],
        exportDir: null,
        author: "Rimmerge",
        // The real backend never builds a description template for an
        // assignment project — `AssignmentProject::new` defaults it to an
        // empty string (unlike `PatchProject`, whose `CreatePatch` use case
        // does build one from the scope) — see
        // `crates/rim-resolve/src/domain/assignment/project.rs`.
        description: "",
        sections: new Map([
          [request.schema.defType, newSection(request.schema.defType, request.schema)],
        ]),
        createdAt: now,
        updatedAt: now,
      };
      assignments.set(assignment.id, assignment);
      return assignmentDetailOf(assignment);
    },
    update_assignment: (payload: unknown) => {
      const request = (payload as { request: UpdateAssignmentRequestDto }).request;
      window.__UPDATE_ASSIGNMENT_CALLS__?.push(request);
      const assignment = requireAssignment(request.assignmentId);
      if (request.name !== null) assignment.name = request.name;
      if (request.author !== null) assignment.author = request.author;
      if (request.description !== null) assignment.description = request.description;
      if (request.refs !== null) assignment.refs = [...request.refs];
      if (request.excludedRefs !== null) assignment.excludedRefs = [...request.excludedRefs];
      if (request.targets !== null) assignment.targets = [...request.targets];
      assignment.updatedAt = new Date().toISOString();
      // Per-section shape (`AssignmentUpdateResultDto.schemaChanges`/
      // `droppedRows`, one entry per section): this fixture project
      // only ever has one section, so one entry either way, but keyed by
      // its own real `defType` rather than a single unconditional slot —
      // the shape a real multi-section project actually returns.
      const [firstSection] = sortedSections(assignment);
      return {
        assignment: assignmentDetailOf(assignment),
        schemaChanges:
          (request.refs !== null || request.excludedRefs !== null) && firstSection
            ? { [firstSection.defType]: { added: [], removed: [], reclassified: [] } }
            : {},
        strandedValues: [],
        droppedRows: [],
      };
    },
    add_assignment_section: (payload: unknown) => {
      const request = (payload as { request: AddAssignmentSectionRequestDto }).request;
      window.__ADD_ASSIGNMENT_SECTION_CALLS__?.push(request);
      const assignment = requireAssignment(request.assignmentId);
      if (assignment.sections.has(request.defType)) {
        throw {
          code: "invalid_input",
          message: `this project already has a section for ${request.defType}`,
        };
      }
      const schema = assignmentSectionSchemaFor(request.defType);
      if (!schema) {
        throw {
          code: "invalid_input",
          message: `${request.defType} is not a valid candidate for this project's own reference set`,
        };
      }
      const section = newSection(request.defType, schema);
      // The mock has no standalone row-creation UI to drive (a
      // disclosed gap, `AssignmentEditorPage.vue`'s own doc comment) —
      // seeding one real own row the moment a free-standing section is
      // added is what lets `list_assignment_items`'s "This project"
      // group and an exported per-section skip both exercise real state
      // rather than a synthetic read-only stand-in.
      if (section.isStandalone) {
        const ownDefName = `${assignment.packageId.replaceAll(".", "_")}_OwnPart`;
        section.standaloneRows.set(ownDefName, {
          values: { effect: { kind: "text", text: "EffectOwn" } },
          defName: ownDefName,
          note: null,
        });
      }
      assignment.sections.set(request.defType, section);
      assignment.updatedAt = new Date().toISOString();
      return assignmentDetailOf(assignment);
    },
    remove_assignment_section: (payload: unknown) => {
      const request = (payload as { request: RemoveAssignmentSectionRequestDto }).request;
      window.__REMOVE_ASSIGNMENT_SECTION_CALLS__?.push(request);
      const assignment = requireAssignment(request.assignmentId);
      if (!assignment.sections.has(request.defType)) {
        return { removed: false, assignment: assignmentDetailOf(assignment) };
      }
      if (!request.force) {
        const referencedBy = referencingRows(assignment, request.defType);
        if (referencedBy.length > 0) {
          throw {
            code: "assignment_section_in_use",
            message: `cannot remove section ${request.defType}: still referenced by ${referencedBy.length} row(s) in another section`,
            detail: { kind: "assignmentSectionInUse", referencedBy },
          };
        }
      }
      assignment.sections.delete(request.defType);
      assignment.updatedAt = new Date().toISOString();
      return { removed: true, assignment: assignmentDetailOf(assignment) };
    },
    set_assignment_row: (payload: unknown) => {
      const request = (payload as { request: SetAssignmentRowRequestDto }).request;
      window.__SET_ASSIGNMENT_ROW_CALLS__?.push(request);
      const assignment = requireAssignment(request.assignmentId);
      const section = resolveSection(assignment, request.section);
      requireRowKeyMatches(section, request.target !== null);
      if (request.target) {
        const key = rowKey(request.target);
        const replaced = section.rows.get(key)?.row ?? null;
        section.rows.set(key, { target: request.target, row: request.row });
        assignment.updatedAt = new Date().toISOString();
        return { replaced };
      }
      const replaced = section.standaloneRows.get(request.row.defName) ?? null;
      section.standaloneRows.set(request.row.defName, request.row);
      assignment.updatedAt = new Date().toISOString();
      return { replaced };
    },
    clear_assignment_row: (payload: unknown) => {
      const request = (payload as { request: ClearAssignmentRowRequestDto }).request;
      const assignment = requireAssignment(request.assignmentId);
      const section = resolveSection(assignment, request.section);
      requireRowKeyMatches(section, request.target !== null);
      if (request.target) {
        const key = rowKey(request.target);
        const replaced = section.rows.get(key)?.row ?? null;
        section.rows.delete(key);
        assignment.updatedAt = new Date().toISOString();
        return { replaced };
      }
      const defName = request.defName ?? "";
      const replaced = section.standaloneRows.get(defName) ?? null;
      section.standaloneRows.delete(defName);
      assignment.updatedAt = new Date().toISOString();
      return { replaced };
    },
    delete_assignment: (payload: unknown) => {
      const assignmentId = (payload as { assignmentId: string }).assignmentId;
      requireAssignment(assignmentId);
      assignments.delete(assignmentId);
      return null;
    },
    get_assignment_coverage: (payload: unknown) => {
      const request = payload as { assignmentId: string; section: string | null };
      const assignment = requireAssignment(request.assignmentId);
      const section = resolveSection(assignment, request.section);
      if (section.isStandalone) {
        return { applicable: false, rows: [] };
      }
      return { applicable: true, rows: assignmentCoverageRows(section) };
    },
    list_assignment_items: (payload: unknown) => {
      const request = (
        payload as {
          request: {
            defType: string;
            filter: { search: string | null; offset: number; limit: number };
            assignmentId: string | null;
          };
        }
      ).request;
      if (request.defType !== "example.PartDef") {
        return { total: 0, items: [] };
      }
      const ownItems: {
        def: { defType: string; defName: string };
        owner: string;
        hint: string | null;
        own: boolean;
      }[] = [];
      if (request.assignmentId) {
        const assignment = assignments.get(request.assignmentId);
        const section = assignment?.sections.get("example.PartDef");
        if (section?.isStandalone && assignment) {
          for (const [defName, row] of section.standaloneRows) {
            const effect = row.values["effect"] as { kind?: string; text?: string } | undefined;
            ownItems.push({
              def: { defType: "example.PartDef", defName },
              owner: assignment.packageId,
              hint: effect?.kind === "text" ? (effect.text ?? null) : null,
              own: true,
            });
          }
        }
      }
      // Matches `rim_session::use_cases::own_items`/`matching_defs`: both
      // halves filter by `filter.search` (case-insensitive `defName`
      // substring) the same way.
      const needle = request.filter.search?.toLowerCase() ?? null;
      const ownFiltered = needle
        ? ownItems.filter((item) => item.def.defName.toLowerCase().includes(needle))
        : ownItems;
      let names = Array.from({ length: ASSIGNMENT_RACE_COUNT }, (_, index) =>
        assignmentPartName(index),
      );
      if (needle) {
        names = names.filter((name) => name.toLowerCase().includes(needle));
      }
      const items = [
        ...ownFiltered,
        ...names.map((name) => ({
          def: { defType: "example.PartDef", defName: name },
          owner: "fixture.parts",
          hint: assignmentPartHint(Number(name.replace("Part", ""))),
          own: false,
        })),
      ];
      const total = items.length;
      const page = items.slice(request.filter.offset, request.filter.offset + request.filter.limit);
      return { total, items: page };
    },
    copy_assignment_row_from: (payload: unknown) => {
      const request = (payload as { request: CopyAssignmentRowFromRequestDto }).request;
      const assignment = requireAssignment(request.assignmentId);
      const index = Number(
        request.sourceDefName.replace(assignmentGroupDefName(0).slice(0, -1), ""),
      );
      // Matches `CopyAssignmentRowResultDto`: the built row plus every
      // borrowed `ItemSlot` value dropped because the def it named isn't
      // active — this fixture never names an inactive def, so `dropped`
      // is always empty here.
      return {
        row: {
          values: {
            parts: { kind: "names", names: [assignmentPartName(index)] },
            enabled: { kind: "text", text: "true" },
          },
          defName: `${assignment.packageId.replaceAll(".", "_")}_${request.target.def.defName}`,
          note: null,
        },
        dropped: [],
      };
    },
    export_assignment: (payload: unknown) => {
      const request = (payload as { request: ExportAssignmentRequestDto }).request;
      window.__EXPORT_ASSIGNMENT_CALLS__?.push(request);
      const assignment = requireAssignment(request.assignmentId);
      assignment.exportDir = request.outDir;
      assignment.updatedAt = new Date().toISOString();
      const folderName = assignment.packageId.replaceAll(".", "_");
      const skipped: AssignmentSkipDto[] = [];
      if (window.__ASSIGNMENT_EXPORT_FORCE_SKIP__) {
        for (const section of sortedSections(assignment)) {
          const [, firstTargetRow] = [...section.rows.entries()][0] ?? [];
          if (firstTargetRow) {
            skipped.push({
              defType: section.defType,
              target: firstTargetRow.target,
              ownDefName: null,
              path: "parts",
              reason: "item(s) Ghost no longer exist in the active list",
            });
            continue;
          }
          const [firstOwnName] = [...section.standaloneRows.keys()];
          if (firstOwnName) {
            skipped.push({
              defType: section.defType,
              target: null,
              ownDefName: firstOwnName,
              path: "",
              reason: "this row's own def type is no longer part of this project",
            });
          }
        }
      }
      const files = sortedSections(assignment).map(
        (section) => `Defs/rimmerge_${section.defType}.xml`,
      );
      return {
        exportPath: `${request.outDir}/${folderName}`,
        installedPath: request.install ? `C:/RimWorld/Mods/${folderName}` : null,
        modsConfigBackupPath: request.install
          ? "C:/RimWorld/ModsConfig.xml.bak-2026-09-05T00-00-00Z"
          : null,
        skipped,
        files: ["About/About.xml", ...files, "rimmerge.json"],
        contentSha256: "0".repeat(64),
      };
    },
    // `@tauri-apps/plugin-dialog`'s `open()` invokes this — mocked here
    // the same way every other command is, so `pickFolder` (used by
    // `PatchExportPanel`'s "Choose…" button, and `SetupPage`'s) resolves
    // to whatever the spec set via `window.__PICK_FOLDER_RESULT__`
    // (`page.evaluate`, before clicking), `null` (a cancelled picker) by
    // default. `pickFile` (`services/dialogs.ts`) shares
    // this same command — distinguished by the real `open()` call's own
    // `options.directory` flag, since a folder picker always sets it and
    // a file picker never does.
    "plugin:dialog|open": (payload: unknown) => {
      const options = (payload as { options?: { directory?: boolean } } | undefined)?.options;
      return options?.directory
        ? (window.__PICK_FOLDER_RESULT__ ?? null)
        : (window.__PICK_FILE_RESULT__ ?? null);
    },
    // The `/startup` page's cost table.
    get_startup_costs: () => startupCosts,
    // Reads and attributes a `Player.log` — the
    // fixed default unless a spec overrides `window.__IMPORT_GAME_LOG_RESULT__`.
    import_game_log: () =>
      window.__IMPORT_GAME_LOG_RESULT__ ?? {
        ...IMPORT_GAME_LOG_DEFAULT,
        ...window.__IMPORT_GAME_LOG_OVERRIDES__,
      },
    // The apply dialog's own def-cache note.
    get_def_cache_carrier: () => ({ carrierModId: window.__DEF_CACHE_CARRIER_MOD_ID__ ?? null }),
    list_notifications: () => computeNotifications(),
    list_muted_notification_kinds: () => [...mutedNotificationKinds],
    dismiss_notification: (payload: unknown) => {
      const { key } = payload as { key: NotificationKeyDto };
      window.__DISMISS_NOTIFICATION_CALLS__?.push(key);
      // Mirrors `commands::notifications::dismiss_notification`'s own
      // special case: `gameVersionChanged` acknowledges through the
      // per-profile store, never the app-global dismissed list every
      // other kind uses.
      if (key.kind === "gameVersionChanged") {
        acknowledgedGameVersion = key.fingerprint;
        return;
      }
      const fingerprints = dismissedFingerprints.get(key.kind) ?? new Set<string>();
      fingerprints.add(key.fingerprint);
      dismissedFingerprints.set(key.kind, fingerprints);
    },
    mute_notification_kind: (payload: unknown) => {
      const { kind } = payload as { kind: NotificationKindDto };
      window.__MUTE_NOTIFICATION_CALLS__?.push(kind);
      mutedNotificationKinds.add(kind);
    },
    unmute_notification_kind: (payload: unknown) => {
      const { kind } = payload as { kind: NotificationKindDto };
      mutedNotificationKinds.delete(kind);
    },
    complete_welcome: () => {
      welcomeCompletedAt = "2026-09-25T00:00:00Z";
      window.__COMPLETE_WELCOME_CALLS__ = (window.__COMPLETE_WELCOME_CALLS__ ?? 0) + 1;
    },
    reset_settings: () => {
      settings = { ...DEFAULT_SETTINGS };
      window.__RESET_SETTINGS_CALLS__ = (window.__RESET_SETTINGS_CALLS__ ?? 0) + 1;
    },
    check_for_update: (): CheckForUpdateOutcomeDto => {
      window.__CHECK_FOR_UPDATE_CALLS__ = (window.__CHECK_FOR_UPDATE_CALLS__ ?? 0) + 1;
      if (!appSettings.network.allowNetwork) {
        return { kind: "skipped", reason: "networkDisabled" };
      }
      const outcome: CheckForUpdateOutcomeDto = window.__CHECK_FOR_UPDATE_RESULT__ ?? {
        kind: "ran",
        outcome: { kind: "unchanged" },
      };
      if (outcome.kind === "ran" && outcome.outcome.kind === "updated") {
        updateCheckLastSuccessVersion = outcome.outcome.latestVersion;
      }
      return outcome;
    },
    // Mirrors `RunLaunchNetworkChecks::execute`'s own shared gate: **before
    // ever reaching a port**, refuses while the first-run notice is
    // unanswered or once already run this session — both produce an
    // empty `databaseRefresh` and a `Skipped` update check, with zero
    // effect on any tracked state. `already_ran_this_launch` is consumed
    // (flipped to `true`) **before** the first-run check, exactly like
    // the real `CheckForUpdate::execute` — so a second call in the same
    // session that is *still* awaiting first-run reports
    // `alreadyRanThisLaunch`, not `awaitingFirstRun` again, once Welcome
    // is later answered without a fresh launch in between.
    run_launch_network_checks: (): LaunchNetworkChecksOutcomeDto => {
      const alreadyRan = launchChecksRanThisSession;
      launchChecksRanThisSession = true;
      if (welcomeCompletedAt === null) {
        return {
          updateCheck: { kind: "skipped", reason: "awaitingFirstRun" },
          databaseRefresh: [],
        };
      }
      if (alreadyRan) {
        return {
          updateCheck: { kind: "skipped", reason: "alreadyRanThisLaunch" },
          databaseRefresh: [],
        };
      }
      window.__RUN_LAUNCH_NETWORK_CHECKS_CALLS__ =
        (window.__RUN_LAUNCH_NETWORK_CHECKS_CALLS__ ?? 0) + 1;
      if (!appSettings.network.allowNetwork) {
        return {
          updateCheck: { kind: "skipped", reason: "networkDisabled" },
          databaseRefresh: [],
        };
      }
      const updateCheck: CheckForUpdateOutcomeDto = appSettings.network.checkForUpdates
        ? { kind: "ran", outcome: { kind: "unchanged" } }
        : { kind: "skipped", reason: "checkDisabled" };
      return { updateCheck, databaseRefresh: [] };
    },
  };

  window.__E2E_MOCK_IPC__ = fixtures;
}
