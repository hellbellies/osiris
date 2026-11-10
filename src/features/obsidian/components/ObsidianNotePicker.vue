<script setup lang="ts">
import { ref, watch } from "vue";
import { watchDebounced } from "@vueuse/core";
import { useObsidianStore } from "../stores/obsidianStore";
import type { AppError, ObsidianNote } from "@/types";

/** Dialog to find notes in the open vaults and pick them one by one. */
const props = defineProps<{
  /** Adds a note; rejecting with an `AppError` shows the message. */
  pick: (note: ObsidianNote) => Promise<unknown>;
  /** Notes already picked, shown as added. */
  pickedIds?: string[];
}>();

const open = defineModel<boolean>("open", { default: false });

const store = useObsidianStore();
const query = ref("");
const results = ref<ObsidianNote[] | null>(null);
const searching = ref(false);
const error = ref<string | null>(null);
const adding = ref<string | null>(null);

/** Ignores responses to queries that were superseded while in flight. */
let searchSeq = 0;

async function search(term: string): Promise<void> {
  const seq = ++searchSeq;
  error.value = null;
  if (!term.trim()) {
    results.value = null;
    searching.value = false;
    return;
  }
  searching.value = true;
  try {
    const found = await store.search(term);
    if (seq === searchSeq) results.value = found;
  } catch (err: unknown) {
    if (seq === searchSeq) error.value = (err as AppError).message;
  } finally {
    if (seq === searchSeq) searching.value = false;
  }
}

watchDebounced(query, (term) => void search(term), { debounce: 250 });

watch(open, (isOpen) => {
  if (!isOpen) return;
  query.value = "";
  results.value = null;
  error.value = null;
});

async function add(note: ObsidianNote): Promise<void> {
  adding.value = note.id;
  error.value = null;
  try {
    await props.pick(note);
  } catch (err: unknown) {
    error.value = (err as AppError).message;
  } finally {
    adding.value = null;
  }
}
</script>

<template>
  <UModal v-model:open="open" title="Add Obsidian notes" :ui="{ content: 'sm:max-w-2xl' }">
    <template #body>
      <div class="flex flex-col gap-4">
        <UFormField
          description="Searches note names and text in the vaults open in Obsidian."
          :error="error ?? undefined"
        >
          <UInput
            v-model="query"
            autofocus
            icon="i-lucide-search"
            placeholder="Search notes…"
            :loading="searching"
            class="w-full"
          />
        </UFormField>

        <ul
          v-if="results?.length"
          class="max-h-80 divide-y divide-default overflow-y-auto rounded-md border border-default"
        >
          <li v-for="note in results" :key="note.id" class="flex items-center gap-3 px-3 py-2">
            <UIcon name="i-lucide-notebook-pen" class="size-4 shrink-0 text-muted" />
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm text-highlighted">{{ note.title }}</p>
              <p class="truncate text-xs text-muted">
                {{ note.vault }}<template v-if="note.folder"> · {{ note.folder }}</template>
              </p>
            </div>
            <UBadge
              v-if="pickedIds?.includes(note.id)"
              color="success"
              variant="subtle"
              size="sm"
              icon="i-lucide-check"
              label="Added"
            />
            <UButton
              v-else
              color="neutral"
              variant="subtle"
              size="xs"
              icon="i-lucide-plus"
              :label="note.workstreamId === null ? 'Add' : 'Move here'"
              :loading="adding === note.id"
              @click="add(note)"
            />
          </li>
        </ul>
        <p
          v-else-if="results && !searching"
          class="rounded-md border border-dashed border-default px-3 py-6 text-center text-sm text-muted"
        >
          No notes match “{{ query.trim() }}”.
        </p>
      </div>
    </template>
  </UModal>
</template>
