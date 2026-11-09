<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { useTodosStore } from "../stores/todosStore";
import type { AppError, Todo } from "@/types";

/** Creates a todo (no `todo` given) or edits/deletes an existing one. */
const props = defineProps<{
  todo?: Todo | null;
  /** When creating: add the new todo to this workstream. */
  workstreamId?: number | null;
}>();

/** Emitted after a todo was created, saved or deleted. */
const emit = defineEmits<{ changed: [] }>();

const open = defineModel<boolean>("open", { default: false });

const store = useTodosStore();
const form = reactive({ title: "", description: "" });
const error = ref<string | null>(null);
const saving = ref(false);
const confirmDelete = ref(false);

const editing = computed(() => !!props.todo);

watch(open, (isOpen) => {
  if (!isOpen) return;
  form.title = props.todo?.title ?? "";
  form.description = props.todo?.description ?? "";
  error.value = null;
  confirmDelete.value = false;
});

async function run(action: () => Promise<unknown>): Promise<void> {
  saving.value = true;
  error.value = null;
  try {
    await action();
    open.value = false;
    emit("changed");
  } catch (err: unknown) {
    error.value = (err as AppError).message;
  } finally {
    saving.value = false;
  }
}

function submit(): Promise<void> {
  const input = { title: form.title, description: form.description };
  return run(() =>
    props.todo ? store.update(props.todo.id, input) : store.create(input, props.workstreamId ?? null),
  );
}

function remove(): Promise<void> | void {
  // Two-step: the first click asks for confirmation.
  if (!confirmDelete.value) {
    confirmDelete.value = true;
    return;
  }
  const todo = props.todo;
  if (todo) return run(() => store.remove(todo.id));
}
</script>

<template>
  <UModal v-model:open="open" :title="editing ? 'Edit todo' : 'New todo'">
    <template #body>
      <form id="todo-form" class="space-y-4" @submit.prevent="submit">
        <UFormField label="Title" :error="error ?? undefined">
          <UInput v-model="form.title" autofocus placeholder="What needs doing?" class="w-full" />
        </UFormField>
        <UFormField label="Description" hint="Optional · Markdown">
          <UTextarea
            v-model="form.description"
            autoresize
            :rows="4"
            :maxrows="14"
            placeholder="Details, links, notes…"
            class="w-full"
          />
        </UFormField>
      </form>
    </template>
    <template #footer>
      <div class="flex w-full items-center gap-2">
        <UButton
          v-if="editing"
          color="error"
          :variant="confirmDelete ? 'solid' : 'ghost'"
          icon="i-lucide-trash-2"
          :label="confirmDelete ? 'Really delete?' : 'Delete'"
          :disabled="saving"
          @click="remove"
        />
        <div class="ml-auto flex gap-2">
          <UButton color="neutral" variant="ghost" label="Cancel" @click="open = false" />
          <UButton
            type="submit"
            form="todo-form"
            :label="editing ? 'Save' : 'Create'"
            :loading="saving"
          />
        </div>
      </div>
    </template>
  </UModal>
</template>
