import { createRouter, createWebHistory } from "vue-router";

import { isStaleOrUnapplied, requestStaleConfirmation } from "@/composables/useStaleActiveSetGuard";
import { useSessionStore } from "@/stores/session";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: "/setup",
      name: "setup",
      component: () => import("@/pages/SetupPage.vue"),
    },
    {
      path: "/",
      component: () => import("@/layouts/TheShell.vue"),
      children: [
        {
          path: "",
          name: "dashboard",
          component: () => import("@/pages/DashboardPage.vue"),
        },
        {
          path: "inbox",
          name: "inbox",
          component: () => import("@/pages/InboxPage.vue"),
        },
        {
          path: "merge/:key",
          name: "merge-editor",
          component: () => import("@/pages/MergeEditorPage.vue"),
        },
        {
          path: "merge-mod",
          name: "merge-mod",
          component: () => import("@/pages/MergeModPage.vue"),
        },
        {
          path: "patches",
          name: "patches",
          component: () => import("@/pages/PatchListPage.vue"),
        },
        {
          path: "patches/:patchId",
          name: "patch-detail",
          component: () => import("@/pages/PatchDetailPage.vue"),
        },
        {
          // The same merge editor, scoped to the patch: `MergeEditorPage` reads
          // this route's `patchId` param and passes it to
          // `useMergeFieldPage`/`useMergeChoices`.
          path: "patches/:patchId/merge/:key",
          name: "patch-merge-editor",
          component: () => import("@/pages/MergeEditorPage.vue"),
        },
        {
          path: "assignments",
          name: "assignments",
          component: () => import("@/pages/AssignmentListPage.vue"),
        },
        {
          path: "assignments/new",
          name: "assignment-new",
          component: () => import("@/pages/AssignmentWizardPage.vue"),
        },
        {
          path: "assignments/:assignmentId",
          name: "assignment-detail",
          component: () => import("@/pages/AssignmentEditorPage.vue"),
        },
        {
          path: "startup",
          name: "startup",
          component: () => import("@/pages/StartupPage.vue"),
        },
        {
          path: "order",
          name: "order",
          component: () => import("@/pages/LoadOrderPage.vue"),
        },
        {
          path: "order/:modId",
          name: "order-mod",
          component: () => import("@/pages/LoadOrderPage.vue"),
        },
        {
          path: "rules",
          name: "rules",
          component: () => import("@/pages/RulesPage.vue"),
        },
        {
          path: "mods",
          name: "mods",
          component: () => import("@/pages/ModsPage.vue"),
        },
        {
          // The mod info panel — the same "selection lives in the URL"
          // pattern `/order/:modId` uses for `WhyPanel`. `mod-detail` is
          // the established route *name* every caller (`InboxPage`'s
          // `g` shortcut, `PatchInbox`, `ModsPage`'s own row click)
          // already pushes to; it now opens the panel over the mod
          // list instead of the full page, which moved to
          // `mods/:modId/details` (`mod-detail-page`) below.
          path: "mods/:modId",
          name: "mod-detail",
          component: () => import("@/pages/ModsPage.vue"),
        },
        {
          path: "mods/:modId/details",
          name: "mod-detail-page",
          component: () => import("@/pages/ModDetailPage.vue"),
        },
        {
          // `defRef` can contain a literal `/` (e.g. `ThingDef/Wall`) —
          // `vue-router` already percent-encodes/decodes a dynamic
          // segment's param value, so every link built with `params:
          // { defRef }` (see `utils/format.ts`'s `inspectHref`) round-trips
          // without any manual encoding at the call site.
          path: "defs/:defRef",
          name: "def",
          component: () => import("@/pages/DefPage.vue"),
        },
        {
          path: "settings",
          name: "settings",
          component: () => import("@/pages/SettingsPage.vue"),
        },
      ],
    },
  ],
});

// Every page but setup needs a loaded project, guarded here so no
// individual page has to check it itself.
router.beforeEach(async (to, from) => {
  const session = useSessionStore();
  if (to.name !== "setup" && !session.loaded) {
    return { name: "setup" };
  }
  // Warn on project switch: navigating away to `/setup` while a
  // project is loaded and the working active-mod set has pending
  // changes is the closest thing this app has to "switch projects" —
  // confirm first, the same dialog `PendingChangesCloseGuard.vue` shows
  // on window close.
  if (to.name === "setup" && from.name !== "setup" && session.loaded && isStaleOrUnapplied()) {
    const leave = await requestStaleConfirmation();
    if (!leave) {
      return false;
    }
  }
  return true;
});
