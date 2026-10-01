import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";

import MergeFieldRow from "@/components/merge/MergeFieldRow.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeOwnerDto } from "@/types/generated/MergeOwnerDto";

const owners: MergeOwnerDto[] = [
  { modId: "a.mod", name: "Mod A", position: 0 },
  { modId: "b.mod", name: "Mod B", position: 1 },
];

const field: MergeFieldDto = {
  path: "label",
  depth: 0,
  isListItem: false,
  entry: "leaf",
  container: null,
  class: "conflict",
  changedBy: ["a.mod", "b.mod"],
  confidence: 0,
  base: "base label",
  candidates: { "a.mod": "label a", "b.mod": "label b" },
  result: null,
  choice: null,
  preselected: "b.mod",
};

/**
 * `useModLabel` (via `MergeFieldRow`'s owner aria-labels) needs Pinia +
 * Pinia Colada, and `list_mod_names` needs *some* registered fixture —
 * an unregistered command's rejection reaches Vitest as an unhandled
 * rejection that fails the whole run, not just this file (see the
 * identical note in `InboxPage.test.ts`).
 */
function mountRow(props: InstanceType<typeof MergeFieldRow>["$props"]) {
  installMockIpc({ list_mod_names: {} });
  return mount(MergeFieldRow, {
    props,
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

describe("MergeFieldRow", () => {
  afterEach(() => {
    clearMocks();
  });

  it("renders exactly one radio per owner column", () => {
    const wrapper = mountRow({ field, owners });
    const radios = wrapper.findAll('input[type="radio"]');
    expect(radios).toHaveLength(owners.length);
  });

  it("emits choose with { choice: 'from', modId } when a radio is picked", async () => {
    const wrapper = mountRow({ field, owners });

    await wrapper.get('[data-testid="merge-radio-label-b.mod"]').setValue(true);

    expect(wrapper.emitted("choose")).toEqual([[{ choice: "from", modId: "b.mod" }]]);
  });

  it("checks the radio matching the stored choice", () => {
    const chosen: MergeFieldDto = { ...field, choice: { choice: "from", modId: "a.mod" } };
    const wrapper = mountRow({ field: chosen, owners });

    expect(
      (wrapper.get('[data-testid="merge-radio-label-a.mod"]').element as HTMLInputElement).checked,
    ).toBe(true);
    expect(
      (wrapper.get('[data-testid="merge-radio-label-b.mod"]').element as HTMLInputElement).checked,
    ).toBe(false);
  });

  it("emits drop when the drop button is clicked", async () => {
    const wrapper = mountRow({ field, owners });
    await wrapper.get('[data-testid="merge-drop-button"]').trigger("click");
    expect(wrapper.emitted("drop")).toHaveLength(1);
  });

  it("only shows the revert-to-auto button once a choice is stored", () => {
    const undecided = mountRow({ field, owners });
    expect(undecided.find('[data-testid="merge-revert-button"]').exists()).toBe(false);

    const decided = mountRow({
      field: { ...field, choice: { choice: "drop" } },
      owners,
    });
    expect(decided.find('[data-testid="merge-revert-button"]').exists()).toBe(true);
  });

  it('labels the revert button "Clear choice" on a conflict row, "Auto" otherwise', () => {
    const conflict = mountRow({
      field: { ...field, class: "conflict", choice: { choice: "drop" } },
      owners,
    });
    expect(conflict.get('[data-testid="merge-revert-button"]').text()).toBe("Clear choice");

    const oneSided = mountRow({
      field: { ...field, class: "oneSided", choice: { choice: "drop" } },
      owners,
    });
    expect(oneSided.get('[data-testid="merge-revert-button"]').text()).toBe("Auto");
  });

  // -- Drop hidden on collisions --

  it("hides the Drop button on a patch-collision row, showing a disabled, tooltipped placeholder instead", () => {
    const wrapper = mountRow({ field, owners, isPatchCollision: true });

    expect(wrapper.find('[data-testid="merge-drop-button"]').exists()).toBe(false);
    const placeholder = wrapper.get('[data-testid="merge-drop-unavailable"]');
    expect((placeholder.element as HTMLButtonElement).disabled).toBe(true);
    expect(placeholder.attributes("title")).toContain("patch collision");
  });

  it("still shows a real, clickable Drop button on a def-override row", () => {
    const wrapper = mountRow({ field, owners, isPatchCollision: false });

    expect(wrapper.find('[data-testid="merge-drop-button"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="merge-drop-unavailable"]').exists()).toBe(false);
  });

  // -- distinct dropped/unresolved/value rendering --

  it('renders "needs input" — not "absent" — for an unresolved conflict with no choice', () => {
    const wrapper = mountRow({ field: { ...field, choice: null, result: null }, owners });

    expect(wrapper.get('[data-testid="merge-result-needs-input"]').text()).toBe("needs input");
    expect(wrapper.text()).not.toContain("absent");
  });

  it('renders "dropped" — not "absent" — for a stored Drop choice', () => {
    const wrapper = mountRow({
      field: { ...field, choice: { choice: "drop" }, result: null },
      owners,
    });

    expect(wrapper.get('[data-testid="merge-result-dropped"]').text()).toBe("dropped");
  });

  it("renders the resolved value for a stored From choice", () => {
    const wrapper = mountRow({
      field: { ...field, choice: { choice: "from", modId: "b.mod" }, result: "label b" },
      owners,
    });

    expect(wrapper.find('[data-testid="merge-result-needs-input"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="merge-result-dropped"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("label b");
  });

  // -- optimistic UI from the client's own pending choice --

  it("renders a pending local choice immediately, ahead of the server's own field.choice, with a pending cue", () => {
    const wrapper = mountRow({
      field: { ...field, choice: null, result: null },
      owners,
      pendingChoice: { choice: "from", modId: "b.mod" },
    });

    expect(
      (wrapper.get('[data-testid="merge-radio-label-b.mod"]').element as HTMLInputElement).checked,
    ).toBe(true);
    expect(wrapper.text()).toContain("label b");
    expect(wrapper.find('[data-testid="merge-result-pending"]').exists()).toBe(true);
  });

  it("shows no pending cue once the server's own choice matches the pending one", () => {
    const wrapper = mountRow({
      field: { ...field, choice: { choice: "from", modId: "b.mod" }, result: "label b" },
      owners,
      pendingChoice: { choice: "from", modId: "b.mod" },
    });

    expect(wrapper.find('[data-testid="merge-result-pending"]').exists()).toBe(false);
  });

  it("shows an editable input with the seeded draft while editing, and commits on Enter", async () => {
    const wrapper = mountRow({ field, owners, editing: true, editingDraft: "seeded text" });

    const input = wrapper.get('[data-testid="merge-edit-input"]');
    expect((input.element as HTMLInputElement).value).toBe("seeded text");

    await input.setValue("typed text");
    await input.trigger("keydown", { key: "Enter" });

    expect(wrapper.emitted("editCommit")).toEqual([["typed text"]]);
  });

  it("emits editCancel on Escape while editing", async () => {
    const wrapper = mountRow({ field, owners, editing: true, editingDraft: "" });

    await wrapper.get('[data-testid="merge-edit-input"]').trigger("keydown", { key: "Escape" });

    expect(wrapper.emitted("editCancel")).toHaveLength(1);
  });
});
