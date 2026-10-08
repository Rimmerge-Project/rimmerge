import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { CorePlacementDto } from "@/types/generated/CorePlacementDto";
import type { ImportBlockedDto } from "@/types/generated/ImportBlockedDto";
import type { ImportedEntryDto } from "@/types/generated/ImportedEntryDto";
import type { ImportRejectionDto } from "@/types/generated/ImportRejectionDto";
import type { MissingKindDto } from "@/types/generated/MissingKindDto";
import type { SkippedEntryDto } from "@/types/generated/SkippedEntryDto";
import type { VersionCheckDto } from "@/types/generated/VersionCheckDto";
import { assertNever } from "@/utils/assertNever";
import { formatBytes } from "@/utils/format";

/**
 * The toast group of every export/import notice. It renders bottom-right so a toast never
 * covers the Load order header's Export and Import buttons, which the default top-right toasts
 * do for their whole lifetime.
 */
export const ORDER_SHARE_TOAST_GROUP = "order-share";

type NotInstalledEntry = Extract<ImportedEntryDto, { kind: "notInstalled" }>;
type ActivatedRow = { readonly id: string; readonly name: string };
type OtherCopyRow = Extract<ImportedEntryDto, { kind: "matchedOtherCopy" }>;
type DuplicateRow = Extract<ImportedEntryDto, { kind: "duplicate" }>;

/** The preview's entries sorted into the sections the dialog shows. */
export type GroupedEntries = {
  /** Mods the import turns on: a listed mod that was inactive, or an inactive other copy of it. */
  readonly activated: readonly ActivatedRow[];
  readonly notInstalled: readonly NotInstalledEntry[];
  readonly duplicates: readonly DuplicateRow[];
  readonly otherCopies: readonly OtherCopyRow[];
};

/**
 * Groups the entries by what the dialog shows. An entry that is already active changes
 * nothing and has no section; every other variant lands in exactly one (an other-copy match
 * that is inactive also counts as activated).
 */
export function groupImportEntries(entries: readonly ImportedEntryDto[]): GroupedEntries {
  const activated: ActivatedRow[] = [];
  const notInstalled: NotInstalledEntry[] = [];
  const duplicates: DuplicateRow[] = [];
  const otherCopies: OtherCopyRow[] = [];
  for (const entry of entries) {
    switch (entry.kind) {
      case "alreadyActive":
        break;
      case "activated":
        activated.push({ id: entry.id, name: entry.name });
        break;
      case "matchedOtherCopy":
        otherCopies.push(entry);
        if (entry.activation === "activated") {
          activated.push({ id: entry.installed, name: entry.name });
        }
        break;
      case "notInstalled":
        notInstalled.push(entry);
        break;
      case "duplicate":
        duplicates.push(entry);
        break;
      default:
        assertNever(entry);
    }
  }
  return { activated, notInstalled, duplicates, otherCopies };
}

/** The sentence for a document that cannot be imported. */
export function rejectionMessage(reason: ImportRejectionDto): MessageDescriptor {
  switch (reason.kind) {
    case "tooLarge":
      return descriptor("orderShare.rejected.tooLarge", { limit: formatBytes(reason.limitBytes) });
    case "tooManyEntries":
      return descriptor("orderShare.rejected.tooManyEntries", { limit: reason.limit });
    case "malformedXml":
      return descriptor("orderShare.rejected.malformedXml");
    case "dtdNotAllowed":
      return descriptor("orderShare.rejected.dtdNotAllowed");
    case "tooDeep":
      return descriptor("orderShare.rejected.tooDeep");
    case "unrecognizedFormat":
      return descriptor("orderShare.rejected.unrecognizedFormat");
    case "missingModList":
      return descriptor("orderShare.rejected.missingModList");
    case "noEntries":
      return descriptor("orderShare.rejected.noEntries");
    default:
      return assertNever(reason);
  }
}

/**
 * Why a listed mod cannot be installed from here, for a kind with no Workshop button;
 * `null` for a Workshop mod, whose row offers the button instead.
 */
export function missingKindNote(kind: MissingKindDto): MessageDescriptor | null {
  switch (kind.kind) {
    case "workshop":
      return null;
    case "dlc":
      return descriptor("orderShare.preview.missingKind.dlc");
    case "rimmergeMergeMod":
      return descriptor("orderShare.preview.missingKind.rimmergeMergeMod");
    case "noLink":
      return descriptor("orderShare.preview.missingKind.noLink");
    default:
      return assertNever(kind);
  }
}

