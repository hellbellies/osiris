<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@nuxt/ui/composables";
import { useTodosStore } from "../stores/todosStore";
import TodoDescription from "./TodoDescription.vue";
import type { AppError, Todo } from "@/types";

const props = defineProps<{ todo: Todo }>();

const emit = defineEmits<{
  /** The title was clicked. */
  edit: [todo: Todo];
  /** The todo was completed or reopened. */
  changed: [todo: Todo];
}>();

const store = useTodosStore();
const toast = useToast();
const toggling = ref(false);

const done = computed(() => props.todo.completedAt !== null);

async function toggle(): Promise<void> {
  toggling.value = true;
  try {
    emit("changed", await store.setCompleted(props.todo.id, !done.value));
  } catch (err: unknown) {
    toast.add({ title: (err as AppError).message, color: "error", icon: "i-lucide-circle-alert" });
  } finally {
    toggling.value = false;
  }
}
</script>

<template>
  <li class="flex items-start gap-3 px-4 py-2.5">
    <!-- Rows align to the top for multi-line descriptions; keep the checkbox on the title's line. -->
    <div class="flex h-6 items-center">
      <UCheckbox
        :model-value="done"
        :disabled="toggling"
        :aria-label="done ? `Reopen ${todo.title}` : `Complete ${todo.title}`"
        @update:model-value="toggle"
      />
    </div>
    <div class="min-w-0 flex-1">
      <ULink
        as="button"
        class="block max-w-full truncate text-left"
        :class="done ? 'text-muted line-through' : ''"
        title="Edit todo"
        @click="emit('edit', todo)"
      >
        {{ todo.title }}
      </ULink>
      <TodoDescription v-if="todo.description.trim()" :markdown="todo.description" class="mt-0.5" />
    </div>
    <slot name="actions" />
  </li>
</template>
