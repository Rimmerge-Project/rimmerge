// Project lifecycle, orders, mods, the active set, apply, startup costs, and settings.

import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { AppConfigDto } from "@/types/generated/AppConfigDto";
import type { AppLinkTargetDto } from "@/types/generated/AppLinkTargetDto";
import type { ApplyPreflightDto } from "@/types/generated/ApplyPreflightDto";
import type { ApplyReportDto } from "@/types/generated/ApplyReportDto";
import type { ApplyRequestDto } from "@/types/generated/ApplyRequestDto";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { AppSettingsResponseDto } from "@/types/generated/AppSettingsResponseDto";
import type { DashboardDto } from "@/types/generated/DashboardDto";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import type { DeactivateRequestDto } from "@/types/generated/DeactivateRequestDto";
import type { DefaultPathsDto } from "@/types/generated/DefaultPathsDto";
import type { DefCacheCarrierDto } from "@/types/generated/DefCacheCarrierDto";
import type { GameLogSummaryDto } from "@/types/generated/GameLogSummaryDto";
import type { ImportGameLogRequestDto } from "@/types/generated/ImportGameLogRequestDto";
import type { LedgerStatsDto } from "@/types/generated/LedgerStatsDto";
import type { ModCostRowDto } from "@/types/generated/ModCostRowDto";
import type { ModDetailDto } from "@/types/generated/ModDetailDto";
import type { ModFilterDto } from "@/types/generated/ModFilterDto";
import type { ModInfoDto } from "@/types/generated/ModInfoDto";
import type { ModLinkKindDto } from "@/types/generated/ModLinkKindDto";
import type { ModNamesDto } from "@/types/generated/ModNamesDto";
import type { ModPageDto } from "@/types/generated/ModPageDto";
import type { ModPreviewDto } from "@/types/generated/ModPreviewDto";
import type { OrderRowDto } from "@/types/generated/OrderRowDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { PendingActiveChangesDto } from "@/types/generated/PendingActiveChangesDto";
import type { PlacementExplanationDto } from "@/types/generated/PlacementExplanationDto";
import type { ProjectPathsDto } from "@/types/generated/ProjectPathsDto";
import type { ProjectSummaryDto } from "@/types/generated/ProjectSummaryDto";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";
import type { SettingsDto } from "@/types/generated/SettingsDto";
import { call } from "./core";

/** Scans `paths` and loads a fresh session, emitting `project://progress`. */
export function loadProject(paths: ProjectPathsDto): Promise<ProjectSummaryDto> {
  return call("load_project", { paths });
}

/**
 * The default game/workshop/`ModsConfig.xml`/profile paths for the setup
 * page — the result of the backend's one precedence ladder. Every field
 * is `null`, and `error` carries the reason (naming every candidate
 * location that was looked at), when no install could be found.
 */
export function getDefaultPaths(): Promise<DefaultPathsDto> {
  return call("get_default_paths");
}

/**
 * Pins the setup page's paths into `<base>/config.json` so the next
 * launch prefills them without detection. An empty string unpins that
 * path.
 */
export function saveAppConfig(config: AppConfigDto): Promise<void> {
  return call("save_app_config", { config });
}

/**
 * RimSort's own default database directory (`%LOCALAPPDATA%\RimSort\dbs`),
 * for prefilling the rules page's import dialog. `null` when
 * `%LOCALAPPDATA%` isn't set.
 */
export function getDefaultRimSortPaths(): Promise<RimSortPathsDto | null> {
  return call("get_default_rimsort_paths");
}

/** The dashboard's summary counts for the currently selected order. */
export function getDashboard(): Promise<DashboardDto> {
  return call("get_dashboard");
}

/** Selects the order later ledger/finding queries read from. */
export function selectOrder(source: OrderSourceDto): Promise<LedgerStatsDto> {
  return call("select_order", { source });
}

/** Lists every mod in `source`'s order, one row per mod. */
export function listOrder(source: OrderSourceDto): Promise<OrderRowDto[]> {
  return call("list_order", { source });
}

/** The full why-panel explanation for one mod's placement in the suggested order. */
export function explainPlacement(modId: string): Promise<PlacementExplanationDto> {
  return call("explain_placement", { modId });
}

/** Searches and pages the active mod list. */
export function listMods(filter: ModFilterDto): Promise<ModPageDto> {
  return call("list_mods", { filter });
}

/** One mod's full detail: declared order, edges, tags, and live findings. */
export function getMod(modId: string): Promise<ModDetailDto> {
  return call("get_mod", { modId });
}

/**
 * Every active mod's id mapped to its display name (a `_steam`-suffixed
 * id's base id resolves to the same name too), for render sites that show
 * a mod's name instead of its id.
 */
export function listModNames(): Promise<ModNamesDto> {
  return call("list_mod_names");
}

/** Searches and pages the inactive mod list — the Mods page's Inactive tab. */
export function listInactiveMods(filter: ModFilterDto): Promise<ModPageDto> {
  return call("list_inactive_mods", { filter });
}

/**
 * One mod's full information for the mod info panel: active, inactive,
 * or missing, whichever `modId` resolves to, plus its lazily-read
 * `About.xml` details.
 */
export function getModInfo(modId: string): Promise<ModInfoDto> {
  return call("get_mod_info", { modId });
}

/**
 * `modId`'s own `About/Preview.png`, as a base64 `data:` URL — the mod
 * info panel's image.
 */
export function readModPreview(modId: string): Promise<ModPreviewDto> {
  return call("read_mod_preview", { modId });
}

