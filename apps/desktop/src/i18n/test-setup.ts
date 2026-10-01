import { config } from "@vue/test-utils";

import { createAppI18n } from "@/i18n/i18n";

// Wired into `vitest.config.ts`'s `test.setupFiles`. `@vue/test-utils`
// merges (concatenates), rather than replaces, `config.global.plugins`
// with whatever plugin array an individual `mount()` call passes — see
// `node_modules/@vue/test-utils/dist/vue-test-utils.cjs.js`'s
// `plugins: [...(configGlobal.plugins || []), ...(mountGlobal.plugins ||
// [])]` — so every one of this app's existing test files, each of which
// builds its own `global: { plugins: [...] }` array, still gets this
// instance too, with no per-file change needed. `en` is loaded
// synchronously (the constructor's own default) and the `missing`
// handler throws under Vitest's `MODE === "test"` (see `i18n.ts`), so a
// component test that renders a key absent from `en.json` fails loudly
// instead of showing a raw dotted key.
config.global.plugins.push(createAppI18n());
