<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@nuxt/ui/composables";
import { usePullRequestsStore } from "../stores/pullRequestsStore";
import { pullRequestStatus } from "./pullRequestStatus";
import type { AppError, PullRequest } from "@/types";

const props = defineProps<{ pullRequest: PullRequest }>();

const emit = defineEmits<{
  /** The pull request was re-fetched from GitHub; for lists keeping their own copy. */
  refreshed: [pullRequest: PullRequest];
}>();

const store = usePullRequestsStore();
const toast = useToast();
const refreshing = ref(false);

async function refresh(): Promise<void> {
  refreshing.value = true;
  try {
    emit("refreshed", await store.refresh(props.pullRequest.id));
  } catch (err: unknown) {
    toast.add({ title: (err as AppError).message, color: "error", icon: "i-lucide-circle-alert" });
  } finally {
    refreshing.value = false;
  }
}

const statusBadge = computed(() => pullRequestStatus(props.pullRequest));

const involvesMe = computed(
  () => props.pullRequest.authoredByMe || props.pullRequest.reviewRequested,
);
</script>

<template>
  <li class="flex items-center gap-3 px-4 py-2.5">
    <UBadge
      color="neutral"
      variant="subtle"
      class="shrink-0 font-mono"
      :label="`#${pullRequest.number}`"
    />
    <div class="min-w-0 flex-1">
      <ULink
        as="button"
        class="block max-w-full truncate text-left"
        :title="`Open ${pullRequest.repository}#${pullRequest.number} on GitHub`"
        @click="store.open(pullRequest.id)"
      >
        {{ pullRequest.title }}
      </ULink>
      <p class="truncate text-xs text-muted">
        {{ pullRequest.repository }}<template v-if="pullRequest.author"> · {{ pullRequest.author }}</template>
      </p>
    </div>
    <UTooltip v-if="pullRequest.reviewRequested" text="Your review is requested">
      <UIcon name="i-lucide-eye" class="size-4 shrink-0 text-warning" />
    </UTooltip>
    <UTooltip
      v-if="!involvesMe && !pullRequest.otherOpen"
      text="No longer needs you (kept for its workstream)"
    >
      <UIcon name="i-lucide-user-x" class="size-4 shrink-0 text-muted" />
    </UTooltip>
    <UBadge
      :color="statusBadge.color"
      variant="subtle"
      class="shrink-0"
      :label="statusBadge.label"
    />
    <UTooltip text="Refresh from GitHub">
      <UButton
        color="neutral"
        variant="ghost"
        size="sm"
        icon="i-lucide-refresh-cw"
        :loading="refreshing"
        :aria-label="`Refresh ${pullRequest.repository}#${pullRequest.number}`"
        @click="refresh"
      />
    </UTooltip>
    <slot name="actions" />
  </li>
</template>