/**
 * `modId`'s own `About/ModIcon.png`, as a base64 `data:` URL — the mod
 * info panel's small icon, shown beside its name.
 */
export function readModIcon(modId: string): Promise<ModPreviewDto> {
  return call("read_mod_icon", { modId });
}

/**
 * Opens `modId`'s own workshop or homepage link in the system browser.
 * The backend re-derives the actual URL from session data; this call
 * never sends one.
 */
export function openModLink(modId: string, link: ModLinkKindDto): Promise<void> {
  return call("open_mod_link", { modId, link });
}

/**
 * Opens one of the app's own fixed external links (the GitHub repo, its
 * issues page, the support page) in the system browser. The backend maps
 * `target` to its own constant URL; this call never sends one.
 */
export function openAppLink(target: AppLinkTargetDto): Promise<void> {
  return call("open_app_link", { target });
}

/** This build's own version string, for the Settings page's About section. */
export function getAppVersion(): Promise<string> {
  return call("get_app_version");
}

/** Plans what {@link activateMods} would do — a confirm dialog's own read. */
export function planActivateMods(request: ActivateRequestDto): Promise<ActivatePlanDto> {
  return call("plan_activate_mods", { request });
}

/** Commits an activation onto the working (unscanned) active-mod set. */
export function activateMods(request: ActivateRequestDto): Promise<PendingActiveChangesDto> {
  return call("activate_mods", { request });
}

/** Plans what {@link deactivateMods} would do. */
export function planDeactivateMods(request: DeactivateRequestDto): Promise<DeactivatePlanDto> {
  return call("plan_deactivate_mods", { request });
}

/** Commits a deactivation onto the working (unscanned) active-mod set. */
export function deactivateMods(request: DeactivateRequestDto): Promise<PendingActiveChangesDto> {
  return call("deactivate_mods", { request });
}

/**
 * The working active-mod set's own pending-change summary: `unscanned`
 * (the working set vs. the last scan) and `unapplied` (the last scan vs.
 * `ModsConfig.xml` on disk).
 */
export function getPendingActiveChanges(): Promise<PendingActiveChangesDto> {
  return call("get_pending_active_changes");
}

/**
 * Re-scans the session's own working active-mod set, emitting
 * `project://progress` while it runs — the Rescan banner's own action.
 */
export function rescanProject(): Promise<ProjectSummaryDto> {
  return call("rescan_project");
}

/**
 * Persists decisions/rules and, when asked, writes `ModsConfig.xml`.
 * Rejects with a `RimmergeError` whose `code` is `"rimworld_running"`
 * when the game looks like it's running and `request.force` is `false`.
 */
export function apply(request: ApplyRequestDto): Promise<ApplyReportDto> {
  return call("apply", { request });
}

/**
 * The hard problems in `source`'s order — what `apply` would write — for
 * the Apply confirmation. Read-only.
 */
export function getApplyPreflight(source: OrderSourceDto): Promise<ApplyPreflightDto> {
  return call("get_apply_preflight", { source });
}

/**
 * The `/startup` page's per-mod cost table, one
 * row per active mod straight off `Report.mod_costs` — sorting happens
 * client-side.
 */
export function getStartupCosts(): Promise<ModCostRowDto[]> {
  return call("get_startup_costs");
}

/**
 * Reads and parses `path` (a `Player.log`),
 * attributing every failure/timer/etc. against the currently loaded
 * session's own active mods. Read-only — the log is never modified,
 * moved, or copied.
 */
export function importGameLog(path: string): Promise<GameLogSummaryDto> {
  return call("import_game_log", { request: { path } satisfies ImportGameLogRequestDto });
}

/**
 * The active mod (if any) carrying a known def-cache plugin, for the
 * apply dialog's own note.
 */
export function getDefCacheCarrier(): Promise<DefCacheCarrierDto> {
  return call("get_def_cache_carrier");
}

/** The current sorter/ledger settings. */
export function getSettings(): Promise<SettingsDto> {
  return call("get_settings");
}

/** Replaces the sorter/ledger settings, persisting the change. */
export function setSettings(settings: SettingsDto): Promise<SettingsDto> {
  return call("set_settings", { settings });
}

/**
 * The sorter/ledger settings' own hard-coded defaults
 * (`rim_session::Settings::default()`), never the current project's
 * settings and never persisted — the Settings page's "Reset to
 * defaults" reads this to refill its form.
 */
export function getDefaultSettings(): Promise<SettingsDto> {
  return call("get_default_settings");
}

/**
 * The app-global network/reminder preferences (`<base>/app-settings.json`)
 * — one per machine, never per profile — plus how the file loaded, so the
 * Settings page can show its own "your settings file couldn't be read"
 * message. Works even before a project is loaded.
 */
export function getAppSettings(): Promise<AppSettingsResponseDto> {
  return call("get_app_settings");
}

/** Replaces the app-global preferences wholesale, persisting the change. */
export function updateAppSettings(settings: AppSettingsDto): Promise<AppSettingsDto> {
  return call("update_app_settings", { settings });
}

/**
 * Restores just the network half of the app settings to its defaults —
 * every switch back on — leaving the reminder threshold untouched.
 */
export function resetNetworkPolicy(): Promise<AppSettingsDto> {
  return call("reset_network_policy");
}

/**
 * Turns on every recommended rule-database source (Rust decides which
 * those are) and returns the saved settings. The download itself is the
 * ordinary manual refresh, run by the caller afterwards.
 */
export function enableRecommendedRuleDatabases(): Promise<AppSettingsDto> {
  return call("enable_recommended_rule_databases");
}
