import type en from "@/locales/en.json";

// Augments vue-i18n's own schema types with `en`'s shape, so `t("no.such.key")`
// (and a `MessageDescriptor.key` rendered through it) fails `vue-tsc` — the
// English catalogue is the schema every locale, including `en` itself, is
// checked against.
declare module "vue-i18n" {
  export interface DefineLocaleMessage extends Readonly<typeof en> {}
}
