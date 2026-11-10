<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { watchDebounced } from "@vueuse/core";
import { useWorkstreamsStore } from "../stores/workstreamsStore";
import type { AppError, WorkstreamItemRef, WorkstreamItemSearch } from "@/types";

/** "New workstream" dialog: a name plus optional items picked by searching all item types. */
const props = defineProps<{
  /** Persists the workstream; rejecting with an `AppError` keeps the modal open and shows the message. */
  save: (name: string, items: WorkstreamItemRef[]) => Promise<unknown>;
}>();

const open = defineModel<boolean>("open", { default: false });

const store = useWorkstreamsStore();
const { workstreams } = storeToRefs(store);

const name = ref("");
const error = ref<string | null>(null);
const saving = ref(false);

/** A search result of any type, flattened for display. */
interface Candidate {
  key: string;
  item: WorkstreamItemRef;
  /** Short reference shown as a badge, e.g. the Jira key or PR number. */
  badge: string | null;
  title: string;
  details: string[];
}

const query = ref("");
const results = ref<WorkstreamItemSearch | null>(null);
const searching = ref(false);
const searchError = ref<string | null>(null);
/** Picked items by `Candidate.key`; kept while the query changes. */
const selected = ref(new Map<string, Candidate>());

/** Ignores responses to queries that were superseded while in flight. */
let searchSeq = 0;

async function search(term: string): Promise<void> {
  const seq = ++searchSeq;
  searchError.value = null;
  if (!term.trim()) {
    results.value = null;
    searching.value = false;
    return;
  }
  searching.value = true;
  try {
    const found = await store.searchItems(term);
    if (seq === searchSeq) results.value = found;
  } catch (err: unknown) {
    if (seq === searchSeq) searchError.value = (err as AppError).message;
  } finally {
    if (seq === searchSeq) searching.value = false;
  }
}

watchDebounced(query, (term) => void search(term), { debounce: 250 });

function workstreamName(id: number | null): string | null {
  if (id === null) return null;
  return workstreams.value.find((w) => w.id === id)?.name ?? null;
}

function candidate(
  item: WorkstreamItemRef,
  badge: string | null,
  title: string,
  details: (string | null | undefined)[],
  workstreamId: number | null,
): Candidate {
  const current = workstreamName(workstreamId);
  return {
    key: `${item.itemType}:${item.itemId}`,
    item,
    badge,
    title,
    details: [...details, current && `in ${current}`].filter((d): d is string => !!d),
  };
}

const groups = computed(() => {
  const found = results.value;
  if (!found) return [];
  return [
    {
      label: "Jira issues",
      icon: "i-lucide-ticket",
      items: found.jiraIssues.map((issue) =>
        candidate(
          { itemType: "jiraIssue", itemId: issue.id },
          issue.key,
          issue.summary,
          [issue.statusName],
          issue.workstreamId,
        ),
      ),
    },
    {
      label: "Pull requests",
      icon: "i-lucide-git-pull-request",
      items: found.pullRequests.map((pr) =>
        candidate(
          { itemType: "pullRequest", itemId: pr.id },
          `#${pr.number}`,
          pr.title,
          [pr.repository, pr.state === "open" ? null : pr.state],
          pr.workstreamId,
        ),
      ),
    },
    {
      label: "Todos",
      icon: "i-lucide-list-todo",
      items: found.todos.map((todo) =>
        candidate(
          { itemType: "todo", itemId: String(todo.id) },
          null,
          todo.title,
          [todo.completedAt === null ? null : "completed"],
          todo.workstreamId,
        ),
      ),
    },
    {
      label: "Obsidian notes",
      icon: "i-lucide-notebook-pen",
      items: found.obsidianNotes.map((note) =>
        candidate(
          { itemType: "obsidianNote", itemId: note.id },
          null,
          note.title,
          [note.vault, note.folder],
          note.workstreamId,
        ),
      ),
    },
  ].filter((group) => group.items.length);
});

const matches = computed(() => groups.value.flatMap((group) => group.items));
const allMatchesSelected = computed(
  () => matches.value.length > 0 && matches.value.every((c) => selected.value.has(c.key)),
);

function toggle(c: Candidate, checked: boolean): void {
  if (checked) selected.value.set(c.key, c);
  else selected.value.delete(c.key);
}

function toggleAllMatches(): void {
  const select = !allMatchesSelected.value;
  for (const c of matches.value) toggle(c, select);
}

