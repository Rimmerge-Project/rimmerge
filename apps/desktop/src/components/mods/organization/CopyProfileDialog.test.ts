import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, type Pinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import CopyProfileDialog from "@/components/mods/organization/CopyProfileDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { CopyAnswerDto } from "@/types/generated/CopyAnswerDto";
import type { CopyReportDto } from "@/types/generated/CopyReportDto";

/**
 * The copy dialog sequences four steps over the two copy commands and shows only what the backend
 * answered: the picked folder, the preview of the chosen mode, a confirm for Replace (Cancel
 * focused), and the result.
 */

type Call = { command: string; payload: unknown };
type Handler = (payload: unknown) => unknown;

const calls: Call[] = [];
const pickerOptions: unknown[] = [];
let wrappers: ReturnType<typeof mount>[] = [];

function report(overrides: Partial<CopyReportDto> = {}): CopyReportDto {
  return {
    mode: "merge",
    groupsAdded: 2,
    groupsMerged: 1,
    membershipsAdded: 5,
    membershipsAlready: 1,
    modColorsAdded: 0,
    modColorsKept: 0,
    groupColorsAdopted: 0,
    clashes: [],
    discardedGroups: 0,
    discardedMemberships: 0,
    discardedModColors: 0,
    notInstalledMods: 0,
    ...overrides,
  };
}

function answer(overrides: Partial<CopyReportDto> = {}): CopyAnswerDto {
  return { kind: "report", report: report(overrides) };
}

const REPLACE_REPORT: Partial<CopyReportDto> = {
  mode: "replace",
  groupsMerged: 0,
  discardedGroups: 3,
  discardedMemberships: 1,
  discardedModColors: 4,
};

