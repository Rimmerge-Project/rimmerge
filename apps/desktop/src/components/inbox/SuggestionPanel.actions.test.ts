// SuggestionPanel: patchWillFail evidence and the action buttons.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { buildDetail, mountPanel } from "@/components/inbox/SuggestionPanel.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";

beforeEach(() => installMockIpc({ list_mod_names: {} }));
afterEach(() => clearMocks());

describe("SuggestionPanel evidence: patchWillFail", () => {
  it("names the mod, target, and the removing mod for a removedBy cause", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "patchWillFail",
        modId: "a.mod",
        defKey: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        operation: 'Verse.PatchOperationReplace(Defs/ThingDef[defName="Wall"]/statBases)',
        leafXpath: 'Defs/ThingDef[defName="Wall"]/statBases',
        cause: { kind: "removedBy", modId: "b.mod" },
      }),
    );
    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("a.mod");
    expect(text).toContain('Defs/ThingDef[defName="Wall"]/statBases');
    expect(text).toContain("ThingDef/Wall");
    expect(text).toContain("b.mod");
    expect(text).toContain("which loads first, removes the target node");
  });

  it("names the injecting mod for a notYetInjected cause", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "patchWillFail",
        modId: "a.mod",
        defKey: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        operation: 'Verse.PatchOperationReplace(Defs/ThingDef[defName="Wall"]/statBases)',
        leafXpath: 'Defs/ThingDef[defName="Wall"]/statBases',
        cause: { kind: "notYetInjected", modId: "c.mod" },
      }),
    );
    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("c.mod");
    expect(text).toContain("injects");
  });

  it("explains a deadTarget cause with no reorder framing", () => {
    const wrapper = mountPanel(
      buildDetail({
        kind: "patchWillFail",
        modId: "a.mod",
        defKey: { defType: "ThingDef", defName: "Wall" },
        selector: "defName",
        operation: 'Verse.PatchOperationReplace(Defs/ThingDef[defName="Wall"]/statBases)',
        leafXpath: 'Defs/ThingDef[defName="Wall"]/statBases',
        cause: { kind: "deadTarget" },
      }),
    );
    const text = wrapper.get('[data-testid="finding-payload"]').text();
    expect(text).toContain("regardless of load order");
  });
});

describe("SuggestionPanel actions", () => {
  function mountMissingMod(overrides: Partial<ResolutionDetailDto> = {}) {
    return mountPanel(buildDetail({ kind: "missingMod", modId: "gone.mod" }, overrides));
  }

  it("emits accept, ignore, and pickAlternative from their buttons", async () => {
    const wrapper = mountMissingMod({
      suggestion: {
        action: { kind: "removeMod", modId: "gone.mod" },
        confidence: 85,
        rationale: "not found on disk",
        rationaleCode: { kind: "missingModNotInstalled" },
        alternatives: [
          {
            action: { kind: "ignore" },
            rationale: "keep it in ModsConfig",
            rationaleCode: { kind: "keepMissingModId" },
          },
        ],
      },
    });

    await wrapper.get('[data-testid="accept-button"]').trigger("click");
    expect(wrapper.emitted("accept")).toHaveLength(1);

    await wrapper.get('[data-testid="ignore-button"]').trigger("click");
    expect(wrapper.emitted("ignore")).toHaveLength(1);

    await wrapper.get('[data-testid="alternative-1"]').trigger("click");
    expect(wrapper.emitted("pickAlternative")).toEqual([[1]]);
  });

  it("only shows the revert button once a decision exists", () => {
    const undecided = mountMissingMod();
    expect(undecided.find('[data-testid="revert-button"]').exists()).toBe(false);

    const decided = mountMissingMod({ hasDecision: true });
    expect(decided.find('[data-testid="revert-button"]').exists()).toBe(true);
  });

  it("opens and saves a note", async () => {
    const wrapper = mountMissingMod();

    await wrapper.get('[data-testid="open-note-button"]').trigger("click");
    expect(wrapper.emitted("openNote")).toHaveLength(1);

    await wrapper.setProps({ noteOpen: true });
    await wrapper.get('[data-testid="decision-note-input"]').setValue("keep this");
    await wrapper.get('[data-testid="decision-note-save"]').trigger("click");

    expect(wrapper.emitted("saveNote")).toEqual([["keep this"]]);
  });
});
