import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import {
  type InboxKeysBackend,
  type InboxKeysStore,
  useInboxKeys,
} from "@/composables/useInboxKeys";
import { installMockIpc } from "@/services/ipc.mock";
import { useInboxStore } from "@/stores/inbox";
import type { FindingKey } from "@/types/brands";
import type { DecideRequestDto } from "@/types/generated/DecideRequestDto";
import type { ResolutionDetailDto } from "@/types/generated/ResolutionDetailDto";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";

function summary(key: string): ResolutionSummaryDto {
  return {
    key,
    finding: { kind: "missingMod", modId: "gone.mod" },
    status: "needsInput",
    confidence: 50,
    effective: { kind: "accept" },
    hasDecision: false,
    mergeState: null,
    structuralGuardField: null,
    scope: null,
    defRef: null,
  };
}

function detailFor(key: string, overrides: Partial<ResolutionDetailDto> = {}): ResolutionDetailDto {
  return {
    key,
    finding: { kind: "missingMod", modId: "gone.mod" },
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
        {
          action: { kind: "accept" },
          rationale: "some other alternative",
          rationaleCode: { kind: "keepCurrentWinner" },
        },
      ],
    },
    status: "needsInput",
    effective: { kind: "removeMod", modId: "gone.mod" },
    note: null,
    hasDecision: false,
    resolvedBySuggested: null,
    mergeState: null,
    structuralGuardField: null,
    scope: null,
    defRef: null,
    ...overrides,
  };
}

const DECIDE_RESULT = {
  stats: { auto: 0, needsInput: 0, overridden: 0, resolvedBySuggested: 0 },
  resorted: false,
  movedMods: 0,
};

function mountHarness(
  overrides: Partial<{
    currentDetail: ResolutionDetailDto | undefined;
    backend: InboxKeysBackend;
    store: InboxKeysStore;
    onEscape: () => void;
  }> = {},
) {
  const decideCalls: DecideRequestDto[] = [];
  const revertCalls: string[] = [];
  installMockIpc({
    decide: (payload: unknown) => {
      const request = (payload as { request: DecideRequestDto }).request;
      decideCalls.push(request);
      return DECIDE_RESULT;
    },
    revert_decision: (payload: unknown) => {
      revertCalls.push((payload as { key: string }).key);
      return DECIDE_RESULT;
    },
  });

  const items = [summary("a"), summary("b"), summary("c")];
  const currentDetail = ref<ResolutionDetailDto | undefined>(
    "currentDetail" in overrides ? overrides.currentDetail : detailFor("b"),
  );
  const onOpenOrder = vi.fn();
  const onOpenModDetail = vi.fn();
  const onFocusSearch = vi.fn();
  const onToggleHelp = vi.fn();
  const onOpenMerge = vi.fn();

  const Harness = defineComponent({
    setup() {
      useInboxKeys({
        items,
        currentDetail: () => currentDetail.value,
        containerRef: () => document.querySelector<HTMLElement>('[data-testid="container"]'),
        onOpenOrder,
        onOpenModDetail,
        onFocusSearch,
        onToggleHelp,
        onOpenMerge,
        ...(overrides.backend ? { backend: overrides.backend } : {}),
        ...(overrides.store ? { store: overrides.store } : {}),
        ...(overrides.onEscape ? { onEscape: overrides.onEscape } : {}),
      });
      return {};
    },
    // `outside-focusable`: a plain, non-interactive-but-focusable
    // element (no button/link/input/select role, no dialog ancestor) —
    // isolates the narrower "focus must be body or inside the finding
    // list" decide-scope check from the general `isInert` check, which
    // every other inert-target test here already exercises.
    template: `
      <div>
        <div tabindex="0" data-testid="container">
          <input data-testid="text-input" />
        </div>
        <button type="button" data-testid="some-button">A button</button>
        <div role="dialog" data-testid="a-dialog">
          <button type="button" data-testid="dialog-button">Dialog button</button>
        </div>
        <div role="radiogroup" data-testid="a-radiogroup">
          <button type="button" role="radio" data-testid="radio-button">An option</button>
        </div>
        <div tabindex="0" data-testid="outside-focusable">Some other focusable region</div>
      </div>
    `,
  });

  // The same pinia instance is both installed into the harness and made
  // "active" here, so `useInboxStore()` called from a test body (outside
  // the component tree) resolves to the identical store instance the
  // mounted composable reads and writes.
  const pinia = createPinia();
  setActivePinia(pinia);

  const wrapper = mount(Harness, {
    attachTo: document.body,
    global: { plugins: [pinia, PiniaColada] },
  });

  return {
    wrapper,
    currentDetail,
    onOpenOrder,
    onOpenModDetail,
    onFocusSearch,
    onToggleHelp,
    onOpenMerge,
    decideCalls,
    revertCalls,
  };
}