/** A promise the test settles itself, for a call that must stay in flight. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function mountDialog(
  handlers: Record<string, Handler>,
  pickedFolder: string | null = "C:/other",
  profileDir: string | null = null,
) {
  calls.length = 0;
  pickerOptions.length = 0;
  const pinia: Pinia = createPinia();
  if (profileDir !== null) {
    useSessionStore(pinia).setLoaded(
      { gameDir: "G", workshopDir: "W", modsConfig: "M", profileDir },
      "current",
    );
  }
  installMockIpc({
    get_organization: { kind: "loaded", groups: [], coloredModCount: 0 },
    list_working_mods: { items: [], total: 0, reorderable: true },
    list_inactive_mods: { items: [], total: 0, reorderable: false },
    "plugin:dialog|open": (payload: unknown) => {
      pickerOptions.push((payload as { options: unknown }).options);
      return pickedFolder;
    },
    ...Object.fromEntries(
      Object.entries(handlers).map(([command, handler]) => [
        command,
        (payload: unknown) => {
          calls.push({ command, payload });
          return handler(payload);
        },
      ]),
    ),
  });
  const wrapper = mount(CopyProfileDialog, {
    props: {
      visible: true,
      "onUpdate:visible": (value: boolean) => {
        void wrapper.setProps({ visible: value });
      },
    },
    attachTo: document.body,
    global: {
      plugins: [pinia, PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true, transition: false },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

async function choose(wrapper: ReturnType<typeof mount>): Promise<void> {
  await flushPromises();
  await wrapper.get('[data-testid="copy-choose"]').trigger("click");
  await flushPromises();
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

describe("CopyProfileDialog", () => {
  it("previews the picked folder in Merge and copies it with the same request", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () => answer(),
      copy_organization: () => answer(),
    });

    await choose(wrapper);

    expect(calls).toEqual([
      {
        command: "preview_copy_organization",
        payload: { request: { sourceDir: "C:/other", mode: "merge" } },
      },
    ]);
    expect(wrapper.get('[data-testid="copy-folder"]').text()).toBe("Copying from C:/other");
    expect(wrapper.get('[data-testid="copy-line-groupsAdded"]').text()).toBe("2 groups added");
    expect(wrapper.get('[data-testid="copy-line-groupsMerged"]').text()).toContain(
      "1 group merged",
    );

    await wrapper.get('[data-testid="copy-submit"]').trigger("click");
    await flushPromises();

    expect(calls.at(-1)).toEqual({
      command: "copy_organization",
      payload: { request: { sourceDir: "C:/other", mode: "merge" } },
    });
    expect(wrapper.get('[data-testid="copy-result"]').text()).toContain(
      "Merged the other profile's groups and colours into yours.",
    );
  });

  it("says a single group in the singular and lists a colour clash with its path", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        answer({
          groupsAdded: 1,
          groupsMerged: 0,
          membershipsAdded: 1,
          membershipsAlready: 0,
          clashes: [{ path: ["Gear", "Swords"], kept: "blue", offered: "green" }],
        }),
    });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-line-groupsAdded"]').text()).toBe("1 group added");
    expect(wrapper.get('[data-testid="copy-clashes-heading"]').text()).toContain(
      "1 group has a different colour here",
    );
    expect(wrapper.get('[data-testid="copy-clash"]').text()).toBe(
      "Gear / Swords: yours is Blue, theirs is Green",
    );
  });

  it("asks again in the other mode and shows what a Replace discards", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: (payload) =>
        (payload as { request: { mode: string } }).request.mode === "replace"
          ? answer(REPLACE_REPORT)
          : answer(),
    });
    await choose(wrapper);

    await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
    await flushPromises();

    expect(calls.map((call) => call.payload)).toEqual([
      { request: { sourceDir: "C:/other", mode: "merge" } },
      { request: { sourceDir: "C:/other", mode: "replace" } },
    ]);
    expect(wrapper.get('[data-testid="copy-preview"]').text()).toContain("3 groups");
    expect(wrapper.get('[data-testid="copy-submit"]').text()).toBe("Replace…");
  });

  it("keeps the modes selectable while a preview loads, but cannot change the folder or copy", async () => {
    const held = deferred<CopyAnswerDto>();
    const wrapper = mountDialog({ preview_copy_organization: () => held.promise });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-loading"]').text()).toBe("Loading…");
    expect(wrapper.get('[data-testid="copy-mode-replace"]').attributes("disabled")).toBeUndefined();
    expect(wrapper.get('[data-testid="copy-choose"]').attributes("disabled")).toBeDefined();
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeDefined();

    held.resolve(answer());
    await flushPromises();

    expect(wrapper.find('[data-testid="copy-loading"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeUndefined();
  });

  it("shows the newest choice when an older preview resolves last", async () => {
    const merge = deferred<CopyAnswerDto>();
    const replace = deferred<CopyAnswerDto>();
    const wrapper = mountDialog({
      preview_copy_organization: (payload) =>
        (payload as { request: { mode: string } }).request.mode === "replace"
          ? replace.promise
          : merge.promise,
    });
    await choose(wrapper);

    await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
    replace.resolve(answer(REPLACE_REPORT));
    await flushPromises();
    merge.resolve(answer());
    await flushPromises();

    expect(wrapper.get('[data-testid="copy-preview"]').text()).toContain("3 groups");
    expect(wrapper.get('[data-testid="copy-submit"]').text()).toBe("Replace…");
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeUndefined();
    expect(wrapper.find('[data-testid="copy-loading"]').exists()).toBe(false);
  });

  it("starts the folder picker beside this profile's own folder", async () => {
    const wrapper = mountDialog(
      { preview_copy_organization: () => answer() },
      "C:/other",
      "C:/Users/me/AppData/Local/rimmerge/profiles/abc123",
    );

    await choose(wrapper);

    expect(pickerOptions).toEqual([
      expect.objectContaining({
        directory: true,
        defaultPath: "C:/Users/me/AppData/Local/rimmerge/profiles",
      }),
    ]);
  });

  it("asks to confirm a Replace with Cancel focused, and Cancel copies nothing", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () => answer(REPLACE_REPORT),
      copy_organization: () => answer(REPLACE_REPORT),
    });
    await choose(wrapper);
    await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
    await flushPromises();

    await wrapper.get('[data-testid="copy-submit"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="copy-confirm"]').text()).toContain("This cannot be undone.");
    expect(document.activeElement).toBe(wrapper.get('[data-testid="copy-confirm-cancel"]').element);
    expect(calls.some((call) => call.command === "copy_organization")).toBe(false);

    await wrapper.get('[data-testid="copy-confirm-cancel"]').trigger("click");
    await flushPromises();

    expect(wrapper.props("visible")).toBe(false);
    expect(calls.some((call) => call.command === "copy_organization")).toBe(false);
  });

  it("copies in Replace only after the confirm", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () => answer(REPLACE_REPORT),
      copy_organization: () => answer(REPLACE_REPORT),
    });
    await choose(wrapper);
    await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
    await flushPromises();
    await wrapper.get('[data-testid="copy-submit"]').trigger("click");
    await flushPromises();

    await wrapper.get('[data-testid="copy-confirm-replace"]').trigger("click");
    await flushPromises();

    expect(calls.at(-1)).toEqual({
      command: "copy_organization",
      payload: { request: { sourceDir: "C:/other", mode: "replace" } },
    });
    expect(wrapper.get('[data-testid="copy-result"]').text()).toContain(
      "Replaced your groups and colours with the other profile's.",
    );
  });

  it("shows a refusal as a sentence, keeps Merge disabled and copies nothing", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        ({ kind: "refused", reason: { kind: "sourceMissing" } }) satisfies CopyAnswerDto,
    });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-refusal"]').text()).toContain(
      "That folder has no groups and colours file.",
    );
    expect(wrapper.find('[data-testid="copy-preview"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeDefined();
  });

  it("words the refusal that names this profile's own folder", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        ({ kind: "refused", reason: { kind: "sameProfile" } }) satisfies CopyAnswerDto,
    });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-refusal"]').text()).toContain(
      "That is this profile's own folder.",
    );
  });

  it("stays on the first step and asks nothing when the picker is cancelled", async () => {
    const wrapper = mountDialog({ preview_copy_organization: () => answer() }, null);

    await choose(wrapper);

    expect(calls).toEqual([]);
    expect(wrapper.find('[data-testid="copy-folder"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeDefined();
  });

  it("shows a failed command inside the dialog", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () => {
        throw { code: "organization_unavailable", message: "not available" };
      },
    });

    await choose(wrapper);

    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeDefined();
  });

  describe("while a copy is running", () => {
    async function startHeldCopy() {
      const held = deferred<CopyAnswerDto>();
      const wrapper = mountDialog({
        preview_copy_organization: () => answer(),
        copy_organization: () => held.promise,
      });
      await choose(wrapper);
      await wrapper.get('[data-testid="copy-submit"]').trigger("click");
      await flushPromises();
      return { wrapper, held };
    }

    function pressEscape(): void {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape" }));
    }

    it("cannot be closed with Escape, the close button or Cancel until it finishes", async () => {
      const { wrapper, held } = await startHeldCopy();

      pressEscape();
      await flushPromises();

      expect(wrapper.props("visible")).toBe(true);
      expect(wrapper.get('[data-testid="copy-cancel"]').attributes("disabled")).toBeDefined();
      expect(document.querySelector(".p-dialog-close-button")).toBeNull();

      held.resolve(answer());
      await flushPromises();
      pressEscape();
      await flushPromises();

      expect(wrapper.props("visible")).toBe(false);
    });

    it("cannot be closed from the Replace confirm either", async () => {
      const held = deferred<CopyAnswerDto>();
      const wrapper = mountDialog({
        preview_copy_organization: () => answer(REPLACE_REPORT),
        copy_organization: () => held.promise,
      });
      await choose(wrapper);
      await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
      await flushPromises();
      await wrapper.get('[data-testid="copy-submit"]').trigger("click");
      await flushPromises();
      await wrapper.get('[data-testid="copy-confirm-replace"]').trigger("click");
      await flushPromises();

      pressEscape();
      await flushPromises();

      expect(wrapper.props("visible")).toBe(true);
      expect(
        wrapper.get('[data-testid="copy-confirm-cancel"]').attributes("disabled"),
      ).toBeDefined();
      held.resolve(answer(REPLACE_REPORT));
      await flushPromises();
    });

    it("writes nothing into a dialog that was reopened meanwhile", async () => {
      const { wrapper, held } = await startHeldCopy();

      await wrapper.setProps({ visible: false });
      await wrapper.setProps({ visible: true });
      held.resolve(answer());
      await flushPromises();

      expect(wrapper.find('[data-testid="copy-result"]').exists()).toBe(false);
      expect(wrapper.find('[data-testid="copy-folder"]').exists()).toBe(false);
      expect(wrapper.get('[data-testid="copy-choose"]').attributes("disabled")).toBeUndefined();
    });
  });

  describe("a copy that comes back refused or failed", () => {
    const REFUSED = { kind: "refused", reason: { kind: "sourceMissing" } } as const;

    it("from a Replace confirm returns to the preview with focus on Choose", async () => {
      const wrapper = mountDialog({
        preview_copy_organization: () => answer(REPLACE_REPORT),
        copy_organization: () => REFUSED satisfies CopyAnswerDto,
      });
      await choose(wrapper);
      await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
      await flushPromises();
      await wrapper.get('[data-testid="copy-submit"]').trigger("click");
      await flushPromises();

      await wrapper.get('[data-testid="copy-confirm-replace"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="copy-confirm"]').exists()).toBe(false);
      expect(wrapper.get('[data-testid="copy-refusal"]').text()).toContain(
        "That folder has no groups and colours file.",
      );
      expect(document.activeElement).toBe(wrapper.get('[data-testid="copy-choose"]').element);
    });

    it("from a Merge stays on the preview and disables Merge", async () => {
      const wrapper = mountDialog({
        preview_copy_organization: () => answer(),
        copy_organization: () => REFUSED satisfies CopyAnswerDto,
      });
      await choose(wrapper);
      // The keyboard user is on Merge when it is clicked; the button then turns disabled.
      (wrapper.get('[data-testid="copy-submit"]').element as HTMLElement).focus();
      expect(document.activeElement).toBe(wrapper.get('[data-testid="copy-submit"]').element);

      await wrapper.get('[data-testid="copy-submit"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="copy-refusal"]').exists()).toBe(true);
      expect(wrapper.get('[data-testid="copy-submit"]').attributes("disabled")).toBeDefined();
      expect(document.activeElement).toBe(wrapper.get('[data-testid="copy-choose"]').element);
    });

    it("from a failed command shows the error on the preview with focus on Choose", async () => {
      const wrapper = mountDialog({
        preview_copy_organization: () => answer(REPLACE_REPORT),
        copy_organization: () => {
          throw { code: "profile_io_failed", message: "disk full" };
        },
      });
      await choose(wrapper);
      await wrapper.get('[data-testid="copy-mode-replace"]').trigger("change");
      await flushPromises();
      await wrapper.get('[data-testid="copy-submit"]').trigger("click");
      await flushPromises();

      await wrapper.get('[data-testid="copy-confirm-replace"]').trigger("click");
      await flushPromises();

      expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
      expect(document.activeElement).toBe(wrapper.get('[data-testid="copy-choose"]').element);
    });
  });

  it("lists two clashes whose paths read the same when joined, each once", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        answer({
          clashes: [
            { path: ["a / b"], kept: "blue", offered: "green" },
            { path: ["a", "b"], kept: "red", offered: "pink" },
          ],
        }),
    });

    await choose(wrapper);

    expect(wrapper.findAll('[data-testid="copy-clash"]')).toHaveLength(2);
  });

  it("words a source identical to this profile", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        ({ kind: "refused", reason: { kind: "nothingToCopy" } }) satisfies CopyAnswerDto,
    });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-refusal"]').text()).toContain(
      "there is nothing to copy",
    );
  });

  it("words a result too large to save with formatted sizes", async () => {
    const wrapper = mountDialog({
      preview_copy_organization: () =>
        ({
          kind: "refused",
          reason: { kind: "tooLargeToSave", bytes: 3_145_728, limit: 2_097_152 },
        }) satisfies CopyAnswerDto,
    });

    await choose(wrapper);

    expect(wrapper.get('[data-testid="copy-refusal"]').text()).toContain(
      "The result would be 3.00 MiB, more than the 2.00 MiB",
    );
  });
});
