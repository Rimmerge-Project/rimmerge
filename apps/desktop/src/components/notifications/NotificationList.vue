<script setup lang="ts">
import { useI18n } from "vue-i18n";

import NotificationItem from "@/components/notifications/NotificationItem.vue";
import { useNotificationsQuery } from "@/queries/notifications";

const { t } = useI18n();
const { data: notifications, isPending } = useNotificationsQuery();
</script>

<template>
  <div class="flex w-80 flex-col gap-3 p-3">
    <h2
      id="notification-list-heading"
      class="text-text text-sm font-semibold"
    >
      {{ t("notifications.heading") }}
    </h2>

    <p
      v-if="isPending"
      class="text-text-muted text-xs"
      data-testid="notification-list-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="!notifications || notifications.length === 0"
      class="text-text-muted text-xs"
      data-testid="notification-list-empty"
    >
      {{ t("notifications.empty") }}
    </p>
    <ul
      v-else
      class="flex flex-col gap-2"
      data-testid="notification-list"
    >
      <NotificationItem
        v-for="notification in notifications"
        :key="`${notification.key.kind}:${notification.key.fingerprint}`"
        :notification="notification"
      />
    </ul>
  </div>
</template>
