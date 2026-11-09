<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { useToast } from "@nuxt/ui/composables";
import { JiraIssueRow, useJiraIssuesStore } from "@/features/jira";
import { JiraIssuePullRequests, PullRequestRow } from "@/features/github";
import { TodoEditModal, TodoRow } from "@/features/todos";
import { useWorkstreamsStore } from "../stores/workstreamsStore";
import type { AppError, Todo, WorkstreamItemRef } from "@/types";

/** Lists everything in a workstream, grouped by item type. */
const props = defineProps<{ id: number }>();

const store = useWorkstreamsStore();
const { current, currentError } = storeToRefs(store);
const jiraStore = useJiraIssuesStore();
const toast = useToast();

async function removeItem(item: WorkstreamItemRef): Promise<void> {
  try {
    await store.assign(item, null);
  } catch (err: unknown) {
    toast.add({ title: (err as AppError).message, color: "error", icon: "i-lucide-circle-alert" });
  }
}

/** Reloads this workstream and the list (item counts) after an item changed. */
async function refresh(): Promise<void> {
  await Promise.all([store.loadDetail(props.id), store.load()]);
}

// One dialog for adding a todo to this workstream (no `todo`) or editing one.
const todoModalOpen = ref(false);
const editingTodo = ref<Todo | null>(null);

function openTodo(todo: Todo | null): void {
  editingTodo.value = todo;
  todoModalOpen.value = true;
}

watch(() => props.id, (id) => void store.loadDetail(id));
onMounted(() => {
  void store.loadDetail(props.id);
  // Status options give the status badges their colors.
  if (!jiraStore.loaded) void jiraStore.load();
});
</script>

<template>
  <UAlert
    v-if="currentError"
    color="error"
    variant="subtle"
    icon="i-lucide-circle-alert"
    :title="currentError.message"
  />

  <div v-else-if="current" class="flex flex-col gap-6">
    <UCard :ui="{ body: 'p-0 sm:p-0' }">
      <template #header>
        <div class="flex items-center gap-2">
          <UIcon name="i-lucide-list-todo" class="size-4 text-muted" />
          <h2 class="font-semibold text-highlighted">Todos</h2>
          <UBadge color="neutral" variant="subtle" size="sm" :label="current.todos.length" />
          <UButton
            class="ml-auto"
            color="neutral"
            variant="ghost"
            size="sm"
            icon="i-lucide-plus"
            label="Add todo"
            @click="openTodo(null)"
          />
        </div>
      </template>

      <ul v-if="current.todos.length" class="divide-y divide-default">
        <TodoRow
          v-for="todo in current.todos"
          :key="todo.id"
          :todo="todo"
          @edit="openTodo"
          @changed="refresh"
        >
          <template #actions>
            <UTooltip text="Remove from workstream (keeps the todo)">
              <UButton
                color="neutral"
                variant="ghost"
                size="sm"
                icon="i-lucide-x"
                :aria-label="`Remove ${todo.title} from workstream`"
                @click="removeItem({ itemType: 'todo', itemId: String(todo.id) })"
              />
            </UTooltip>
          </template>
        </TodoRow>
      </ul>
      <p v-else class="px-4 py-6 text-center text-sm text-muted">
        No todos yet. Use “Add todo” to create one in this workstream.
      </p>
    </UCard>

    <TodoEditModal
      v-model:open="todoModalOpen"
      :todo="editingTodo"
      :workstream-id="id"
      @changed="refresh"
    />

    <UCard :ui="{ body: 'p-0 sm:p-0' }">
      <template #header>
        <div class="flex items-center gap-2">
          <UIcon name="i-lucide-ticket" class="size-4 text-muted" />
          <h2 class="font-semibold text-highlighted">Jira issues</h2>
          <UBadge color="neutral" variant="subtle" size="sm" :label="current.jiraIssues.length" />
        </div>
      </template>

      <ul v-if="current.jiraIssues.length" class="divide-y divide-default">
        <JiraIssueRow v-for="issue in current.jiraIssues" :key="issue.id" :issue="issue">
          <template #related>
            <JiraIssuePullRequests :issue-id="issue.id" />
          </template>
          <template #actions>
            <UTooltip text="Remove from workstream">
              <UButton
                color="neutral"
                variant="ghost"
                size="sm"
                icon="i-lucide-x"
                :aria-label="`Remove ${issue.key} from workstream`"
                @click="removeItem({ itemType: 'jiraIssue', itemId: issue.id })"
              />
            </UTooltip>
          </template>
        </JiraIssueRow>
      </ul>
      <UEmpty
        v-else
        class="py-10"
        icon="i-lucide-ticket"
        title="No Jira issues yet"
        description="Add issues to this workstream from the Jira issues list."
        :actions="[
          { label: 'Go to Jira issues', icon: 'i-lucide-arrow-right', color: 'neutral', variant: 'subtle', to: '/jira' },
        ]"
      />
    </UCard>

    <UCard :ui="{ body: 'p-0 sm:p-0' }">
      <template #header>
        <div class="flex items-center gap-2">
          <UIcon name="i-lucide-git-pull-request" class="size-4 text-muted" />
          <h2 class="font-semibold text-highlighted">Pull requests</h2>
          <UBadge color="neutral" variant="subtle" size="sm" :label="current.pullRequests.length" />
        </div>
      </template>

      <ul v-if="current.pullRequests.length" class="divide-y divide-default">
        <PullRequestRow
          v-for="pr in current.pullRequests"
          :key="pr.id"
          :pull-request="pr"
          @refreshed="store.applyPullRequest"
        >
          <template #actions>
            <UTooltip text="Remove from workstream">
              <UButton
                color="neutral"
                variant="ghost"
                size="sm"
                icon="i-lucide-x"
                :aria-label="`Remove ${pr.repository}#${pr.number} from workstream`"
                @click="removeItem({ itemType: 'pullRequest', itemId: pr.id })"
              />
            </UTooltip>
          </template>
        </PullRequestRow>
      </ul>
      <UEmpty
        v-else
        class="py-10"
        icon="i-lucide-git-pull-request"
        title="No pull requests yet"
        description="Add pull requests to this workstream from the pull requests list."
        :actions="[
          { label: 'Go to pull requests', icon: 'i-lucide-arrow-right', color: 'neutral', variant: 'subtle', to: '/pull-requests' },
        ]"
      />
    </UCard>
  </div>
</template>
