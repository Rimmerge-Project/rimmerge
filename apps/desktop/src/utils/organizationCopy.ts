import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { CopyRefusalDto } from "@/types/generated/CopyRefusalDto";
import type { CopyReportDto } from "@/types/generated/CopyReportDto";
import type { OrganizationIssueDto } from "@/types/generated/OrganizationIssueDto";
import type { OrganizationRuleDto } from "@/types/generated/OrganizationRuleDto";
import { assertNever } from "@/utils/assertNever";
import { formatBytes } from "@/utils/format";

/** Why the other profile's file cannot be used, in a whole sentence about *that* profile. */
function unusableMessage(issue: OrganizationIssueDto): MessageDescriptor {
  switch (issue.kind) {
    case "unreadable":
      return descriptor("mods.groups.copy.refused.unusable.unreadable");
    case "tooLarge":
      return descriptor("mods.groups.copy.refused.unusable.tooLarge");
    case "malformed":
      return descriptor("mods.groups.copy.refused.unusable.malformed");
    case "newerVersion":
      return descriptor("mods.groups.copy.refused.unusable.newerVersion");
    case "invalid":
      return descriptor("mods.groups.copy.refused.unusable.invalid");
    default:
      return assertNever(issue);
  }
}

/** Which limit the result would break. */
function limitMessage(rule: OrganizationRuleDto): MessageDescriptor {
  switch (rule) {
    case "tooDeep":
      return descriptor("mods.groups.copy.refused.limit.tooDeep");
    case "tooManyGroups":
      return descriptor("mods.groups.copy.refused.limit.tooManyGroups");
    case "tooManyMembers":
      return descriptor("mods.groups.copy.refused.limit.tooManyMembers");
    case "tooManyColors":
      return descriptor("mods.groups.copy.refused.limit.tooManyColors");
    case "groupNotFound":
    case "wouldCreateCycle":
    case "nameTaken":
    case "sameGroup":
    case "danglingParent":
    case "duplicateGroupId":
    case "groupIdOutOfRange":
    case "nextIdBehind":
      return descriptor("mods.groups.copy.refused.limit.other");
    default:
      return assertNever(rule);
  }
}

/** The sentence for a refused copy. Every one ends "Nothing was changed" in its own text. */
export function copyRefusalMessage(reason: CopyRefusalDto): MessageDescriptor {
  switch (reason.kind) {
    case "sameProfile":
      return descriptor("mods.groups.copy.refused.sameProfile");
    case "sourceNotAFolder":
      return descriptor("mods.groups.copy.refused.notAFolder");
    case "sourceMissing":
      return descriptor("mods.groups.copy.refused.missing");
    case "sourceUnusable":
      return unusableMessage(reason.issue);
    case "sourceEmpty":
      return descriptor("mods.groups.copy.refused.empty");
    case "nothingToCopy":
      return descriptor("mods.groups.copy.refused.nothingToCopy");
    case "limit":
      return limitMessage(reason.rule);
    case "tooLargeToSave":
      return descriptor("mods.groups.copy.refused.tooLargeToSave", {
        bytes: formatBytes(reason.bytes),
        limit: formatBytes(reason.limit),
      });
    default:
      return assertNever(reason);
  }
}

/**
 * The folder that holds `path` (both separator styles; a trailing one is ignored), or `null` when
 * `path` has no parent to name. Where the folder picker starts: profile folders sit side by side.
 */
export function parentFolder(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut <= 0) {
    return null;
  }
  const parent = trimmed.slice(0, cut);
  // `C:` alone is a drive-relative path, not the drive's root.
  return /^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent;
}

/** One count line of a copy report: a whole, pluralised sentence. */
export type CopyLine = { readonly id: string; readonly message: MessageDescriptor };

function line(id: string, key: string, count: number): CopyLine | null {
  return count === 0 ? null : { id, message: descriptor(key, { count }, count) };
}

/** The counts a copy would change or changed, as sentences; zero counts are left out. */
export function copyLines(report: CopyReportDto): readonly CopyLine[] {
  const lines = [
    line("groupsAdded", "mods.groups.copy.line.groupsAdded", report.groupsAdded),
    line("groupsMerged", "mods.groups.copy.line.groupsMerged", report.groupsMerged),
    line("membershipsAdded", "mods.groups.copy.line.membershipsAdded", report.membershipsAdded),
    line(
      "membershipsAlready",
      "mods.groups.copy.line.membershipsAlready",
      report.membershipsAlready,
    ),
    line("modColorsAdded", "mods.groups.copy.line.modColorsAdded", report.modColorsAdded),
    line("modColorsKept", "mods.groups.copy.line.modColorsKept", report.modColorsKept),
    line(
      "groupColorsAdopted",
      "mods.groups.copy.line.groupColorsAdopted",
      report.groupColorsAdopted,
    ),
    line("notInstalled", "mods.groups.copy.line.notInstalled", report.notInstalledMods),
  ];
  return lines.filter((entry): entry is CopyLine => entry !== null);
}

/** What a replace throws away, as sentences; zero counts are left out. */
export function discardLines(report: CopyReportDto): readonly CopyLine[] {
  const lines = [
    line("discardGroups", "mods.groups.copy.discard.groups", report.discardedGroups),
    line("discardMemberships", "mods.groups.copy.discard.memberships", report.discardedMemberships),
    line("discardModColors", "mods.groups.copy.discard.modColors", report.discardedModColors),
  ];
  return lines.filter((entry): entry is CopyLine => entry !== null);
}
