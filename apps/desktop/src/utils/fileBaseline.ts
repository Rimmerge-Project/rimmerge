import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { BaselineIssueDto } from "@/types/generated/BaselineIssueDto";
import type { MembershipMarkDto } from "@/types/generated/MembershipMarkDto";
import { assertNever } from "@/utils/assertNever";

/** What a row's membership mark says, as a short badge word and a tooltip; `null` for no mark. */
export type MembershipLabel = {
  readonly text: MessageDescriptor;
  readonly title: MessageDescriptor;
};

/**
 * The badge words for one {@link MembershipMarkDto}, exhaustive. `none` has no badge: a row
 * that matches the file says nothing.
 */
export function membershipLabel(mark: MembershipMarkDto): MembershipLabel | null {
  switch (mark) {
    case "none":
      return null;
    case "added":
      return {
        text: descriptor("mods.table.membership.added"),
        title: descriptor("mods.table.membership.addedTitle"),
      };
    case "removed":
      return {
        text: descriptor("mods.table.membership.removed"),
        title: descriptor("mods.table.membership.removedTitle"),
      };
    default:
      return assertNever(mark);
  }
}

/** The sentence for a file that could not be read; the markers keep the last list that worked. */
export function staleBaselineMessage(reason: BaselineIssueDto): MessageDescriptor {
  switch (reason) {
    case "missing":
      return descriptor("mods.fileChange.stale.missing");
    case "unreadable":
      return descriptor("mods.fileChange.stale.unreadable");
    case "malformed":
      return descriptor("mods.fileChange.stale.malformed");
    default:
      return assertNever(reason);
  }
}
