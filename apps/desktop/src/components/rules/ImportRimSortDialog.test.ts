import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ImportRimSortDialog from "@/components/rules/ImportRimSortDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

async function mountDialog(defaults: RimSortPathsDto | null) {
  installMockIpc({ get_default_rimsort_paths: defaults });
  const wrapper = mount(ImportRimSortDialog, {
    props: { visible: true },
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  await flush();
  await wrapper.vm.$nextTick();
  return wrapper;
}

async function submit(wrapper: Awaited<ReturnType<typeof mountDialog>>): Promise<void> {
  await wrapper.get('[data-testid="import-rimsort-form"]').trigger("submit");
}

function emittedPaths(wrapper: Awaited<ReturnType<typeof mountDialog>>): RimSortPathsDto {
  const events = wrapper.emitted("import");
  expect(events).toBeTruthy();
  return (events as RimSortPathsDto[][])[0]?.[0] as RimSortPathsDto;
}

describe("ImportRimSortDialog", () => {
  afterEach(() => {
    clearMocks();
  });

  // On a machine with no
  // RimSort install, `get_default_rimsort_paths` returns per-field
  // `null`s (`commands/project.rs`'s own `.is_file()` check) — this
  // dialog is the only way into that all-absent import path. A blank
  // field must submit `null`, never an empty string
  // (a `RimSortPaths` field being `Some("")` is not a state the backend
  // treats as "not part of this import").
  it("submits null for every field left blank, matching an all-absent default", async () => {
    const wrapper = await mountDialog({
      userRules: null,
      communityRules: null,
      steamDb: null,
    });

    await submit(wrapper);

    expect(emittedPaths(wrapper)).toEqual({
      userRules: null,
      communityRules: null,
      steamDb: null,
    });
  });

  it("prefills from a default that exists and still submits null for one that doesn't", async () => {
    const wrapper = await mountDialog({
      userRules: "C:/RimSort/dbs/userRules.json",
      communityRules: null,
      steamDb: null,
    });

    expect(
      (wrapper.get('[data-testid="import-user-rules-path"]').element as HTMLInputElement).value,
    ).toBe("C:/RimSort/dbs/userRules.json");

    await submit(wrapper);

    expect(emittedPaths(wrapper)).toEqual({
      userRules: "C:/RimSort/dbs/userRules.json",
      communityRules: null,
      steamDb: null,
    });
  });

  it("treats a whitespace-only field as blank, submitting null rather than the literal text", async () => {
    const wrapper = await mountDialog({
      userRules: null,
      communityRules: null,
      steamDb: null,
    });

    await wrapper.get('[data-testid="import-community-rules-path"]').setValue("   ");
    await submit(wrapper);

    expect(emittedPaths(wrapper).communityRules).toBeNull();
  });

  it("submits a hand-typed path unmodified when one is actually entered", async () => {
    const wrapper = await mountDialog({
      userRules: null,
      communityRules: null,
      steamDb: null,
    });

    await wrapper
      .get('[data-testid="import-steam-db-path"]')
      .setValue("D:/RimSort/dbs/Steam-Workshop-Database/steamDB.json");
    await submit(wrapper);

    expect(emittedPaths(wrapper).steamDb).toBe(
      "D:/RimSort/dbs/Steam-Workshop-Database/steamDB.json",
    );
  });
});
