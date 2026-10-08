import type { MessageDescriptor } from "@/i18n/messageDescriptor";
import { descriptor } from "@/i18n/messageDescriptor";
import { RimmergeError } from "@/services/ipc";
import type { CommandErrorCode } from "@/types/generated/CommandErrorCode";
import { assertNever } from "@/utils/assertNever";

/**
 * Renders a failed mutation/command call as user-facing text: a
 * {@link RimmergeError}'s own `.message` when the rejection came from the
 * IPC boundary, `fallback` otherwise (a plain `Error`, or any other
 * rejection that never reached the backend at all). Shared by every one
 * of this app's own error-rendering call sites instead of a hand-copied
 * `error instanceof RimmergeError ? error.message : <fallback>` ternary
 * — `fallback` stays a parameter, not a shared string, since each call
 * site's own wording names the action that actually failed.
 *
 * @deprecated Superseded by {@link describeCommandError}, which renders
 * by `CommandErrorCode` instead of showing the backend's own English
 * `.message` as the primary text. Kept for the call sites this batch
 * doesn't touch yet (each one migrates as its own page is extracted);
 * new call sites should use {@link describeCommandError}.
 */
export function describeFailure(error: unknown, fallback: string): string {
  return error instanceof RimmergeError ? error.message : fallback;
}

/**
 * A failed command, rendered for display: a localized `title` +
 * one-line `detail`, keyed off the stable {@link CommandErrorCode} —
 * never by showing the backend's own English `.message` as the primary
 * text. `technicalDetail` carries that raw message for an "expandable
 * technical details" affordance a caller may render; `null` for a
 * rejection that never reached the IPC boundary at all (a plain
 * `Error`, a thrown non-`Error`, …), which renders the generic
 * `error.generic.*` pair instead.
 */
export type CommandErrorDescriptor = {
  readonly title: MessageDescriptor;
  readonly detail: MessageDescriptor;
  readonly technicalDetail: string | null;
};

/**
 * One literal `t()` key pair per {@link CommandErrorCode} — a `satisfies
 * Record<...>` table, not a template-literal key built from `error.code`,
 * so every key stays a source literal (`i18n/locales.test.ts`'s
 * unused-key check needs this) and a new `CommandErrorCode` variant is a
 * TypeScript error here instead of a silently-missing translation.
 */