function press(target: Element, key: string): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, cancelable: true, bubbles: true }));
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("useInboxKeys", () => {
  afterEach(() => {
    clearMocks();
  });

  it("moves the cursor with j/k and clamps at the bounds", () => {
    const { wrapper } = mountHarness();
    const inbox = useInboxStore();
    expect(inbox.cursor).toBe(0);

    press(document.body, "j");
    expect(inbox.cursor).toBe(1);
    press(document.body, "j");
    press(document.body, "j");
    expect(inbox.cursor).toBe(2); // clamped to the last of 3 items

    press(document.body, "k");
    expect(inbox.cursor).toBe(1);
    wrapper.unmount();
  });

  it("Enter accepts the suggestion and advances the cursor", async () => {
    const { wrapper, decideCalls } = mountHarness();
    const inbox = useInboxStore();
    inbox.setCursor(1); // "b" — see `mountHarness`'s fixed `currentDetail`

    press(document.body, "Enter");
    await flush();

    expect(decideCalls).toEqual([
      { key: "b", action: { kind: "removeMod", modId: "gone.mod" }, note: null },
    ]);
    expect(inbox.cursor).toBe(2);
    wrapper.unmount();
  });

  it("digit keys pick the matching 1-based alternative", async () => {
    const { wrapper, decideCalls } = mountHarness();

    press(document.body, "2");
    await flush();

    expect(decideCalls).toEqual([{ key: "b", action: { kind: "accept" }, note: null }]);
    wrapper.unmount();
  });

  it("i ignores the current finding", async () => {
    const { wrapper, decideCalls } = mountHarness();

    press(document.body, "i");
    await flush();

    expect(decideCalls).toEqual([{ key: "b", action: { kind: "ignore" }, note: null }]);
    wrapper.unmount();
  });

  it("u reverts only when a decision exists", async () => {
    const { wrapper, revertCalls } = mountHarness({
      currentDetail: detailFor("b", { hasDecision: false }),
    });

    press(document.body, "u");
    await flush();
    expect(revertCalls).toEqual([]);
    wrapper.unmount();
  });

  it("u reverts when a decision is present", async () => {
    const { wrapper, revertCalls } = mountHarness({
      currentDetail: detailFor("b", { hasDecision: true }),
    });

    press(document.body, "u");
    await flush();
    expect(revertCalls).toEqual(["b"]);
    wrapper.unmount();
  });

  it("o and g navigate to the finding's primary mod", () => {
    const { wrapper, onOpenOrder, onOpenModDetail } = mountHarness();

    press(document.body, "o");
    expect(onOpenOrder).toHaveBeenCalledWith("gone.mod");

    press(document.body, "g");
    expect(onOpenModDetail).toHaveBeenCalledWith("gone.mod");
    wrapper.unmount();
  });

  it("does not navigate when the finding names no mod", () => {
    const { wrapper, onOpenOrder } = mountHarness({
      currentDetail: {
        ...detailFor("b"),
        // `patchCollision` falls back to `mods[0] ?? ""` — an empty
        // `mods` array is the one case `primaryModIdOf` can return an
        // empty string for.
        finding: {
          kind: "patchCollision",
          key: { defType: "ThingDef", defName: "Wall" },
          selector: "defName",
          subPath: null,
          mods: [],
        },
      },
    });

    press(document.body, "o");
    expect(onOpenOrder).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("/ focuses search and ? toggles help", () => {
    const { wrapper, onFocusSearch, onToggleHelp } = mountHarness();

    press(document.body, "/");
    expect(onFocusSearch).toHaveBeenCalledTimes(1);

    press(document.body, "?");
    expect(onToggleHelp).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("f cycles the status filter", () => {
    const { wrapper } = mountHarness();
    const inbox = useInboxStore();
    expect(inbox.filter.status).toBe("needsInput");

    press(document.body, "f");
    expect(inbox.filter.status).toBe("auto");
    wrapper.unmount();
  });

  it("is inert while focus is inside a text input", async () => {
    const { wrapper, decideCalls } = mountHarness();
    const input = wrapper.get('[data-testid="text-input"]').element;

    press(input, "Enter");
    await flush();

    expect(decideCalls).toEqual([]);
    wrapper.unmount();
  });

  it("j/k still move the cursor from a clicked radio, while the arrows stay with the radio group", () => {
    const { wrapper } = mountHarness();
    const inbox = useInboxStore();
    const radio = wrapper.get('[data-testid="radio-button"]').element;

    press(radio, "j");
    expect(inbox.cursor).toBe(1);
    press(radio, "ArrowDown");
    press(radio, "ArrowRight");
    expect(inbox.cursor).toBe(1);
    press(radio, "k");
    expect(inbox.cursor).toBe(0);
    wrapper.unmount();
  });

  it("Enter on a focused button does not decide (native click semantics own it)", async () => {
    const { wrapper, decideCalls } = mountHarness();
    const button = wrapper.get('[data-testid="some-button"]').element;

    press(button, "Enter");
    await flush();

    expect(decideCalls).toEqual([]);
    wrapper.unmount();
  });

  it("Enter inside a dialog does not decide, even on a plain element there", async () => {
    const { wrapper, decideCalls } = mountHarness();
    const dialog = wrapper.get('[data-testid="a-dialog"]').element;

    press(dialog, "Enter");
    await flush();

    expect(decideCalls).toEqual([]);
    wrapper.unmount();
  });

  it("Enter does not decide when focus sits on some other focusable element outside the finding list", async () => {
    // `event.target` alone (what `isInert` checks) can't tell "a link/
    // button/input" from "some other tabindex'd, non-form element" —
    // this is what the narrower, `document.activeElement`-based
    // decide-scope check exists for, so it needs a *real* `.focus()`
    // (not just an event dispatched at the element) to be meaningful.
    const { wrapper, decideCalls } = mountHarness();
    const outside = wrapper.get('[data-testid="outside-focusable"]').element as HTMLElement;
    outside.focus();
    expect(document.activeElement).toBe(outside);

    press(outside, "Enter");
    await flush();

    expect(decideCalls).toEqual([]);
    wrapper.unmount();
  });

  it("m decides an empty merge on a def-override finding and navigates to its editor", async () => {
    const { wrapper, decideCalls, onOpenMerge } = mountHarness({
      currentDetail: detailFor("b", {
        finding: {
          kind: "defOverride",
          key: { defType: "ThingDef", defName: "Wall" },
          owners: ["a.mod", "b.mod"],
          winner: "b.mod",
        },
      }),
    });

    press(document.body, "m");
    await flush();

    expect(decideCalls).toEqual([
      {
        key: "b",
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Wall" }, choices: {} },
        note: null,
      },
    ]);
    expect(onOpenMerge).toHaveBeenCalledWith("b");
    wrapper.unmount();
  });

  it("m does nothing on a finding that can't be merged", async () => {
    const { wrapper, decideCalls, onOpenMerge } = mountHarness();

    press(document.body, "m");
    await flush();

    expect(decideCalls).toEqual([]);
    expect(onOpenMerge).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("picking a merge alternative decides it and navigates to its editor", async () => {
    const { wrapper, decideCalls, onOpenMerge } = mountHarness({
      currentDetail: detailFor("b", {
        suggestion: {
          action: { kind: "accept" },
          confidence: 60,
          rationale: "because",
          rationaleCode: { kind: "defOverrideUnexplained" },
          alternatives: [
            {
              action: { kind: "merge", key: { defType: "ThingDef", defName: "Wall" }, choices: {} },
              rationale: "merge field by field",
              rationaleCode: { kind: "mergeUnexplainedDefOverride" },
            },
          ],
        },
      }),
    });

    press(document.body, "1");
    await flush();

    expect(decideCalls).toEqual([
      {
        key: "b",
        action: { kind: "merge", key: { defType: "ThingDef", defName: "Wall" }, choices: {} },
        note: null,
      },
    ]);
    expect(onOpenMerge).toHaveBeenCalledWith("b");
    wrapper.unmount();
  });

  it("Enter decides when focus is inside the finding list container itself", async () => {
    const { wrapper, decideCalls } = mountHarness();
    const container = wrapper.get('[data-testid="container"]').element as HTMLElement;
    container.focus();
    expect(document.activeElement).toBe(container);

    press(container, "Enter");
    await flush();

    expect(decideCalls).toEqual([
      { key: "b", action: { kind: "removeMod", modId: "gone.mod" }, note: null },
    ]);
    wrapper.unmount();
  });

  it("a custom backend receives decide/revert instead of the profile mutations", async () => {
    const backendDecideCalls: DecideRequestDto[] = [];
    const backendRevertCalls: FindingKey[] = [];
    const backend: InboxKeysBackend = {
      decide: (request) => {
        backendDecideCalls.push(request);
        return Promise.resolve(DECIDE_RESULT);
      },
      revert: (key) => {
        backendRevertCalls.push(key);
        return Promise.resolve(DECIDE_RESULT);
      },
    };
    const { wrapper, decideCalls, revertCalls } = mountHarness({
      backend,
      currentDetail: detailFor("b", { hasDecision: true }),
    });

    press(document.body, "Enter");
    await flush();
    press(document.body, "u");
    await flush();

    expect(backendDecideCalls).toEqual([
      { key: "b", action: { kind: "removeMod", modId: "gone.mod" }, note: null },
    ]);
    expect(backendRevertCalls).toEqual(["b"]);
    // The profile's own `decide`/`revert_decision` commands are never hit.
    expect(decideCalls).toEqual([]);
    expect(revertCalls).toEqual([]);
    wrapper.unmount();
  });

  it("a custom store is read and written instead of the profile inbox store", () => {
    const customStore: InboxKeysStore = {
      cursor: 0,
      expandedKey: null,
      setCursor(index) {
        this.cursor = index;
      },
      setExpandedKey(key) {
        this.expandedKey = key;
      },
      cycleStatusFilter: vi.fn(),
    };
    const { wrapper } = mountHarness({ store: customStore });
    const profileInbox = useInboxStore();

    press(document.body, "j");
    expect(customStore.cursor).toBe(1);
    expect(profileInbox.cursor).toBe(0);

    press(document.body, "f");
    expect(customStore.cycleStatusFilter).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("Escape calls onEscape when provided, and does nothing when omitted", () => {
    const onEscape = vi.fn();
    const { wrapper } = mountHarness({ onEscape });

    press(document.body, "Escape");
    expect(onEscape).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("Escape is a no-op when onEscape is not provided", () => {
    // No assertion beyond "this doesn't throw" — the profile inbox never
    // passes `onEscape`, and Escape must stay inert there.
    const { wrapper } = mountHarness();
    expect(() => press(document.body, "Escape")).not.toThrow();
    wrapper.unmount();
  });
});
