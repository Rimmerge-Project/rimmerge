<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useAppVersionQuery, useOpenAppLinkMutation } from "@/queries/links";
import type { AppLinkTargetDto } from "@/types/generated/AppLinkTargetDto";

/**
 * The Settings page's own About section: the app version and the
 * project's fixed external links (repo, issues, support). Every link
 * opens through `open_app_link` — this component never builds or sends
 * a URL, only a `target` naming which of the app's own fixed links to
 * open.
 */
const { t } = useI18n();
const { data: version } = useAppVersionQuery();
const { mutateAsync: openAppLink } = useOpenAppLinkMutation();

function open(target: AppLinkTargetDto): void {
  void openAppLink(target);
}
</script>

<template>
  <section
    class="border-border-subtle border-t pt-6"
    data-testid="settings-about"
  >
    <h2 class="text-text mb-2 text-base font-semibold">
      {{ t("settings.about.heading") }}
    </h2>
    <p
      v-if="version"
      class="text-text-muted mb-2 text-sm"
      data-testid="settings-about-version"
    >
      {{ t("settings.about.versionLabel", { version }) }}
    </p>
    <p class="text-text-muted mb-3 max-w-md text-sm">
      {{ t("settings.about.supportBlurb") }}
    </p>
    <div class="flex flex-wrap gap-3 text-sm">
      <button
        type="button"
        class="text-accent cursor-pointer underline"
        data-testid="settings-about-repo-link"
        @click="open('githubRepo')"
      >
        {{ t("settings.about.repoLink") }}
      </button>
      <button
        type="button"
        class="text-accent cursor-pointer underline"
        data-testid="settings-about-issues-link"
        @click="open('githubIssues')"
      >
        {{ t("settings.about.issuesLink") }}
      </button>
      <button
        type="button"
        class="text-accent cursor-pointer underline"
        data-testid="settings-about-support-link"
        @click="open('support')"
      >
        {{ t("settings.about.supportLink") }}
      </button>
    </div>
  </section>
</template>
