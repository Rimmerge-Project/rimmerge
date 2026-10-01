// ESLint flat config: template-aware linting only, via `eslint-plugin-vue`
// (its `flat/recommended` config) — Biome owns everything else (formatting,
// plain-`.ts` linting). `@typescript-eslint/parser` is wired in only as
// the parser `vue-eslint-parser` hands `<script>` blocks to, so it can
// parse TypeScript syntax — no `typescript-eslint` rule plugin here,
// per this project's convention of keeping ESLint minimal.
import vueParser from "vue-eslint-parser";
import tsParser from "@typescript-eslint/parser";
import pluginVue from "eslint-plugin-vue";

export default [
  {
    ignores: ["dist/**", "src-tauri/**", "src/types/generated/**", "node_modules/**"],
  },
  ...pluginVue.configs["flat/recommended"],
  {
    files: ["**/*.vue"],
    languageOptions: {
      parser: vueParser,
      parserOptions: {
        parser: tsParser,
        extraFileExtensions: [".vue"],
        sourceType: "module",
      },
    },
  },
  // `vue/no-bare-strings-in-template` (localization): every `.vue` file
  // now goes through it — the ratchet that grew this rule's own `files`
  // list batch by batch, component by component, is done (its last form
  // is preserved in version control history for reference). `attributes`
  // extends the rule's own default (title/aria-*/input-placeholder/
  // img-alt) with the PrimeVue props this app actually passes
  // user-facing text through.
  {
    files: ["**/*.vue"],
    rules: {
      "vue/no-bare-strings-in-template": [
        "error",
        {
          // Extends the rule's own default allowlist (punctuation, the
          // dash variants, `·`) with the edge-relationship arrow this
          // app renders throughout (`{after} → {before}`) and the `×`
          // close-button glyph (always paired with its own translated
          // `aria-label`) — glyphs, never translated text, the same
          // category as the default list's own dashes.
          allowlist: [
            "(",
            ")",
            ",",
            ".",
            "&",
            "+",
            "-",
            "=",
            "*",
            "/",
            "#",
            "%",
            "!",
            "?",
            ":",
            "[",
            "]",
            "{",
            "}",
            "<",
            ">",
            "·",
            "•",
            "‐",
            "–",
            "—",
            "−",
            "|",
            "→",
            "×",
          ],
          attributes: {
            "/.+/": [
              "title",
              "aria-label",
              "aria-placeholder",
              "aria-roledescription",
              "aria-valuetext",
              "label",
              "header",
              "placeholder",
              "summary",
              "detail",
              "emptyMessage",
            ],
            input: ["placeholder"],
            img: ["alt"],
          },
        },
      ],
    },
  },
];
