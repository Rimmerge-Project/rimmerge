import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import ImportPreviewDialog from "@/components/order/ImportPreviewDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { ImportedEntryDto } from "@/types/generated/ImportedEntryDto";
import type { OrderImportOutcomeDto } from "@/types/generated/OrderImportOutcomeDto";
import type { OrderImportPreviewDto } from "@/types/generated/OrderImportPreviewDto";

const ALL_ENTRIES: ImportedEntryDto[] = [
  { kind: "alreadyActive", id: "example.kept", name: "Kept Mod" },
  { kind: "activated", id: "example.fresh", name: "Fresh Mod" },
  {
    kind: "matchedOtherCopy",
    listed: "example.twin",
    installed: "example.twin_steam",
    name: "Twin Mod",
    activation: "alreadyActive",
  },
  {
    kind: "notInstalled",
    listed: "example.framework",
    name: "Example Framework",
    missing: { kind: "workshop", workshopId: 1234567890 },
  },
  { kind: "notInstalled", listed: "ludeon.rimworld.royalty", name: null, missing: { kind: "dlc" } },
  {
    kind: "notInstalled",
    listed: "rimmerge.merge.abc123",
    name: null,
    missing: { kind: "rimmergeMergeMod" },
  },
  { kind: "notInstalled", listed: "someone.localmod", name: null, missing: { kind: "noLink" } },
  { kind: "duplicate", id: "example.kept", firstPosition: 1 },
];

function previewFixture(overrides: Partial<OrderImportPreviewDto> = {}): OrderImportPreviewDto {
  return {
    listed: 8,
    order: ["ludeon.rimworld", "example.kept", "example.fresh", "example.twin_steam"],
    entries: ALL_ENTRIES,
    deactivated: [{ modId: "example.dropped", name: "Dropped Mod" }],
    moved: 2,
    core: "listed",
    version: { kind: "same" },
    replacesPendingChanges: false,
    skipped: [
      { kind: "notAnEntry", line: 4 },
      { kind: "malformedId", position: 5, text: "not an id" },
    ],
    omittedSkipped: 0,
    importBlocked: null,
    ...overrides,
  };
}

function ready(overrides: Partial<OrderImportPreviewDto> = {}): OrderImportOutcomeDto {
  return { kind: "ready", preview: previewFixture(overrides) };
}