watch(open, (isOpen) => {
  if (!isOpen) return;
  name.value = "";
  error.value = null;
  query.value = "";
  results.value = null;
  searchError.value = null;
  selected.value.clear();
});

async function submit(): Promise<void> {
  saving.value = true;
  error.value = null;
  try {
    await props.save(
      name.value,
      [...selected.value.values()].map((c) => c.item),
    );
    open.value = false;
  } catch (err: unknown) {
    error.value = (err as AppError).message;
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <UModal v-model:open="open" title="New workstream" :ui="{ content: 'sm:max-w-2xl' }">
    <template #body>
      <form id="workstream-create-form" class="flex flex-col gap-4" @submit.prevent="submit">
        <UAlert
          v-if="error"
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          :title="error"
        />

        <UFormField label="Name">
          <UInput v-model="name" autofocus placeholder="e.g. Checkout redesign" class="w-full" />
        </UFormField>

        <UFormField
          label="Items"
          hint="Optional"
          description="Find synced Jira issues, pull requests, todos and Obsidian notes that contain a search term."
          :error="searchError ?? undefined"
          :help="results?.obsidianError ? `Obsidian notes not searched: ${results.obsidianError}` : undefined"
        >
          <UInput
            v-model="query"
            icon="i-lucide-search"
            placeholder="Search by key, title, repository…"
            :loading="searching"
            class="w-full"
            @keydown.enter.prevent
          />
        </UFormField>

        <div v-if="selected.size" class="flex flex-wrap items-center gap-1.5">
          <span class="mr-1 text-sm text-muted">{{ selected.size }} selected:</span>
          <UButton
            v-for="c in selected.values()"
            :key="c.key"
            color="neutral"
            variant="subtle"
            size="xs"
            trailing-icon="i-lucide-x"
            :label="c.badge ?? c.title"
            :title="`Remove ${c.title}`"
            class="max-w-56"
            :ui="{ label: 'truncate' }"
            @click="selected.delete(c.key)"
          />
          <UButton
            color="neutral"
            variant="link"
            size="xs"
            label="Clear"
            @click="selected.clear()"
          />
        </div>

        <div v-if="matches.length" class="rounded-md border border-default">
          <div class="flex items-center justify-between border-b border-default px-3 py-1.5">
            <span class="text-sm text-muted">
              {{ matches.length === 1 ? "1 match" : `${matches.length} matches` }}
            </span>
            <UButton
              color="neutral"
              variant="ghost"
              size="xs"
              :icon="allMatchesSelected ? 'i-lucide-square-minus' : 'i-lucide-check-check'"
              :label="allMatchesSelected ? 'Deselect all' : 'Select all'"
              @click="toggleAllMatches"
            />
          </div>
          <div class="max-h-80 overflow-y-auto">
            <section v-for="group in groups" :key="group.label">
              <h3
                class="sticky top-0 flex items-center gap-1.5 bg-elevated px-3 py-1 text-xs font-medium text-muted"
              >
                <UIcon :name="group.icon" class="size-3.5" />
                {{ group.label }}
              </h3>
              <ul class="divide-y divide-default">
                <li v-for="c in group.items" :key="c.key">
                  <UCheckbox
                    :model-value="selected.has(c.key)"
                    :ui="{ root: 'px-3 py-2', wrapper: 'min-w-0 flex-1' }"
                    @update:model-value="toggle(c, $event === true)"
                  >
                    <template #label>
                      <span class="flex min-w-0 items-center gap-2">
                        <UBadge
                          v-if="c.badge"
                          color="neutral"
                          variant="subtle"
                          size="sm"
                          class="shrink-0 font-mono"
                          :label="c.badge"
                        />
                        <span class="truncate">{{ c.title }}</span>
                      </span>
                    </template>
                    <template v-if="c.details.length" #description>
                      <span class="block truncate">{{ c.details.join(" · ") }}</span>
                    </template>
                  </UCheckbox>
                </li>
              </ul>
            </section>
          </div>
        </div>
        <p
          v-else-if="results && !searching"
          class="rounded-md border border-dashed border-default px-3 py-6 text-center text-sm text-muted"
        >
          No items contain “{{ query.trim() }}”.
        </p>
      </form>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" label="Cancel" @click="open = false" />
        <UButton
          type="submit"
          form="workstream-create-form"
          :label="selected.size ? `Create with ${selected.size} ${selected.size === 1 ? 'item' : 'items'}` : 'Create'"
          :loading="saving"
        />
      </div>
    </template>
  </UModal>
</template>