const ERROR_CODE_KEYS = {
  no_project_loaded: {
    title: "error.code.no_project_loaded.title",
    detail: "error.code.no_project_loaded.detail",
  },
  session_lost: {
    title: "error.code.session_lost.title",
    detail: "error.code.session_lost.detail",
  },
  internal: {
    title: "error.code.internal.title",
    detail: "error.code.internal.detail",
  },
  invalid_input: {
    title: "error.code.invalid_input.title",
    detail: "error.code.invalid_input.detail",
  },
  scan_failed: {
    title: "error.code.scan_failed.title",
    detail: "error.code.scan_failed.detail",
  },
  mods_config_io_failed: {
    title: "error.code.mods_config_io_failed.title",
    detail: "error.code.mods_config_io_failed.detail",
  },
  profile_io_failed: {
    title: "error.code.profile_io_failed.title",
    detail: "error.code.profile_io_failed.detail",
  },
  rimsort_import_failed: {
    title: "error.code.rimsort_import_failed.title",
    detail: "error.code.rimsort_import_failed.detail",
  },
  rimworld_running: {
    title: "error.code.rimworld_running.title",
    detail: "error.code.rimworld_running.detail",
  },
  mod_not_found: {
    title: "error.code.mod_not_found.title",
    detail: "error.code.mod_not_found.detail",
  },
  finding_not_found: {
    title: "error.code.finding_not_found.title",
    detail: "error.code.finding_not_found.detail",
  },
  merge_source_failed: {
    title: "error.code.merge_source_failed.title",
    detail: "error.code.merge_source_failed.detail",
  },
  merge_mod_io_failed: {
    title: "error.code.merge_mod_io_failed.title",
    detail: "error.code.merge_mod_io_failed.detail",
  },
  patch_not_found: {
    title: "error.code.patch_not_found.title",
    detail: "error.code.patch_not_found.detail",
  },
  patch_identity_invalid: {
    title: "error.code.patch_identity_invalid.title",
    detail: "error.code.patch_identity_invalid.detail",
  },
  def_not_found: {
    title: "error.code.def_not_found.title",
    detail: "error.code.def_not_found.detail",
  },
  assignment_not_found: {
    title: "error.code.assignment_not_found.title",
    detail: "error.code.assignment_not_found.detail",
  },
  assignment_section_in_use: {
    title: "error.code.assignment_section_in_use.title",
    detail: "error.code.assignment_section_in_use.detail",
  },
  stale_active_set: {
    title: "error.code.stale_active_set.title",
    detail: "error.code.stale_active_set.detail",
  },
  texture_unsupported_format: {
    title: "error.code.texture_unsupported_format.title",
    detail: "error.code.texture_unsupported_format.detail",
  },
  already_running: {
    title: "error.code.already_running.title",
    detail: "error.code.already_running.detail",
  },
  // `detail` is the fallback only: the sentence naming the closed gate is chosen from the
  // error's own `detail.reason` by {@link recommendedRulesUnavailableDetailKey}.
  recommended_rules_unavailable: {
    title: "error.code.recommended_rules_unavailable.title",
    detail: "error.code.recommended_rules_unavailable.detail",
  },
  app_settings_damaged: {
    title: "error.code.app_settings_damaged.title",
    detail: "error.code.app_settings_damaged.detail",
  },
  order_not_applied: {
    title: "error.code.order_not_applied.title",
    detail: "error.code.order_not_applied.detail",
  },
  game_executable_missing: {
    title: "error.code.game_executable_missing.title",
    detail: "error.code.game_executable_missing.detail",
  },
  steam_launch_failed: {
    title: "error.code.steam_launch_failed.title",
    detail: "error.code.steam_launch_failed.detail",
  },
  game_start_failed: {
    title: "error.code.game_start_failed.title",
    detail: "error.code.game_start_failed.detail",
  },
  mod_list_io_failed: {
    title: "error.code.mod_list_io_failed.title",
    detail: "error.code.mod_list_io_failed.detail",
  },
  nothing_to_export: {
    title: "error.code.nothing_to_export.title",
    detail: "error.code.nothing_to_export.detail",
  },
} satisfies Record<CommandErrorCode, { title: string; detail: string }>;

/**
 * The sentence for a refused "Get the recommended rules" click, by which network gate was
 * closed (`CommandErrorDetail`'s `recommendedRulesUnavailable.reason`). A refusal that
 * arrives without that detail keeps the code's generic sentence.
 */
function recommendedRulesUnavailableDetailKey(error: RimmergeError): string {
  const detail = error.detail;
  if (detail === null || detail.kind !== "recommendedRulesUnavailable") {
    return ERROR_CODE_KEYS.recommended_rules_unavailable.detail;
  }
  switch (detail.reason) {
    case "networkOff":
      return "error.code.recommended_rules_unavailable.networkOff";
    case "awaitingFirstRun":
      return "error.code.recommended_rules_unavailable.awaitingFirstRun";
    default:
      return assertNever(detail.reason);
  }
}

/** The detail key for `error`: its code's own, or the structured-detail variant where one exists. */
function detailKeyFor(error: RimmergeError): string {
  return error.code === "recommended_rules_unavailable"
    ? recommendedRulesUnavailableDetailKey(error)
    : ERROR_CODE_KEYS[error.code].detail;
}

/**
 * Renders a failed mutation/command call by its `CommandErrorCode` — the
 * i18n convention every future error surface should follow (see
 * `apps/desktop/CLAUDE.md`): a code maps to a localized title and
 * explanation; the backend's own English `message` is kept only as a
 * "technical details" fallback, never shown as the primary text.
 */
export function describeCommandError(error: unknown): CommandErrorDescriptor {
  if (error instanceof RimmergeError) {
    const keys = ERROR_CODE_KEYS[error.code];
    return {
      title: descriptor(keys.title),
      detail: descriptor(detailKeyFor(error)),
      technicalDetail: error.message,
    };
  }
  return {
    title: descriptor("error.generic.title"),
    detail: descriptor("error.generic.detail"),
    technicalDetail: null,
  };
}
