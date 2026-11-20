// The library organization (groups and mod colours): one wrapper per command.
// Organization is a separate store the sort path never reads, so none of these
// touches the load order, the working set or `ModsConfig.xml`.

import type { CreateGroupRequestDto } from "@/types/generated/CreateGroupRequestDto";
import type { DeleteGroupRequestDto } from "@/types/generated/DeleteGroupRequestDto";
import type { GroupMembershipCountDto } from "@/types/generated/GroupMembershipCountDto";
import type { GroupMembershipCountsRequestDto } from "@/types/generated/GroupMembershipCountsRequestDto";
import type { GroupMembersRequestDto } from "@/types/generated/GroupMembersRequestDto";
import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";
import type { GroupScopeRequestDto } from "@/types/generated/GroupScopeRequestDto";
import type { MoveGroupMembersRequestDto } from "@/types/generated/MoveGroupMembersRequestDto";
import type { OrganizationDto } from "@/types/generated/OrganizationDto";
import type { OrganizationEditOutcomeDto } from "@/types/generated/OrganizationEditOutcomeDto";
import type { RemoveUninstalledMembersRequestDto } from "@/types/generated/RemoveUninstalledMembersRequestDto";
import type { RenameGroupRequestDto } from "@/types/generated/RenameGroupRequestDto";
import type { ReparentGroupRequestDto } from "@/types/generated/ReparentGroupRequestDto";
import type { SetGroupColorRequestDto } from "@/types/generated/SetGroupColorRequestDto";
import type { SetModColorsRequestDto } from "@/types/generated/SetModColorsRequestDto";
import { call } from "./core";

/** The organization: whether it is usable, and the group tree. */
export function getOrganization(): Promise<OrganizationDto> {
  return call("get_organization");
}

/** Creates an empty group. */
export function createGroup(request: CreateGroupRequestDto): Promise<OrganizationEditOutcomeDto> {
  return call("create_group", { request });
}

/** Renames a group. */
export function renameGroup(request: RenameGroupRequestDto): Promise<OrganizationEditOutcomeDto> {
  return call("rename_group", { request });
}

/** Moves a group, with its subtree, under another group or to the top level. */
export function reparentGroup(
  request: ReparentGroupRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("reparent_group", { request });
}

/** Deletes a group. No mod is touched. */
export function deleteGroup(request: DeleteGroupRequestDto): Promise<OrganizationEditOutcomeDto> {
  return call("delete_group", { request });
}

/** Sets or clears a group's colour. */
export function setGroupColor(
  request: SetGroupColorRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("set_group_color", { request });
}

/** Adds mods to a group; their other memberships are untouched. */
export function addGroupMembers(
  request: GroupMembersRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("add_group_members", { request });
}

/** Removes mods from a group. */
export function removeGroupMembers(
  request: GroupMembersRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("remove_group_members", { request });
}

/** The explicit replace: moves mods from one group to another. */
export function moveGroupMembers(
  request: MoveGroupMembersRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("move_group_members", { request });
}

/** Sets or clears mods' own colour. */
export function setModColors(request: SetModColorsRequestDto): Promise<OrganizationEditOutcomeDto> {
  return call("set_mod_colors", { request });
}

/**
 * Removes the named mods from a group and its subgroups, in one save, but only those still not
 * installed: a mod installed again since the request was planned keeps its memberships.
 */
export function removeUninstalledMembers(
  request: RemoveUninstalledMembersRequestDto,
): Promise<OrganizationEditOutcomeDto> {
  return call("remove_uninstalled_members", { request });
}

/** A group's mods, subgroups included, resolved against the install. Read-only. */
export function planGroupScope(request: GroupScopeRequestDto): Promise<GroupScopePlanDto> {
  return call("plan_group_scope", { request });
}

/** How many of the given mods each group holds directly. Read-only. */
export function groupMembershipCounts(
  request: GroupMembershipCountsRequestDto,
): Promise<GroupMembershipCountDto[]> {
  return call("group_membership_counts", { request });
}

/** "Start over" after an unusable `organization.json`: keeps a copy, then saves an empty one. */
export function resetOrganization(): Promise<OrganizationDto> {
  return call("reset_organization");
}