/** The line explaining a part of the input that was left out. */
export function skippedMessage(skipped: SkippedEntryDto): MessageDescriptor {
  switch (skipped.kind) {
    case "notAnEntry":
      return descriptor("orderShare.preview.skipped.notAnEntry", { line: skipped.line });
    case "malformedId":
      return descriptor("orderShare.preview.skipped.malformedId", {
        position: skipped.position,
        text: skipped.text,
      });
    default:
      return assertNever(skipped);
  }
}

/** The version line, only when the list was made with another RimWorld version. */
export function versionNote(version: VersionCheckDto): MessageDescriptor | null {
  switch (version.kind) {
    case "unknown":
    case "same":
      return null;
    case "differs":
      return descriptor("orderShare.preview.versionDiffers", {
        listed: version.listed,
        game: version.game,
      });
    default:
      return assertNever(version);
  }
}

/** The note about where Core ends up; `null` when the list placed it itself or Core is missing. */
export function corePlacementNote(core: CorePlacementDto): MessageDescriptor | null {
  switch (core) {
    case "listed":
      return null;
    case "addedFirst":
      return descriptor("orderShare.preview.coreAdded");
    case "missing":
      // The blocker line ("Core isn't installed...") already says it.
      return null;
    default:
      return assertNever(core);
  }
}

/** Why "Use this order" is unavailable, from the backend's reason. */
export function importBlockedMessage(
  blocked: ImportBlockedDto,
  labelOf: (id: string) => string,
): MessageDescriptor {
  switch (blocked.kind) {
    case "coreMissing":
      return descriptor("orderShare.preview.blocked.coreMissing");
    case "nothingInstalled":
      return descriptor("orderShare.preview.blocked.nothingInstalled");
    case "tooMany":
      return descriptor("orderShare.preview.blocked.tooMany", { limit: blocked.limit });
    case "unknown":
      return descriptor("orderShare.preview.blocked.unknown", { id: labelOf(blocked.id) });
    case "duplicate":
      return descriptor("orderShare.preview.blocked.duplicate", { id: labelOf(blocked.id) });
    default:
      return assertNever(blocked);
  }
}

const MARKDOWN_SPECIALS = /[*_~`|\\]/g;
const BRACKETS = /[[\]<>]/g;
const MENTIONS = /@/g;
/** Written after every `@` so `@everyone` is not a mention; `MENTION_BREAK` in the text codec. */
const MENTION_BREAK = "\u2060";
// biome-ignore lint/suspicious/noControlCharactersInRegex: stripping control characters is the point
const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f]/g;
const WORKSHOP_ITEM_URL = "https://steamcommunity.com/sharedfiles/filedetails/?id=";

/**
 * A sender's name made safe to sit in the shared text: control characters dropped, brackets
 * and angle brackets turned to parentheses (so a name cannot fake an id or a link) and chat
 * markdown escaped, and a word joiner put after every `@` so a pasted `@everyone` pings no
 * one. Mirrors `escape_name` in `rim-session`'s text codec, which writes the same format; the
 * shared fixture `crates/rim-session/tests/fixtures/client_text_parity.json` is asserted by both
 * this function's vitest and `render_text`'s Rust test, so changing one escaper alone fails one.
 */
function textSafeName(name: string): string {
  return name
    .replace(CONTROL_CHARACTERS, "")
    .replace(BRACKETS, (bracket) => (bracket === "[" || bracket === "<" ? "(" : ")"))
    .replace(MARKDOWN_SPECIALS, (special) => `\\${special}`)
    .replace(MENTIONS, `@${MENTION_BREAK}`);
}

/**
 * The "Copy the missing list" text: the not-installed rows in the shareable text format
 * (`N. Name [id] <workshop url>`), so a friend can be asked for them. A mod Rimmerge made on
 * the sender's machine is left out, since nobody can install it. Display formatting only;
 * the URL is text for the clipboard and is never opened from here.
 */
export function missingListText(rows: readonly NotInstalledEntry[]): string {
  return rows
    .filter((row) => row.missing.kind !== "rimmergeMergeMod")
    .map((row, index) => {
      const name = row.name === null ? "" : `${textSafeName(row.name)} `;
      const link =
        row.missing.kind === "workshop"
          ? ` <${WORKSHOP_ITEM_URL}${String(row.missing.workshopId)}>`
          : "";
      return `${index + 1}. ${name}[${row.listed}]${link}`;
    })
    .join("\n");
}
