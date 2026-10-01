import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent } from "vue";

import { usePatchFindingQuery } from "@/queries/patches";
import { installMockIpc } from "@/services/ipc.mock";
import { asFindingKey } from "@/types/brands";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";

const PATCH_ID = "abc123def456";
const KEY = asFindingKey("def_override:ThingDef/Wall:[a.mod,b.mod]");

function detail(): ResolutionDetailDto {
  return {
    key: KEY,
    finding: {
      kind: "defOverride",
      key: { defType: "ThingDef", defName: "Wall" },
      owners: ["a.mod", "b.mod"],
      winner: "b.mod",
    },
    suggestion: {
      action: { kind: "ignore" },
      confidence: 50,
      rationale: "Not addressed by this patch; load order decides as today.",
      rationaleCode: { kind: "notAddressedByPatch" },
      alternatives: [],
    },
    status: "needsInput",
    effective: { kind: "ignore" },
    note: null,
    hasDecision: false,
    resolvedBySuggested: null,
    mergeState: null,
    structuralGuardField: null,
    scope: { kind: "full" },
    defRef: "ThingDef/Wall",
  };
}

function mountHarness(getPatchFinding: () => unknown) {
  installMockIpc({ get_patch_finding: getPatchFinding });

  let composable!: ReturnType<typeof usePatchFindingQuery>;
  const Harness = defineComponent({
    setup() {
      composable = usePatchFindingQuery(
        () => PATCH_ID,
        () => KEY,
      );
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });

  return {
    wrapper,
    get query() {
      return composable;
    },
  };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("usePatchFindingQuery", () => {
  afterEach(() => {
    clearMocks();
  });

  it("resolves the finding's detail on success", async () => {
    const { query, wrapper } = mountHarness(() => detail());
    await flush();
    await wrapper.vm.$nextTick();

    expect(query.data.value).toEqual(detail());
    expect(query.error.value).toBeNull();
    wrapper.unmount();
  });

  it("maps a finding_not_found rejection to null data, not a query error", async () => {
    const { query, wrapper } = mountHarness(() => {
      throw { code: "finding_not_found", message: "no such finding in this patch's scope" };
    });
    await flush();
    await wrapper.vm.$nextTick();

    expect(query.data.value).toBeNull();
    expect(query.error.value).toBeNull();
    wrapper.unmount();
  });

  it("still surfaces any other error code as a real query error", async () => {
    const { query, wrapper } = mountHarness(() => {
      throw { code: "profile_io_failed", message: "disk is full" };
    });
    await flush();
    await wrapper.vm.$nextTick();

    expect(query.error.value).not.toBeNull();
    wrapper.unmount();
  });
});
