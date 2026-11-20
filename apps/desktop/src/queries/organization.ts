import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { awaitInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import {
  addGroupMembers,
  createGroup,
  deleteGroup,
  getOrganization,
  moveGroupMembers,
  planGroupScope,
  removeGroupMembers,
  removeUninstalledMembers,
  renameGroup,
  reparentGroup,
  resetOrganization,
  setGroupColor,
  setModColors,
} from "@/services/ipc";
import type { CreateGroupRequestDto } from "@/types/generated/CreateGroupRequestDto";
import type { DeleteGroupRequestDto } from "@/types/generated/DeleteGroupRequestDto";
import type { GroupMembersRequestDto } from "@/types/generated/GroupMembersRequestDto";
import type { MoveGroupMembersRequestDto } from "@/types/generated/MoveGroupMembersRequestDto";
import type { OrganizationEditOutcomeDto } from "@/types/generated/OrganizationEditOutcomeDto";
import type { RemoveUninstalledMembersRequestDto } from "@/types/generated/RemoveUninstalledMembersRequestDto";
import type { RenameGroupRequestDto } from "@/types/generated/RenameGroupRequestDto";
import type { ReparentGroupRequestDto } from "@/types/generated/ReparentGroupRequestDto";
import type { SetGroupColorRequestDto } from "@/types/generated/SetGroupColorRequestDto";
import type { SetModColorsRequestDto } from "@/types/generated/SetModColorsRequestDto";

/** The organization: its availability and the group tree. */
export function useOrganizationQuery() {
  return useQuery({
    key: () => queryKeys.organization(),
    query: () => getOrganization(),
  });
}

/**
 * One group's scope resolved against the install: how many mods (subgroups included) and how
 * many of them are not installed. Disabled while `groupId` is `null`. Its key sits under the
 * organization key, so an organization change refetches it.
 */
export function useGroupScopePlanQuery(groupId: MaybeRefOrGetter<number | null>) {
  return useQuery({
    key: () => [...queryKeys.organization(), "scope", toValue(groupId)] as const,
    query: () => planGroupScope({ id: toValue(groupId) as number }),
    enabled: () => toValue(groupId) !== null,
  });
}

/**
 * Every organization edit, each a mutation that refetches the organization and the two lists
 * before it resolves (the `organizationChanged` event refetches the same, so the page never
 * waits on the event round trip). None changes the load order or the working set, so nothing
 * else is invalidated.
 */
export function useOrganizationMutations() {
  const queryCache = useQueryCache();
  const refetchOrganization = () =>
    awaitInvalidation(
      queryCache,
      { key: [...queryKeys.organization()] },
      { key: [...queryKeys.workingModsAll()] },
      { key: [...queryKeys.inactiveModsAll()] },
    );
  const edit = <Request>(run: (request: Request) => Promise<OrganizationEditOutcomeDto>) => ({
    mutation: run,
    onSuccess: refetchOrganization,
  });
  return {
    createGroup: useMutation(edit((request: CreateGroupRequestDto) => createGroup(request))),
    renameGroup: useMutation(edit((request: RenameGroupRequestDto) => renameGroup(request))),
    reparentGroup: useMutation(edit((request: ReparentGroupRequestDto) => reparentGroup(request))),
    deleteGroup: useMutation(edit((request: DeleteGroupRequestDto) => deleteGroup(request))),
    setGroupColor: useMutation(edit((request: SetGroupColorRequestDto) => setGroupColor(request))),
    addMembers: useMutation(edit((request: GroupMembersRequestDto) => addGroupMembers(request))),
    removeMembers: useMutation(
      edit((request: GroupMembersRequestDto) => removeGroupMembers(request)),
    ),
    moveMembers: useMutation(
      edit((request: MoveGroupMembersRequestDto) => moveGroupMembers(request)),
    ),
    setModColors: useMutation(edit((request: SetModColorsRequestDto) => setModColors(request))),
    removeUninstalled: useMutation(
      edit((request: RemoveUninstalledMembersRequestDto) => removeUninstalledMembers(request)),
    ),
    resetOrganization: useMutation({
      mutation: () => resetOrganization(),
      onSuccess: refetchOrganization,
    }),
  };
}
