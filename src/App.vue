<script setup lang="ts">
import { computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import type { NavigationMenuItem } from "@nuxt/ui";
import { useToast } from "@nuxt/ui/composables";
import { useSettingsStore, useThemeSync } from "@/features/settings";
import { useJiraIssuesStore } from "@/features/jira";
import { usePullRequestsStore } from "@/features/github";
import { useTodosStore } from "@/features/todos";
import { useWorkstreamsStore } from "@/features/workstreams";
import { useObsidianStore } from "@/features/obsidian";
import type { ObsidianStatus } from "@/types";
import SidebarFetch from "@/components/SidebarFetch.vue";

const settingsStore = useSettingsStore();
useThemeSync(computed(() => settingsStore.settings?.theme));

const workstreamsStore = useWorkstreamsStore();
const jiraStore = useJiraIssuesStore();
const pullRequestsStore = usePullRequestsStore();
const todosStore = useTodosStore();
// Completed workstreams stay reachable from the overview's "Completed" tab.
const { active: activeWorkstreams } = storeToRefs(workstreamsStore);
const { issues } = storeToRefs(jiraStore);
const { pullRequests } = storeToRefs(pullRequestsStore);
/** The badge counts what needs the user, not the other open pull requests. */
const myPullRequests = computed(() =>
  pullRequests.value.filter((pr) => pr.authoredByMe || pr.reviewRequested),
);
const { open: openTodos } = storeToRefs(todosStore);

const mainItems = computed<NavigationMenuItem[]>(() => [
  { label: "Workstreams", icon: "i-lucide-layers", to: "/workstreams", exact: true },
  {
    label: "Jira issues",
    icon: "i-lucide-ticket",
    to: "/jira",
    badge: issues.value.length || undefined,
  },
  {
    label: "Pull requests",
    icon: "i-lucide-git-pull-request",
    to: "/pull-requests",
    badge: myPullRequests.value.length || undefined,
  },
  {
    label: "Todos",
    icon: "i-lucide-list-todo",
    to: "/todos",
    badge: openTodos.value.length || undefined,
  },
]);

const workstreamItems = computed<NavigationMenuItem[]>(() =>
  activeWorkstreams.value.length === 0
    ? []
    : [
        { type: "label", label: "Your workstreams" },
        ...activeWorkstreams.value.map((workstream) => ({
          label: workstream.name,
          icon: "i-lucide-folder",
          to: `/workstreams/${workstream.id}`,
          badge: workstream.itemCount || undefined,
        })),
      ],
);

const footerItems: NavigationMenuItem[] = [
  { label: "Settings", icon: "i-lucide-settings", to: "/settings" },
];

const obsidianStore = useObsidianStore();
const toast = useToast();

/**
 * Warns at startup when Obsidian is installed but can't be used for notes. Not
 * installed or not running stays quiet: that is normal, and Settings shows it.
 */
function warnAboutObsidian(status: ObsidianStatus | null): void {
  const problems: Partial<Record<ObsidianStatus["state"], string>> = {
    outdated: `Obsidian ${status?.version ?? ""} is too old for its command line interface; 1.12.7 or later is needed.`,
    cliDisabled: "Turn on its command line interface: Obsidian → Settings → General → Advanced.",
    failed: status?.message ?? "Obsidian answered unexpectedly.",
  };
  const description = status && problems[status.state];
  if (!description) return;
  toast.add({
    title: "Obsidian notes unavailable",
    description,
    icon: "i-lucide-triangle-alert",
    color: "warning",
    actions: [{ label: "Settings", color: "neutral", variant: "subtle", to: "/settings" }],
  });
}

// Everything here is local data; the theme in particular is needed before any page renders.
onMounted(() => {
  void obsidianStore.checkStatus().then(warnAboutObsidian);
  if (!settingsStore.settings) void settingsStore.load();
  void workstreamsStore.load();
  void jiraStore.load();
  void pullRequestsStore.load();
  void todosStore.load();
});
</script>

<template>
  <UApp>
    <UDashboardGroup>
      <UDashboardSidebar collapsible resizable :ui="{ footer: 'border-t border-default' }">
        <template #header="{ collapsed }">
          <span class="px-1 text-lg font-semibold tracking-tight text-highlighted">
            {{ collapsed ? "o" : "osiris" }}
          </span>
        </template>

        <template #default="{ collapsed }">
          <UNavigationMenu :collapsed="collapsed" :items="mainItems" orientation="vertical" />
          <UNavigationMenu
            v-if="workstreamItems.length"
            :collapsed="collapsed"
            :items="workstreamItems"
            orientation="vertical"
            class="mt-4"
          />
        </template>

        <template #footer="{ collapsed }">
          <div class="flex w-full flex-col gap-3">
            <SidebarFetch :collapsed="collapsed" />
            <UNavigationMenu
              :collapsed="collapsed"
              :items="footerItems"
              orientation="vertical"
              class="w-full"
            />
          </div>
        </template>
      </UDashboardSidebar>

      <RouterView />
    </UDashboardGroup>
  </UApp>
</template>
