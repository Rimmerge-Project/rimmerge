import type { ChangeFilterDto } from "@/types/generated/ChangeFilterDto";
import type { EffectiveFieldFilterDto } from "@/types/generated/EffectiveFieldFilterDto";
import type { FieldRowFilterDto } from "@/types/generated/FieldRowFilterDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";
import type { ListItemsFilterDto } from "@/types/generated/ListItemsFilterDto";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";
import type { ModFilterDto } from "@/types/generated/ModFilterDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";

/**
 * Query key factories for Pinia Colada, centralized here so every query
 * definition and `useSessionEvents`'s invalidation both build the same
 * keys from the same source. Each factory returns a fresh array (Colada
 * keys are plain arrays of primitives — never a reactive value: unwrap
 * refs/route params to their current value before calling one of these).
 */
export const queryKeys = {
  /** `get_dashboard`'s result for the currently selected order. */
  dashboard: () => ["dashboard"] as const,
  /** `get_apply_preflight`'s result for one order source. */
  applyPreflight: (source: OrderSourceDto) => ["apply", "preflight", source] as const,
  /** `get_default_paths`'s result, for the setup page. */
  defaultPaths: () => ["setup", "defaultPaths"] as const,
  /** `get_default_rimsort_paths`'s result, for the import dialog. */
  defaultRimSortPaths: () => ["setup", "defaultRimSortPaths"] as const,
  /** `list_order`'s result for one order source. */
  order: (source: OrderSourceDto) => ["order", source] as const,
  /** `explain_placement`'s result for one mod. */
  placement: (modId: string) => ["order", "placement", modId] as const,
  /** `list_findings`'s result for one filter. */
  findings: (filter: FindingFilterDto) => ["findings", filter] as const,
  /** `get_finding`'s result for one finding key. */
  finding: (key: string) => ["findings", "detail", key] as const,
  /** `list_rules`'s result, optionally filtered to one origin. */
  rules: (origin: RuleOriginDto | null) => ["rules", origin] as const,
  /** `list_orphaned_decisions`'s result. */
  orphanedDecisions: () => ["rules", "orphanedDecisions"] as const,
  /** `get_rule_databases`'s result — the Databases card. */
  ruleDatabases: () => ["rules", "ruleDatabases"] as const,
  /** `list_tags`'s result. */
  tags: () => ["tags"] as const,
  /** `list_mods`'s result for one filter. */
  mods: (filter: ModFilterDto) => ["mods", filter] as const,
  /** `get_mod`'s result for one mod. */
  mod: (modId: string) => ["mods", "detail", modId] as const,
  /** `list_mod_names`'s result. */
  modNames: () => ["mods", "names"] as const,
  /** `list_inactive_mods`'s result for one filter — the Inactive tab. */
  inactiveMods: (filter: ModFilterDto) => ["mods", "inactive", filter] as const,
  /** `get_pending_active_changes`'s result. */
  pendingActiveChanges: () => ["mods", "pendingActiveChanges"] as const,
  /** `get_mod_info`'s result for one mod — the mod info panel. */
  modInfo: (modId: string) => ["mods", "info", modId] as const,
  /** `read_mod_preview`'s result for one mod. */
  modPreview: (modId: string) => ["mods", "preview", modId] as const,
  /** `read_mod_icon`'s result for one mod. */
  modIcon: (modId: string) => ["mods", "icon", modId] as const,
  /** `get_app_version`'s result — the Settings page's About section. */
  appVersion: () => ["appVersion"] as const,
  /** `get_settings`'s result. */
  settings: () => ["settings"] as const,
  /** `get_app_settings`'s result — the app-global network/reminder preferences. */
  appSettings: () => ["appSettings"] as const,
  /** `get_startup_costs`'s result — the `/startup` page's cost table. */
  startupCosts: () => ["startupCosts"] as const,
  /** `get_def_cache_carrier`'s result — the apply dialog's own note. */
  defCacheCarrier: () => ["defCacheCarrier"] as const,
  /** `list_notifications`'s result — the bell and the Welcome card. */
  notifications: () => ["notifications"] as const,
  /** `list_muted_notification_kinds`'s result — Settings → Network's own muted-kinds list. */
  mutedNotificationKinds: () => ["notifications", "muted"] as const,
  /** The merge editor's own commands. */
  merge: {
    /**
     * `get_merge_preview`'s result for one finding key and filter —
     * against the profile when `patchId` is `null`, or against that
     * compat patch's own scoped preview when given.
     */
    preview: (key: string, filter: MergeFieldFilterDto, patchId: string | null = null) =>
      ["merge", "preview", key, filter, patchId] as const,
    /** `get_merge_mod`'s result. */
    mod: () => ["merge", "mod"] as const,
    /** `preview_merge_mod_file`'s result for one relative path. */
    file: (path: string) => ["merge", "file", path] as const,
  },
  /** `read_texture`'s own command. */
  textures: {
    /** `read_texture`'s result for one mod and normalized texture path. */
    one: (modId: string, texturePath: string) => ["textures", modId, texturePath] as const,
  },
  /** The compat-patch commands. */
  patches: {
    /** `list_patches`'s result. */
    list: () => ["patches"] as const,
    /** `get_patch`'s result for one patch. */
    detail: (patchId: string) => ["patches", "detail", patchId] as const,
    /** `list_patch_findings`'s result for one patch and filter. */
    findings: (patchId: string, filter: FindingFilterDto) =>
      ["patches", "detail", patchId, "findings", filter] as const,
    /** `get_patch_finding`'s result for one patch and finding key. */
    finding: (patchId: string, key: string) =>
      ["patches", "detail", patchId, "findings", "detail", key] as const,
    /** `get_patch_render`'s result for one patch. */
    render: (patchId: string) => ["patches", "detail", patchId, "render"] as const,
    /** `preview_patch_file`'s result for one patch and relative path. */
    file: (patchId: string, path: string) => ["patches", "detail", patchId, "file", path] as const,
  },
  /** The def inspector's own commands. */
  defs: {
    /** `list_mod_changes`'s result for one mod and filter. */
    changes: (modId: string, filter: ChangeFilterDto) =>
      ["defs", "changes", modId, filter] as const,
    /** `inspect_def`'s result for one ref, source, and field filter. */
    inspection: (defRef: string, filter: EffectiveFieldFilterDto) =>
      ["defs", "inspection", defRef, filter] as const,
    /** `resolve_def_graphic`'s result for one def ref. */
    graphic: (defRef: string) => ["defs", "graphic", defRef] as const,
    /** `read_def_texture`'s result for one def ref and texture key. */
    texture: (defRef: string, textureKey: string) =>
      ["defs", "texture", defRef, textureKey] as const,
    /** `search_defs`'s result for one query and limit. */
    search: (query: string, limit: number) => ["defs", "search", query, limit] as const,
    /** `get_def_conflict_view`'s result for one finding key, filter, and patch scope (`null` for the profile). */
    conflictView: (key: string, filter: FieldRowFilterDto, patchId: string | null) =>
      ["defs", "conflictView", key, filter, patchId] as const,
  },
  /** The patch maker's own commands. */
  assignments: {
    /** `list_assignments`'s result. */
    list: () => ["assignments"] as const,
    /** `get_assignment`'s result for one project. */
    detail: (assignmentId: string) => ["assignments", "detail", assignmentId] as const,
    /** `get_assignment_coverage`'s result for one project's own section. */
    coverage: (assignmentId: string, section: string) =>
      ["assignments", "detail", assignmentId, "coverage", section] as const,
    /** `list_assignment_items`'s result for one item type, filter, and (when given) owning project. */
    items: (defType: string, filter: ListItemsFilterDto, assignmentId: string | null) =>
      ["assignments", "items", defType, filter, assignmentId] as const,
  },
};
