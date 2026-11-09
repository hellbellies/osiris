import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { call } from "@/lib/ipc";
import type { AppError, Todo, TodoInput } from "@/types";

/**
 * All todos. Mutations update the local list in place and reject with an
 * `AppError` so the calling UI can show the message where the user acted.
 */
export const useTodosStore = defineStore("todos", () => {
  const todos = ref<Todo[]>([]);
  const loaded = ref(false);
  const error = ref<AppError | null>(null);

  /** Oldest first, like a queue. */
  const open = computed(() => todos.value.filter((todo) => todo.completedAt === null));
  /** Most recently completed first. */
  const completed = computed(() =>
    todos.value
      .filter((todo) => todo.completedAt !== null)
      .sort((a, b) => (b.completedAt ?? 0) - (a.completedAt ?? 0)),
  );

  async function load(): Promise<void> {
    error.value = null;
    try {
      todos.value = await call("list_todos");
    } catch (err: unknown) {
      error.value = err as AppError;
    } finally {
      loaded.value = true;
    }
  }

  function replace(todo: Todo): Todo {
    const index = todos.value.findIndex((candidate) => candidate.id === todo.id);
    if (index === -1) todos.value.push(todo);
    else todos.value[index] = todo;
    return todo;
  }

  /** Creates a todo, optionally directly inside a workstream. */
  async function create(input: TodoInput, workstreamId: number | null = null): Promise<Todo> {
    return replace(await call("create_todo", { input, workstreamId }));
  }

  async function update(id: number, input: TodoInput): Promise<Todo> {
    return replace(await call("update_todo", { id, input }));
  }

  async function setCompleted(id: number, isCompleted: boolean): Promise<Todo> {
    return replace(await call("set_todo_completed", { id, completed: isCompleted }));
  }

  async function remove(id: number): Promise<void> {
    await call("delete_todo", { id });
    todos.value = todos.value.filter((todo) => todo.id !== id);
  }

  /** Opens a link from a description in the default browser or mail app. */
  async function openLink(url: string): Promise<void> {
    await call("open_todo_link", { url });
  }

  /** Reflects a saved workstream change locally, without reloading. */
  function applyWorkstream(id: number, workstreamId: number | null): void {
    const todo = todos.value.find((candidate) => candidate.id === id);
    if (todo) todo.workstreamId = workstreamId;
  }

  return {
    todos,
    open,
    completed,
    loaded,
    error,
    load,
    create,
    update,
    setCompleted,
    remove,
    openLink,
    applyWorkstream,
  };
});
