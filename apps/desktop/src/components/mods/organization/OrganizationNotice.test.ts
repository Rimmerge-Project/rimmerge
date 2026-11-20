import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";

import OrganizationNotice from "@/components/mods/organization/OrganizationNotice.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { OrganizationDto } from "@/types/generated/OrganizationDto";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

let wrappers: ReturnType<typeof mount>[] = [];
const rescanCalls: unknown[] = [];

function mountNotice(
  organization: OrganizationDto | undefined,
  rescanFails: { code: string; message: string } | null = null,
) {
  rescanCalls.length = 0;
  installMockIpc({
    rescan_project: () => {
      rescanCalls.push(true);
      if (rescanFails !== null) {
        throw rescanFails;
      }
      return { selected: "current", warnings: [], ruleWarnings: [] };
    },
  });
  const wrapper = mount(OrganizationNotice, {
    props: { organization },
    global: { plugins: [createPinia(), PiniaColada] },
  });
  wrappers.push(wrapper);
  return wrapper;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

describe("OrganizationNotice", () => {
  it("renders nothing while groups work or are not known yet", () => {
    expect(
      mountNotice({ kind: "loaded", groups: [], coloredModCount: 0 })
        .find('[data-testid="organization-notice"]')
        .exists(),
    ).toBe(false);
    expect(mountNotice(undefined).find('[data-testid="organization-notice"]').exists()).toBe(false);
  });

  it("offers only Try again for a file that could not be read, then Start over after a retry", async () => {
    const wrapper = mountNotice({ kind: "recovered", issue: { kind: "unreadable" } });

    expect(wrapper.text()).toContain("couldn't read your groups and colours");
    expect(wrapper.find('[data-testid="organization-start-over"]').exists()).toBe(false);

    await wrapper.get('[data-testid="organization-try-again"]').trigger("click");
    await flushPromises();

    expect(rescanCalls).toHaveLength(1);
    expect(wrapper.find('[data-testid="organization-start-over"]').exists()).toBe(true);
    await wrapper.get('[data-testid="organization-start-over"]').trigger("click");
    expect(wrapper.emitted("startOver")).toEqual([[]]);
  });

  it("says in the button and the sentence that Try again is a full rescan", () => {
    const wrapper = mountNotice({ kind: "recovered", issue: { kind: "unreadable" } });

    expect(wrapper.get('[data-testid="organization-try-again"]').text()).toBe(
      "Rescan and try again",
    );
    expect(wrapper.text()).toContain("close it and rescan");
  });

  it("shows a failed retry next to the button instead of leaving an unhandled rejection", async () => {
    const unhandled = vi.fn();
    process.on("unhandledRejection", unhandled);
    try {
      const wrapper = mountNotice(
        { kind: "recovered", issue: { kind: "unreadable" } },
        { code: "profile_io_failed", message: "disk said no" },
      );

      await wrapper.get('[data-testid="organization-try-again"]').trigger("click");
      await flushPromises();
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(wrapper.get('[data-testid="organization-retry-error"]').text()).not.toBe("");
      // A failed retry still offers Start over: reading the file again did not help.
      expect(wrapper.find('[data-testid="organization-start-over"]').exists()).toBe(true);
      expect(unhandled).not.toHaveBeenCalled();
    } finally {
      process.off("unhandledRejection", unhandled);
    }
  });

  it.each([
    [{ kind: "tooLarge", limitBytes: 4_194_304 }, "too large"],
    [{ kind: "malformed" }, "damaged"],
    [{ kind: "newerVersion", found: 9 }, "newer version"],
    [{ kind: "invalid", rule: "wouldCreateCycle" }, "groups that don't fit together"],
    // The same sentence covers causes that are not a loop or a limit, so it names none alone.
    [{ kind: "invalid", rule: "danglingParent" }, "groups that don't fit together"],
    [{ kind: "invalid", rule: "nameTaken" }, "groups that don't fit together"],
  ] as const)("offers Start over at once, and no retry, for %j", (issue, words) => {
    const wrapper = mountNotice({ kind: "recovered", issue });

    expect(wrapper.text()).toContain(words);
    expect(wrapper.find('[data-testid="organization-try-again"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="organization-start-over"]').exists()).toBe(true);
  });

  it("only says groups are unavailable when the session has no organization store", () => {
    const wrapper = mountNotice({ kind: "notLoaded" });

    expect(wrapper.text()).toContain("not available in this session");
    expect(wrapper.findAll("button")).toHaveLength(0);
  });
});
