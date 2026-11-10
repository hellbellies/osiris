<script setup lang="ts">
import { computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useTimeAgo } from "@vueuse/core";
import { usePullRequestsStore } from "../stores/pullRequestsStore";
import PullRequestRow from "./PullRequestRow.vue";
import type { PullRequest } from "@/types";

defineSlots<{
  /** Extra controls at the end of each row. */
  actions?(props: { pullRequest: PullRequest }): unknown;
}>();

const store = usePullRequestsStore();
const { pullRequests, lastSyncedAt, loaded, syncing, error } = storeToRefs(store);

const lastSyncedAgo = useTimeAgo(computed(() => lastSyncedAt.value ?? 0));

/** Other people's pull requests waiting for the user's review. */
const reviewRequested = computed(() => pullRequests.value.filter((pr) => pr.reviewRequested));
/** The user's own open pull requests. */
const authored = computed(() =>
  pullRequests.value.filter((pr) => pr.authoredByMe && !pr.reviewRequested),
);
/** Everything else open in the watched repositories. */
const otherOpen = computed(() =>
  pullRequests.value.filter((pr) => pr.otherOpen && !pr.authoredByMe && !pr.reviewRequested),
);

const sections = computed(() => [
  {
    key: "review",
    title: "Review requested",
    icon: "i-lucide-eye",
    items: reviewRequested.value,
    empty: "No pull requests are waiting for your review.",
  },
  {
    key: "authored",
    title: "My pull requests",
    icon: "i-lucide-git-pull-request-arrow",
    items: authored.value,
    empty: "You have no open pull requests.",
  },
  {
    key: "other",
    title: "Other open pull requests",
    icon: "i-lucide-git-pull-request",
    items: otherOpen.value,
    empty: "No other open pull requests in the repositories you work in or have configured.",
  },
]);

// Fetching lives in the sidebar (SidebarFetch), so it is available on every page.
// Local data only, so always reload: other views may have changed workstream links.
onMounted(() => void store.load());
</script>

<template>
  <div class="flex flex-col gap-6">
    <p class="text-sm text-muted">
      <template v-if="lastSyncedAt">Last fetched {{ lastSyncedAgo }}</template>
      <template v-else>Not fetched yet</template>
    </p>

    <UAlert
      v-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-circle-alert"
      :title="error.message"
    />

    <UEmpty
      v-if="loaded && !syncing && !lastSyncedAt"
      class="py-10"
      icon="i-lucide-git-pull-request"
      title="No pull requests yet"
      description="Add a GitHub token in the settings, then use Fetch in the sidebar."
    />

    <template v-else>
      <UCard v-for="section in sections" :key="section.key" :ui="{ body: 'p-0 sm:p-0' }">
        <template #header>
          <div class="flex items-center gap-2">
            <UIcon :name="section.icon" class="size-4 text-muted" />
            <h2 class="font-semibold text-highlighted">{{ section.title }}</h2>
            <UBadge color="neutral" variant="subtle" size="sm" :label="section.items.length" />
          </div>
        </template>

        <ul v-if="section.items.length" class="divide-y divide-default">
          <PullRequestRow v-for="pr in section.items" :key="pr.id" :pull-request="pr">
            <template #actions>
              <slot name="actions" :pull-request="pr" />
            </template>
          </PullRequestRow>
        </ul>
        <p v-else class="px-4 py-6 text-center text-sm text-muted">{{ section.empty }}</p>
      </UCard>
    </template>
  </div>
</template>
