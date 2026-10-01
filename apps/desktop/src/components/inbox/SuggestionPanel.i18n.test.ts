// Regression coverage for the "raw wire value leaks into a rendered
// sentence instead of its translated label" bug class (see the fixes to
// `SuggestionPanel.vue`'s `placement`/`relationKind`/`origin`/`strength`/
// `layer` interpolations): renders under zh-CN and asserts the
// translated word appears while the raw English/camelCase wire value
// does not.

import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";
import { buildDetail, mountPanel } from "@/components/inbox/SuggestionPanel.test-support";
import { buildPluralRules } from "@/i18n/format";
import en from "@/locales/en.json";
import zhCN from "@/locales/zh-CN.json";
import { installMockIpc } from "@/services/ipc.mock";
import type { FindingDto } from "@/types/generated/FindingDto";

beforeEach(() => installMockIpc({ list_mod_names: {} }));
afterEach(() => clearMocks());

/**
 * A zh-CN i18n instance — installed last in `mountPanel`'s plugin list, so
 * it wins over the app-wide `en` instance `i18n/test-setup.ts` installs
 * for every mount. `fallbackLocale: "en"` matches `createAppI18n`'s own
 * real production setting (never `false`): a key not yet translated in
 * `zh-CN.json` still renders through English rather than a raw dotted
 * key, exactly as the real app behaves for an in-progress locale.
 */
function zhI18n() {
  return createI18n({
    legacy: false,
    locale: "zh-CN",
    fallbackLocale: "en",
    messages: { en, "zh-CN": zhCN },
    pluralRules: buildPluralRules(),
  });
}

describe("SuggestionPanel renders enum/DTO values through their translated label under zh-CN", () => {
  it("placementOverruled: {placement}/{origin} render as 顶部/用户决定, never the raw wire value", () => {
    const finding: FindingDto = {
      kind: "placementOverruled",
      modId: "a.mod",
      placement: "top",
      origin: "userDecision",
      by: { after: "b.mod", before: "a.mod", layer: "declared", detail: "d" },
      landedAt: 0,
    };

    const wrapper = mountPanel(buildDetail(finding), {}, [zhI18n()]);
    const text = wrapper.find('[data-testid="finding-payload"]').text();

    expect(text).toContain("顶部");
    expect(text).toContain("用户决定");
    expect(text).not.toContain("top");
    expect(text).not.toContain("userDecision");
  });

  it("edgeDropped: {strength} renders as its translated label, never the raw wire value", () => {
    const finding: FindingDto = {
      kind: "edgeDropped",
      after: "b.mod",
      before: "a.mod",
      strength: "hard",
      edgeKind: "loadAfter",
      detail: "d",
      winner: null,
    };

    const wrapper = mountPanel(buildDetail(finding), {}, [zhI18n()]);
    const text = wrapper.find('[data-testid="finding-payload"]').text();

    expect(text).toContain("硬性");
    expect(text).not.toContain("hard");
  });

  it("declarationOverridden: {by.layer} renders as its translated label, never the raw wire value", () => {
    const finding: FindingDto = {
      kind: "declarationOverridden",
      declaredAfter: "a.mod",
      declaredBefore: "b.mod",
      edgeKind: "loadAfter",
      detail: "d",
      by: { after: "b.mod", before: "a.mod", layer: "userDecision", detail: "d" },
    };

    const wrapper = mountPanel(buildDetail(finding), {}, [zhI18n()]);
    const text = wrapper.find('[data-testid="finding-payload"]').text();

    expect(text).toContain("用户决定");
    expect(text).not.toContain("userDecision");
  });
});
