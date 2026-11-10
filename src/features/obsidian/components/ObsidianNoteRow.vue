<script setup lang="ts">
import { useToast } from "@nuxt/ui/composables";
import { useObsidianStore } from "../stores/obsidianStore";
import type { AppError, ObsidianNote } from "@/types";

const props = defineProps<{ note: ObsidianNote }>();

const store = useObsidianStore();
const toast = useToast();

async function open(): Promise<void> {
  try {
    await store.open(props.note.id);
  } catch (err: unknown) {
    toast.add({ title: (err as AppError).message, color: "error", icon: "i-lucide-circle-alert" });
  }
}
</script>

<template>
  <li class="flex items-center gap-3 px-4 py-2.5">
    <UIcon name="i-lucide-notebook-pen" class="size-4 shrink-0 text-muted" />
    <div class="min-w-0 flex-1">
      <ULink
        as="button"
        class="block max-w-full truncate text-left"
        title="Open in Obsidian"
        @click="open"
      >
        {{ note.title }}
      </ULink>
      <p class="truncate text-xs text-muted">
        {{ note.vault }}<template v-if="note.folder"> · {{ note.folder }}</template>
      </p>
    </div>
    <slot name="actions" />
  </li>
</template>