function mountDialog(
  outcome: OrderImportOutcomeDto | null,
  props: { isImporting?: boolean; replacesUnwritten?: boolean } = {},
) {
  return mount(ImportPreviewDialog, {
    props: {
      visible: true,
      outcome,
      isImporting: props.isImporting ?? false,
      replacesUnwritten: props.replacesUnwritten ?? false,
    },
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
}

describe("ImportPreviewDialog", () => {
  beforeEach(() => {
    installMockIpc({ list_mod_names: {} });
  });
  afterEach(() => {
    clearMocks();
    localStorage.removeItem("rimmerge.modLabelMode");
    document.body.innerHTML = "";
  });

  it("renders every entry variant in its own section", async () => {
    const wrapper = mountDialog(ready());
    await flush();

    expect(wrapper.get('[data-testid="import-preview-summary"]').text()).toBe("8 mods in the list");
    expect(wrapper.get('[data-testid="import-preview-activated"]').text()).toContain("Fresh Mod");
    expect(wrapper.get('[data-testid="import-preview-deactivated"]').text()).toContain(
      "Dropped Mod",
    );
    expect(wrapper.get('[data-testid="import-preview-duplicates"]').text()).toContain(
      "example.kept — the first position (#1) is used.",
    );
    expect(wrapper.get('[data-testid="import-preview-other-copies"]').text()).toContain(
      "example.twin → your copy Twin Mod",
    );
    expect(wrapper.findAll('[data-testid="import-preview-missing-row"]')).toHaveLength(4);
  });

  it("words each kind of missing mod and offers the Workshop button only for a Workshop mod", async () => {
    const wrapper = mountDialog(ready());
    await flush();

    const missing = wrapper.get('[data-testid="import-preview-missing"]').text();

    expect(missing).toContain("A DLC you don't have installed.");
    expect(missing).toContain(
      "The sender's merge mod, made by Rimmerge on their computer; you don't need it.",
    );
    expect(missing).toContain("The list has no Workshop link for this mod.");
    expect(wrapper.findAll('[data-testid^="import-preview-workshop-"]')).toHaveLength(1);
  });

  it("sends the Workshop id when the button is clicked and names the mod for assistive tech", async () => {
    const wrapper = mountDialog(ready());
    await flush();
    const button = wrapper.get('[data-testid="import-preview-workshop-1234567890"]');

    await button.trigger("click");

    expect(wrapper.emitted("openWorkshop")).toEqual([[1234567890]]);
    expect(button.attributes("aria-label")).toBe("Open Example Framework on Steam Workshop");
  });

  it("renders both skipped variants and the count that was not listed", async () => {
    const wrapper = mountDialog(ready({ omittedSkipped: 3 }));
    await flush();

    const skipped = wrapper.get('[data-testid="import-preview-skipped"]').text();

    expect(skipped).toContain("Lines not understood (5)");
    expect(skipped).toContain("Line 4: not a mod entry.");
    expect(skipped).toContain("Entry 5: “not an id” isn't a valid package id.");
    expect(skipped).toContain("3 more entries not shown.");
  });

  it("notes a differing version, Core put first and a missing Core", async () => {
    const differs = mountDialog(
      ready({
        version: { kind: "differs", listed: "1.5.4409", game: "1.6.4871" },
        core: "addedFirst",
      }),
    );
    await flush();

    expect(differs.get('[data-testid="import-preview-version"]').text()).toBe(
      "Made with RimWorld 1.5.4409; you have 1.6.4871.",
    );
    expect(differs.get('[data-testid="import-preview-core"]').text()).toBe(
      "Core wasn't in the list; it stays first.",
    );

    const missingCore = mountDialog(
      ready({ core: "missing", importBlocked: { kind: "coreMissing" } }),
    );
    await flush();

    expect(missingCore.find('[data-testid="import-preview-core"]').exists()).toBe(false);
    expect(missingCore.get('[data-testid="import-preview-blocked"]').text()).toBe(
      "Core isn't installed, so this list can't be imported.",
    );
  });

  it("warns about unrescanned edits and about an order not yet written", async () => {
    const wrapper = mountDialog(ready({ replacesPendingChanges: true }), {
      replacesUnwritten: true,
    });
    await flush();

    expect(wrapper.find('[data-testid="import-preview-replaces-pending"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="import-preview-replaces-unwritten"]').text()).toContain(
      "This import replaces your Current order",
    );

    const quiet = mountDialog(ready());
    await flush();

    expect(quiet.find('[data-testid="import-preview-replaces-pending"]').exists()).toBe(false);
    expect(quiet.find('[data-testid="import-preview-replaces-unwritten"]').exists()).toBe(false);
  });

  it("confirms with the preview's order when nothing blocks the import", async () => {
    const wrapper = mountDialog(ready());
    await flush();

    await wrapper.get('[data-testid="import-preview-confirm"]').trigger("click");

    expect(wrapper.emitted("confirm")).toEqual([
      [["ludeon.rimworld", "example.kept", "example.fresh", "example.twin_steam"]],
    ]);
  });

  it.each([
    [{ kind: "coreMissing" }, "Core isn't installed, so this list can't be imported."],
    [
      { kind: "nothingInstalled" },
      "None of the listed mods are installed, so there is nothing to import.",
    ],
    [
      { kind: "tooMany", limit: 5002 },
      "This order would hold too many mods; an import can hold at most 5002.",
    ],
    [{ kind: "unknown", id: "example.gone" }, "example.gone isn't an installed mod."],
    [
      { kind: "duplicate", id: "example.kept" },
      "example.kept appears more than once in the order.",
    ],
  ] as const)(
    "disables Use this order and says why when the backend blocks it as %j",
    async (importBlocked, sentence) => {
      const wrapper = mountDialog(ready({ importBlocked }));
      await flush();

      const confirm = wrapper.get('[data-testid="import-preview-confirm"]');

      expect(wrapper.get('[data-testid="import-preview-blocked"]').text()).toBe(sentence);
      expect(confirm.attributes("disabled")).toBeDefined();
      await confirm.trigger("click");
      expect(wrapper.emitted("confirm")).toBeUndefined();
    },
  );

  it("blocks with a reason even when every listed mod is installed, and never invents one", async () => {
    const allInstalled = mountDialog(
      ready({ entries: [ALL_ENTRIES[0] as ImportedEntryDto], importBlocked: null }),
    );
    await flush();

    expect(allInstalled.find('[data-testid="import-preview-blocked"]').exists()).toBe(false);
    expect(
      allInstalled.get('[data-testid="import-preview-confirm"]').attributes("disabled"),
    ).toBeUndefined();

    const onlyMissing = mountDialog(
      ready({
        entries: [ALL_ENTRIES[3] as ImportedEntryDto],
        order: ["ludeon.rimworld"],
        deactivated: [],
        importBlocked: { kind: "nothingInstalled" },
      }),
    );
    await flush();

    expect(
      onlyMissing.get('[data-testid="import-preview-confirm"]').attributes("disabled"),
    ).toBeDefined();
  });

  it("shows a sender's mod by its id when the shell is in id mode, and by name otherwise", async () => {
    const byName = mountDialog(ready());
    await flush();

    expect(byName.get('[data-testid="import-preview-missing"]').text()).toContain(
      "Example Framework",
    );

    localStorage.setItem("rimmerge.modLabelMode", "id");
    const byId = mountDialog(ready());
    await flush();

    const missing = byId.get('[data-testid="import-preview-missing"]').text();
    expect(missing).toContain("example.framework");
    expect(missing).not.toContain("Example Framework");
    expect(
      byId.get('[data-testid="import-preview-workshop-1234567890"]').attributes("aria-label"),
    ).toBe("Open example.framework on Steam Workshop");
  });

  it("copies the missing list as shareable text", async () => {
    const wrapper = mountDialog(ready());
    await flush();

    await wrapper.get('[data-testid="import-preview-copy-missing"]').trigger("click");

    const [text] = wrapper.emitted("copyMissing")?.[0] ?? [];
    expect(text).toBe(
      [
        "1. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>",
        "2. [ludeon.rimworld.royalty]",
        "3. [someone.localmod]",
      ].join("\n"),
    );
  });

  it("cannot be cancelled or closed while the import runs", async () => {
    const wrapper = mountDialog(ready(), { isImporting: true });
    await flush();

    expect(
      wrapper.get('[data-testid="import-preview-cancel"]').attributes("disabled"),
    ).toBeDefined();
    expect(
      wrapper.get('[data-testid="import-preview-confirm"]').attributes("disabled"),
    ).toBeDefined();
    expect(wrapper.find(".p-dialog-close-button").exists()).toBe(false);
  });

  it.each([
    [{ kind: "tooLarge", limitBytes: 4 * 1024 * 1024 }, "The file is larger than 4.00 MiB."],
    [{ kind: "tooManyEntries", limit: 5000 }, "The list has more than 5000 entries."],
    [{ kind: "malformedXml" }, "The file isn't valid XML."],
    [{ kind: "dtdNotAllowed" }, "The file declares a DTD, which mod lists never do."],
    [{ kind: "tooDeep" }, "The file is nested too deeply to be a mod list."],
    [
      { kind: "unrecognizedFormat" },
      "This isn't a RimWorld mod list (.rml), a ModsConfig.xml, or a text list.",
    ],
    [{ kind: "missingModList" }, "The file has no mod list in it."],
    [{ kind: "noEntries" }, "No package ids were found."],
  ] as const)("shows %j as its reason with only a Close button", async (reason, sentence) => {
    const wrapper = mountDialog({ kind: "rejected", reason });
    await flush();

    expect(wrapper.get('[data-testid="import-preview-rejected-reason"]').text()).toBe(sentence);
    expect(wrapper.find('[data-testid="import-preview-confirm"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="import-preview-close"]').exists()).toBe(true);
  });
});
