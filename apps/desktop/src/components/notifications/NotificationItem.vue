<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { releaseUrl, useNotificationActions } from "@/composables/useNotificationActions";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import {
  useDismissNotificationMutation,
  useMuteNotificationKindMutation,
} from "@/queries/notifications";
import type { FreshnessDto } from "@/types/generated/FreshnessDto";
import type { NotificationDto } from "@/types/generated/NotificationDto";
import type { RuleDatabaseDto } from "@/types/generated/RuleDatabaseDto";
import type { SourceSetupDto } from "@/types/generated/SourceSetupDto";
import { assertNever } from "@/utils/assertNever";
import { describeFetchFailure, fetchFailureTechnicalDetail } from "@/utils/fetchFailure";
import {
  notificationActionLabel,
  notificationBody,
  notificationTitle,
} from "@/utils/notifications";
import { DATABASE_LABEL_KEYS } from "@/utils/ruleDatabases";

const { notification } = defineProps<{ notification: NotificationDto }>();

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const { runAction } = useNotificationActions();
const { mutateAsync: dismiss, isLoading: dismissing } = useDismissNotificationMutation();
const { mutateAsync: mute, isLoading: muting } = useMuteNotificationKindMutation();

const title = computed(() => notificationTitle(notification));
const body = computed(() => notificationBody(notification));
const severityLabel = computed(() =>
  notification.severity === "warn"
    ? t("notifications.severityWarn")
    : t("notifications.severityInfo"),
);

/**
 * The release page URL for an `updateAvailable` notice, rendered below
 * the body as selectable plain text — the fallback `showReleasePage`'s
 * own clipboard-failure path promises (`useNotificationActions.ts`),
 * for a webview that blocks clipboard access.
 */
const releaseLink = computed(() =>
  notification.data.kind === "updateAvailable" ? releaseUrl(notification.data.latestVersion) : null,
);

/** Only present for the two rule-database notices — the per-source detail list the generic body doesn't spell out. */
const staleSources = computed(() =>
  notification.data.kind === "ruleDatabasesStale"
    ? Object.entries(notification.data.sources)
    : null,
);
const importedRulesSources = computed(() =>
  notification.data.kind === "importedRulesOutdated"
    ? (Object.keys(notification.data.sources) as RuleDatabaseDto[])
    : null,
);
const importedRulesSourcesText = computed(() =>
  importedRulesSources.value === null
    ? null
    : formatList(
        locale.value,
        importedRulesSources.value.map((database) => t(DATABASE_LABEL_KEYS[database])),
      ),
);

/** Per-source "off" / "on, not downloaded yet" lines for the recommended-sources notice. */
const recommendedSourceLines = computed(() =>
  notification.data.kind === "recommendedSourcesIncomplete"
    ? Object.entries(notification.data.sources).map(([database, setup]) => {
        const source = t(DATABASE_LABEL_KEYS[database as RuleDatabaseDto]);
        return {
          database,
          text: recommendedSourceText(setup, source),
        };
      })
    : null,
);

function recommendedSourceText(setup: SourceSetupDto, source: string): string {
  switch (setup) {
    case "off":
      return t("notifications.recommendedSources.sourceOff", { source });
    case "notDownloaded":
      return t("notifications.recommendedSources.sourceNotDownloaded", { source });
    default:
      return assertNever(setup);
  }
}

function freshnessLabel(freshness: FreshnessDto): string {
  switch (freshness.kind) {
    case "neverFetched":
      return t("notifications.ruleDatabasesStale.neverFetched");
    case "fetchedAt":
      return t("notifications.ruleDatabasesStale.lastSuccess", {
        date: new Date(freshness.at).toLocaleDateString(locale.value),
      });
    default:
      return assertNever(freshness);
  }
}
</script>

<template>
  <li
    class="border-border-subtle flex flex-col gap-2 rounded border p-3 text-sm"
    :data-testid="`notification-item-${notification.key.kind}`"
  >
    <div class="flex items-start gap-2">
      <i
        class="pi shrink-0 pt-0.5 text-sm"
        :class="notification.severity === 'warn' ? 'pi-exclamation-triangle text-status-warn' : 'pi-info-circle text-status-info'"
        aria-hidden="true"
      />
      <div class="flex flex-col gap-1">
        <span class="text-text font-medium">{{ tm(title) }}</span>
        <span class="text-text-muted text-xs">{{ severityLabel }}</span>
        <p class="text-text-muted text-xs">
          {{ tm(body) }}
        </p>

        <ul
          v-if="staleSources"
          class="text-text-faint flex flex-col gap-0.5 text-xs"
        >
          <li
            v-for="[database, detail] in staleSources"
            :key="database"
          >
            {{ t(DATABASE_LABEL_KEYS[database as keyof typeof DATABASE_LABEL_KEYS]) }}:
            {{ freshnessLabel(detail.freshness) }}
            <span v-if="detail.lastFailure">
              — {{ t('notifications.ruleDatabasesStale.lastFailure', { reason: tm(describeFetchFailure(detail.lastFailure, locale)) }) }}
            </span>
            <span
              v-if="detail.lastFailure && fetchFailureTechnicalDetail(detail.lastFailure)"
              class="text-text-faint block font-mono break-all select-text"
              :data-testid="`notification-failure-detail-${database}`"
            >
              {{ t("common.technicalDetail", { detail: fetchFailureTechnicalDetail(detail.lastFailure) }) }}
            </span>
          </li>
        </ul>

        <ul
          v-if="recommendedSourceLines"
          class="text-text-faint flex flex-col gap-0.5 text-xs"
          :data-testid="`notification-sources-${notification.key.kind}`"
        >
          <li
            v-for="line in recommendedSourceLines"
            :key="line.database"
          >
            {{ line.text }}
          </li>
        </ul>

        <p
          v-if="importedRulesSourcesText"
          class="text-text-faint text-xs"
        >
          {{ importedRulesSourcesText }}
        </p>

        <p
          v-if="releaseLink"
          class="text-text-faint font-mono text-xs break-all select-text"
          data-testid="notification-release-url"
        >
          {{ releaseLink }}
        </p>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <Button
        v-for="action in notification.actions"
        :key="action"
        :label="tm(notificationActionLabel(action))"
        size="small"
        severity="secondary"
        outlined
        :data-testid="`notification-action-${notification.key.kind}-${action}`"
        @click="runAction(action, notification)"
      />
      <span class="grow" />
      <Button
        :label="t('notifications.dismiss')"
        size="small"
        text
        :loading="dismissing"
        :data-testid="`notification-dismiss-${notification.key.kind}`"
        @click="dismiss(notification.key)"
      />
      <Button
        v-if="notification.dismissal === 'occurrenceOrMute'"
        :label="t('notifications.mute')"
        size="small"
        text
        :loading="muting"
        :data-testid="`notification-mute-${notification.key.kind}`"
        @click="mute(notification.key.kind)"
      />
    </div>
  </li>
</template>
