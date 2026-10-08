import { describe, expect, it } from "vitest";
import type { ImportedEntryDto } from "@/types/generated/ImportedEntryDto";
import {
  corePlacementNote,
  groupImportEntries,
  missingKindNote,
  missingListText,
  rejectionMessage,
  skippedMessage,
  versionNote,
} from "@/utils/orderShare";
import parityFixture from "../../../../crates/rim-session/tests/fixtures/client_text_parity.json";

type NotInstalled = Extract<ImportedEntryDto, { kind: "notInstalled" }>;

function notInstalled(overrides: Partial<NotInstalled> = {}): NotInstalled {
  return {
    kind: "notInstalled",
    listed: "example.missing",
    name: "Example Missing",
    missing: { kind: "noLink" },
    ...overrides,
  };
}

describe("groupImportEntries", () => {
  it("puts every variant in its own section and leaves an already-active mod out", () => {
    const entries: ImportedEntryDto[] = [
      { kind: "alreadyActive", id: "example.a", name: "A" },
      { kind: "activated", id: "example.b", name: "B" },
      {
        kind: "matchedOtherCopy",
        listed: "example.c",
        installed: "example.c_steam",
        name: "C",
        activation: "alreadyActive",
      },
      notInstalled(),
      { kind: "duplicate", id: "example.a", firstPosition: 1 },
    ];

    const groups = groupImportEntries(entries);

    expect(groups.activated).toEqual([{ id: "example.b", name: "B" }]);
    expect(groups.otherCopies).toHaveLength(1);
    expect(groups.notInstalled).toHaveLength(1);
    expect(groups.duplicates).toHaveLength(1);
  });

  it("counts an inactive other copy as activated under the id that will be used", () => {
    const groups = groupImportEntries([
      {
        kind: "matchedOtherCopy",
        listed: "example.c",
        installed: "example.c_steam",
        name: "C",
        activation: "activated",
      },
    ]);

    expect(groups.activated).toEqual([{ id: "example.c_steam", name: "C" }]);
    expect(groups.otherCopies).toHaveLength(1);
  });
});

describe("rejectionMessage", () => {
  it("words every rejection with its own key and carries the limits", () => {
    expect(rejectionMessage({ kind: "tooLarge", limitBytes: 4 * 1024 * 1024 })).toEqual({
      key: "orderShare.rejected.tooLarge",
      params: { limit: "4.00 MiB" },
    });
    expect(rejectionMessage({ kind: "tooManyEntries", limit: 5000 })).toEqual({
      key: "orderShare.rejected.tooManyEntries",
      params: { limit: 5000 },
    });
    const keys = [
      rejectionMessage({ kind: "malformedXml" }),
      rejectionMessage({ kind: "dtdNotAllowed" }),
      rejectionMessage({ kind: "tooDeep" }),
      rejectionMessage({ kind: "unrecognizedFormat" }),
      rejectionMessage({ kind: "missingModList" }),
      rejectionMessage({ kind: "noEntries" }),
    ].map((message) => message.key);
    expect(keys).toEqual([
      "orderShare.rejected.malformedXml",
      "orderShare.rejected.dtdNotAllowed",
      "orderShare.rejected.tooDeep",
      "orderShare.rejected.unrecognizedFormat",
      "orderShare.rejected.missingModList",
      "orderShare.rejected.noEntries",
    ]);
  });
});

describe("the small notes", () => {
  it("has no note for a Workshop mod, whose row offers the button instead", () => {
    expect(missingKindNote({ kind: "workshop", workshopId: 1234567890 })).toBeNull();
    expect(missingKindNote({ kind: "dlc" })?.key).toBe("orderShare.preview.missingKind.dlc");
    expect(missingKindNote({ kind: "rimmergeMergeMod" })?.key).toBe(
      "orderShare.preview.missingKind.rimmergeMergeMod",
    );
    expect(missingKindNote({ kind: "noLink" })?.key).toBe("orderShare.preview.missingKind.noLink");
  });

  it("words a skipped part with the line or position it carries", () => {
    expect(skippedMessage({ kind: "notAnEntry", line: 7 })).toEqual({
      key: "orderShare.preview.skipped.notAnEntry",
      params: { line: 7 },
    });
    expect(skippedMessage({ kind: "malformedId", position: 3, text: "not an id" })).toEqual({
      key: "orderShare.preview.skipped.malformedId",
      params: { position: 3, text: "not an id" },
    });
  });

  it("mentions the version only when it differs", () => {
    expect(versionNote({ kind: "unknown" })).toBeNull();
    expect(versionNote({ kind: "same" })).toBeNull();
    expect(versionNote({ kind: "differs", listed: "1.5.4409", game: "1.6.4871" })).toEqual({
      key: "orderShare.preview.versionDiffers",
      params: { listed: "1.5.4409", game: "1.6.4871" },
    });
  });

  it("notes Core only when the list did not place it", () => {
    expect(corePlacementNote("listed")).toBeNull();
    expect(corePlacementNote("addedFirst")?.key).toBe("orderShare.preview.coreAdded");
    // The blocked line says Core is missing; a second note would only repeat it.
    expect(corePlacementNote("missing")).toBeNull();
  });
});

describe("missingListText", () => {
  it("renders the shareable text format, with a Workshop link in angle brackets", () => {
    const text = missingListText([
      notInstalled({
        listed: "example.framework",
        name: "Example Framework",
        missing: { kind: "workshop", workshopId: 1234567890 },
      }),
      notInstalled({ listed: "someone.localmod", name: null }),
    ]);

    expect(text).toBe(
      [
        "1. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>",
        "2. [someone.localmod]",
      ].join("\n"),
    );
  });

  it("keeps a sender's name from faking an id or a link, and escapes chat markdown", () => {
    const text = missingListText([
      notInstalled({ listed: "example.real", name: "*Bold* [evil.id] <https://x> _it_" }),
    ]);

    expect(text).toBe("1. \\*Bold\\* (evil.id) (https://x) \\_it\\_ [example.real]");
  });

  it("breaks an @ mention with a word joiner so a pasted list pings no one", () => {
    const text = missingListText([
      notInstalled({ listed: "example.real", name: "@everyone @here" }),
    ]);

    expect(text).toBe("1. @\u2060everyone @\u2060here [example.real]");
  });

  it("writes only the id for a row with no name, as the Rust text codec does", () => {
    expect(missingListText([notInstalled({ listed: "example.real", name: null })])).toBe(
      "1. [example.real]",
    );
  });

  // Parity fixture: `render_matches_the_desktop_client_parity_fixture` in
  // `crates/rim-session/src/mod_list/text.rs` reads the same JSON file and asserts that
  // `render_text` produces `expected` (the codec adds the trailing newline this side omits).
  it("renders the same text as the Rust codec for the shared parity fixture", () => {
    const rows = parityFixture.entries.map((entry) =>
      notInstalled({
        listed: entry.id,
        name: entry.name,
        missing:
          entry.workshopId === null
            ? { kind: "noLink" }
            : { kind: "workshop", workshopId: entry.workshopId },
      }),
    );

    expect(`${missingListText(rows)}\n`).toBe(parityFixture.expected);
  });

  it("leaves out a mod Rimmerge made on the sender's computer and numbers the rest", () => {
    const text = missingListText([
      notInstalled({ listed: "rimmerge.merge.abc", missing: { kind: "rimmergeMergeMod" } }),
      notInstalled({ listed: "example.real", name: null }),
    ]);

    expect(text).toBe("1. [example.real]");
  });
});
