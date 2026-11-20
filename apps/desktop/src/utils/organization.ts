import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { GroupFilterDto } from "@/types/generated/GroupFilterDto";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";

/** The twelve palette colours, in the order the picker lists them. */
export const COLOR_TOKENS: readonly ColorTokenDto[] = [
  "red",
  "orange",
  "amber",
  "lime",
  "green",
  "teal",
  "cyan",
  "blue",
  "indigo",
  "violet",
  "pink",
  "slate",
];

/**
 * Each colour's background utility. A literal table, not a built class name, so Tailwind sees
 * every class; the values come from the `--mark-*` tokens (light and dark, checked for contrast).
 */
export const COLOR_SWATCH_CLASS = {
  red: "bg-mark-red",
  orange: "bg-mark-orange",
  amber: "bg-mark-amber",
  lime: "bg-mark-lime",
  green: "bg-mark-green",
  teal: "bg-mark-teal",
  cyan: "bg-mark-cyan",
  blue: "bg-mark-blue",
  indigo: "bg-mark-indigo",
  violet: "bg-mark-violet",
  pink: "bg-mark-pink",
  slate: "bg-mark-slate",
} as const satisfies Record<ColorTokenDto, string>;

/** Each colour's name key: a colour is never the only cue, so every swatch has its word. */
export const COLOR_NAME_KEY = {
  red: "mods.colors.names.red",
  orange: "mods.colors.names.orange",
  amber: "mods.colors.names.amber",
  lime: "mods.colors.names.lime",
  green: "mods.colors.names.green",
  teal: "mods.colors.names.teal",
  cyan: "mods.colors.names.cyan",
  blue: "mods.colors.names.blue",
  indigo: "mods.colors.names.indigo",
  violet: "mods.colors.names.violet",
  pink: "mods.colors.names.pink",
  slate: "mods.colors.names.slate",
} as const satisfies Record<ColorTokenDto, string>;

/** What the two Mods lists are narrowed to: everything, the mods in no group, or one group. */
export type GroupScopeChoice =
  | { readonly kind: "all" }
  | { readonly kind: "ungrouped" }
  | { readonly kind: "group"; readonly id: number };

/** The scope showing every mod. */
export const ALL_MODS: GroupScopeChoice = { kind: "all" };

/** The list query's group filter for a scope; `null` is no filter. */
export function groupFilterOf(scope: GroupScopeChoice): GroupFilterDto | null {
  switch (scope.kind) {
    case "all":
      return null;
    case "ungrouped":
      return { kind: "ungrouped" };
    case "group":
      return { kind: "inScope", groupId: scope.id };
  }
}

/** Whether two scopes are the same choice. */
export function isSameScope(left: GroupScopeChoice, right: GroupScopeChoice): boolean {
  return left.kind === "group" && right.kind === "group"
    ? left.id === right.id
    : left.kind === right.kind;
}

/** The group with `id`, or `undefined` once it is gone. */
export function findNode(
  nodes: readonly GroupNodeDto[],
  id: number | null,
): GroupNodeDto | undefined {
  return id === null ? undefined : nodes.find((node) => node.id === id);
}

/** `id` and every group below it, from the tree's parent links. */
export function subtreeIds(nodes: readonly GroupNodeDto[], id: number): Set<number> {
  const found = new Set<number>([id]);
  // The tree arrives depth first, so a parent always precedes its children.
  for (const node of nodes) {
    if (node.parent !== null && found.has(node.parent)) {
      found.add(node.id);
    }
  }
  return found;
}

/** The groups from the top level down to `id`, the group included; empty for an unknown id. */
export function pathTo(nodes: readonly GroupNodeDto[], id: number): GroupNodeDto[] {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const path: GroupNodeDto[] = [];
  let current = byId.get(id);
  while (current !== undefined) {
    path.unshift(current);
    current = current.parent === null ? undefined : byId.get(current.parent);
  }
  return path;
}

/**
 * The groups to draw: every one whose ancestors are all expanded. With `shown` (the tree search
 * matches and their ancestors) only those groups are drawn, and they are drawn expanded so each
 * match path stays readable.
 */
export function visibleNodes(
  nodes: readonly GroupNodeDto[],
  expanded: ReadonlySet<number>,
  shown: ReadonlySet<number> | null,
): GroupNodeDto[] {
  const drawn = new Set<number>();
  const result: GroupNodeDto[] = [];
  for (const node of nodes) {
    if (shown !== null && !shown.has(node.id)) {
      continue;
    }
    const isParentOpen =
      node.parent === null ||
      (drawn.has(node.parent) && (shown !== null || expanded.has(node.parent)));
    if (isParentOpen) {
      drawn.add(node.id);
      result.push(node);
    }
  }
  return result;
}

/** What a tree search found: the groups whose name matches, and those plus their ancestors. */
export interface TreeSearchResult {
  readonly matches: ReadonlySet<number>;
  readonly shown: ReadonlySet<number>;
}

/** Case-insensitive name search over the tree; `null` for an empty query (no search). */
export function searchTree(nodes: readonly GroupNodeDto[], query: string): TreeSearchResult | null {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return null;
  }
  const matches = new Set<number>();
  const shown = new Set<number>();
  for (const node of nodes) {
    if (node.name.toLowerCase().includes(needle)) {
      matches.add(node.id);
      for (const ancestor of pathTo(nodes, node.id)) {
        shown.add(ancestor.id);
      }
    }
  }
  return { matches, shown };
}

/** Which organization dialog is open; `null` is none. Each variant carries only what its dialog needs. */
export type OrganizationDialog =
  | { readonly kind: "createGroup"; readonly parent: number | null }
  | { readonly kind: "renameGroup"; readonly id: number }
  | { readonly kind: "deleteGroup"; readonly id: number }
  | { readonly kind: "groupColor"; readonly id: number }
  | { readonly kind: "moveGroup"; readonly id: number }
  | { readonly kind: "removeUninstalled"; readonly id: number }
  | { readonly kind: "modColor"; readonly modId: string; readonly current: ColorTokenDto | null }
  | { readonly kind: "modGroups"; readonly modId: string; readonly groupIds: readonly number[] }
  | { readonly kind: "startOver" };
