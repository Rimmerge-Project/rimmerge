import { definePreset } from "@primevue/themes";
import Aura from "@primevue/themes/aura";

/**
 * Aura's own defaults are an emerald `primary` ramp and a slate `surface`
 * ramp in light mode — overridden here so every PrimeVue component
 * agrees with `style.css`'s tokens: one indigo accent (never competing
 * with a second, Tailwind-only accent), one zinc neutral hue in both
 * schemes, and near-black (not pure black) dark surfaces. Only
 * `semantic.primary` and each scheme's `surface`/`primary`/`text`/
 * `formField.background` are touched, per
 * design decision 1 — every other Aura default (spacing, radii,
 * per-component tokens) is inherited unchanged and derives from these
 * through Aura's own token references.
 */
export const preset = definePreset(Aura, {
  semantic: {
    primary: {
      50: "{indigo.50}",
      100: "{indigo.100}",
      200: "{indigo.200}",
      300: "{indigo.300}",
      400: "{indigo.400}",
      500: "{indigo.500}",
      600: "{indigo.600}",
      700: "{indigo.700}",
      800: "{indigo.800}",
      900: "{indigo.900}",
      950: "{indigo.950}",
    },
    colorScheme: {
      light: {
        surface: {
          0: "#ffffff",
          50: "{zinc.50}",
          100: "{zinc.100}",
          200: "{zinc.200}",
          300: "{zinc.300}",
          400: "{zinc.400}",
          500: "{zinc.500}",
          600: "{zinc.600}",
          700: "{zinc.700}",
          800: "{zinc.800}",
          900: "{zinc.900}",
          950: "{zinc.950}",
        },
        primary: {
          color: "{primary.600}",
          contrastColor: "#ffffff",
          hoverColor: "{primary.700}",
          activeColor: "{primary.800}",
        },
        text: {
          color: "{zinc.900}",
          hoverColor: "{zinc.950}",
          mutedColor: "{zinc.600}",
          hoverMutedColor: "{zinc.700}",
        },
        // `style.css`'s light `surface-2` (`#e4e4e7`, the hover/selected
        // row/input background) is exactly `{zinc.200}`, i.e. this
        // scheme's own `surface.200` above — so inputs sit on the same
        // token as everywhere else that token means "hover-tinted
        // surface", per token table.
        formField: {
          background: "{surface.200}",
        },
      },
      dark: {
        // Aura's own dark defaults for `surface` 0-700 are already the
        // zinc ramp this app wants (unlike light mode's slate default),
        // and Aura's own components read those low indices as *light*
        // text/icon colors even in the dark scheme (e.g. a secondary
        // button's dark-mode label is `{surface.300}`, a form field's
        // dark-mode text is `{surface.0}`) — overriding them with dark
        // values would make that text invisible against its own
        // background. So 0-700 are left
        // unset here and inherit Aura's own zinc defaults; only 800-950
        // (used exclusively as *background* colors throughout Aura) are
        // customized, to the same near-black, not-pure-black range
        // `style.css` uses for `surface-0`/`surface-1`/`surface-2`.
        surface: {
          800: "#232328",
          900: "#1c1c1f",
          950: "#141416",
        },
        primary: {
          color: "{primary.400}",
          contrastColor: "#141416",
          hoverColor: "{primary.300}",
          activeColor: "{primary.200}",
        },
        text: {
          color: "{zinc.100}",
          hoverColor: "#ffffff",
          mutedColor: "{zinc.400}",
          hoverMutedColor: "{zinc.300}",
        },
        // `style.css`'s dark `surface-2` (`#232328`) is exactly this
        // scheme's own `surface.800` above — same reasoning as the light
        // scheme's `formField.background` override.
        formField: {
          background: "{surface.800}",
        },
      },
    },
  },
});
