// ApplyDialog: the hard-problem confirmation panel and its place before the running-game prompt.

import { useQueryCache } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import {
  DECIDED_INCOMPATIBLE_PAIR,
  dashboardFixture,
  EMPTY_PENDING_ACTIVE_CHANGES,
  EMPTY_RULE_SET,
  emptyMergeModFixture,
  flush,
  mountDialog,
  preflightFixture,
  UNANSWERED_MISSING_MOD,
} from "@/components/apply/ApplyDialog.test-support";
import { installMockIpc } from "@/services/ipc.mock";
import type { ApplyPreflightDto } from "@/types/generated/ApplyPreflightDto";
import type { ApplyRequestDto } from "@/types/generated/ApplyRequestDto";

const REPORT = {
  modsConfigPath: "C:/RimWorld/ModsConfig.xml",
  backupPath: "C:/RimWorld/ModsConfig.xml.bak-x",
  decisionsPath: "C:/Profile/rimmerge/decisions.json",
  rulesPath: "C:/Profile/rimmerge/rules.json",
  mergeModPath: null,
  mergeModBackupPath: null,
  skippedMerges: [],
};

function install(
  preflight: ApplyPreflightDto | (() => ApplyPreflightDto),
  calls: ApplyRequestDto[],
  runningUntilForced = false,
) {
  installMockIpc({
    list_rules: EMPTY_RULE_SET,
    list_mod_names: {},
    get_pending_active_changes: EMPTY_PENDING_ACTIVE_CHANGES,
    get_dashboard: dashboardFixture(0),
    get_apply_preflight: preflight,
    get_merge_mod: emptyMergeModFixture(),
    get_def_cache_carrier: { carrierModId: null },
    list_order: [],
    apply: (payload: unknown) => {
      const request = (payload as { request: ApplyRequestDto }).request;
      calls.push(request);
      if (runningUntilForced && !request.force) {
        throw { code: "rimworld_running", message: "RimWorldWin64.exe is running" };
      }
      return REPORT;
    },
  });
}

async function settle(wrapper: { vm: { $nextTick: () => Promise<void> } }): Promise<void> {
  await flush();
  await wrapper.vm.$nextTick();
}

describe("ApplyDialog hard-problem confirmation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("applies in one click when the order has no problems", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture(), calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    expect(calls).toHaveLength(1);
  });

  it("asks before writing when a problem is unanswered, and sends nothing yet", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([UNANSWERED_MISSING_MOD]), calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.get('[data-testid="apply-confirm-heading"]').text()).toBe("Apply anyway?");
    expect(wrapper.get('[data-testid="apply-confirm-problems"]').text()).toContain("gone.mod");
    expect(wrapper.get('[data-testid="apply-confirm"]').text()).toContain(
      "The current order has 1 problem to confirm before writing",
    );
    expect(calls).toEqual([]);
    expect(wrapper.find('[data-testid="apply-dialog-submit"]').exists()).toBe(false);
  });

  it("focuses Go back, and Go back returns to the form without applying", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([UNANSWERED_MISSING_MOD]), calls);
    const { wrapper } = mountDialog({ attachToBody: true });
    await settle(wrapper);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    const goBack = wrapper.get('[data-testid="apply-confirm-go-back"]').element;
    expect(document.activeElement).toBe(goBack);

    await wrapper.get('[data-testid="apply-confirm-go-back"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    const submit = wrapper.get('[data-testid="apply-dialog-submit"]').element;
    expect(document.activeElement).toBe(submit);
    expect(calls).toEqual([]);
    wrapper.unmount();
  });

  it("sends exactly one apply request after Apply anyway", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([UNANSWERED_MISSING_MOD]), calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-confirm-apply-anyway"]').trigger("click");
    await settle(wrapper);

    expect(calls).toEqual([
      { source: "current", writeModsConfig: true, writeMergeMod: false, force: false },
    ]);
  });

  it("lists decided problems apart and does not ask when every problem is decided", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([DECIDED_INCOMPATIBLE_PAIR]), calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);

    expect(wrapper.get('[data-testid="apply-dialog-problems-summary"]').text()).toBe(
      "1 problem, already decided",
    );

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    expect(calls).toHaveLength(1);
  });

  it("separates decided problems from unanswered ones inside the panel", async () => {
    install(preflightFixture([UNANSWERED_MISSING_MOD, DECIDED_INCOMPATIBLE_PAIR]), []);
    const { wrapper } = mountDialog();
    await settle(wrapper);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.get('[data-testid="apply-confirm-problems"]').text()).not.toContain("first.mod");
    expect(wrapper.get('[data-testid="apply-confirm-decided"]').text()).toContain("first.mod");
  });

  it("never asks with Write ModsConfig.xml unchecked, since nothing is written to ModsConfig.xml", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([UNANSWERED_MISSING_MOD]), calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);
    await wrapper
      .get('[data-testid="apply-dialog-write-modsconfig-checkbox"] input')
      .setValue(false);
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    expect(calls).toEqual([
      { source: "current", writeModsConfig: false, writeMergeMod: false, force: false },
    ]);
  });

  it("keeps the running-game prompt a separate step after confirming", async () => {
    const calls: ApplyRequestDto[] = [];
    install(preflightFixture([UNANSWERED_MISSING_MOD]), calls, true);
    const { wrapper } = mountDialog();
    await settle(wrapper);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-confirm-apply-anyway"]').trigger("click");
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="apply-dialog-write-anyway"]').exists()).toBe(true);

    await wrapper.get('[data-testid="apply-dialog-write-anyway"]').trigger("click");
    await settle(wrapper);

    expect(calls.map((call) => call.force)).toEqual([false, true]);
  });

  it("stops with an error, and writes nothing, when the preflight cannot be read", async () => {
    const calls: ApplyRequestDto[] = [];
    install(() => {
      throw { code: "internal", message: "background task panicked" };
    }, calls);
    const { wrapper } = mountDialog();
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(calls).toEqual([]);
    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
  });

  it("asks before writing when a decision since the dialog opened made the held preflight stale", async () => {
    const calls: ApplyRequestDto[] = [];
    let release: () => void = () => {};
    let fetches = 0;
    install(() => {
      fetches += 1;
      if (fetches === 1) {
        return preflightFixture();
      }
      return new Promise<ApplyPreflightDto>((resolve) => {
        release = () => resolve(preflightFixture([UNANSWERED_MISSING_MOD]));
      }) as unknown as ApplyPreflightDto;
    }, calls);
    const { wrapper, pinia } = mountDialog();
    await settle(wrapper);
    expect(fetches).toBe(1);

    void useQueryCache(pinia).invalidateQueries();
    await flush();
    expect(fetches).toBe(2);
    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    release();
    await settle(wrapper);

    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(true);
    expect(calls).toEqual([]);
  });

  it("throws, and writes nothing, when the preflight resolves with no data", async () => {
    const calls: ApplyRequestDto[] = [];
    install(null as unknown as ApplyPreflightDto, calls);
    const failures: unknown[] = [];
    const { wrapper } = mountDialog({ errorHandler: (error) => failures.push(error) });
    await settle(wrapper);

    await wrapper.get('[data-testid="apply-dialog-submit"]').trigger("click");
    await settle(wrapper);

    expect(calls).toEqual([]);
    expect(wrapper.find('[data-testid="apply-confirm"]').exists()).toBe(false);
    expect(failures).toHaveLength(1);
    expect(String(failures[0])).toContain("neither data nor an error");
  });
});
