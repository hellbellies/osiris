<script setup lang="ts">
import { computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useObsidianStore } from "../stores/obsidianStore";
import type { ObsidianState } from "@/types";

/** Settings card: each prerequisite for Obsidian notes, and the vaults. */
const store = useObsidianStore();
const { status, checking } = storeToRefs(store);

/** The prerequisites in the order they are checked; `failsAs` is the state when missing. */
const steps: { label: string; failsAs: ObsidianState; hint: string }[] = [
  {
    label: "Obsidian installed",
    failsAs: "notInstalled",
    hint: "Install Obsidian in the Applications folder.",
  },
  {
    label: "Version 1.12.7 or later",
    failsAs: "outdated",
    hint: "Update Obsidian: the command line interface needs 1.12.7 or later (a new installer, not just an in-app update).",
  },
  {
    label: "Obsidian running",
    failsAs: "notRunning",
    hint: "Start Obsidian. osiris doesn't start it itself, so it never pops up unasked.",
  },
  {
    label: "Command line interface turned on",
    failsAs: "cliDisabled",
    hint: "In Obsidian: Settings → General → Advanced → Command line interface.",
  },
];

/** Per step: met, missing (the first failing step), or unknown (not checked yet). */
const rows = computed(() => {
  const state = status.value?.state;
  const failing = steps.findIndex((step) => step.failsAs === state);
  return steps.map((step, index) => {
    // "failed" means the CLI answered, so every step before it worked.
    const met = state === "ready" || state === "failed" || (failing !== -1 && index < failing);
    const result = met ? "met" : index === failing ? "missing" : "unknown";
    return { ...step, result };
  });
});

const summary = computed(() => {
  switch (status.value?.state) {
    case "ready":
      return "Ready: notes from open vaults can be added to workstreams.";
    case "failed":
      return status.value.message ?? "Obsidian answered unexpectedly.";
    case undefined:
      return "Not checked yet.";
    default:
      return "Notes can't be searched until every step is done.";
  }
});

onMounted(() => {
  if (!status.value) void store.checkStatus();
});
</script>

<template>
  <UCard>
    <template #header>
      <div class="flex items-center justify-between gap-4">
        <div>
          <h2 class="font-semibold text-highlighted">Obsidian</h2>
          <p class="text-sm text-muted">
            Add Obsidian notes to workstreams.
            <template v-if="status?.version">Installed: {{ status.version }}</template>
          </p>
        </div>
        <UButton
          color="neutral"
          variant="subtle"
          icon="i-lucide-refresh-cw"
          label="Check again"
          :loading="checking"
          @click="store.checkStatus()"
        />
      </div>
    </template>

    <div class="flex flex-col gap-4">
      <ul class="flex flex-col gap-2">
        <li v-for="row in rows" :key="row.failsAs" class="flex gap-2 text-sm">
          <UIcon
            v-if="row.result === 'met'"
            name="i-lucide-circle-check"
            class="mt-0.5 size-4 shrink-0 text-success"
          />
          <UIcon
            v-else-if="row.result === 'missing'"
            name="i-lucide-circle-x"
            class="mt-0.5 size-4 shrink-0 text-error"
          />
          <UIcon v-else name="i-lucide-circle-dashed" class="mt-0.5 size-4 shrink-0 text-dimmed" />
          <div>
            <p :class="row.result === 'unknown' ? 'text-muted' : 'text-highlighted'">{{ row.label }}</p>
            <p v-if="row.result === 'missing'" class="text-muted">{{ row.hint }}</p>
          </div>
        </li>
      </ul>
      <p class="text-sm" :class="status?.state === 'failed' ? 'text-error' : 'text-muted'">
        {{ summary }}
      </p>

      <div v-if="status?.vaults.length">
        <h3 class="mb-1 text-sm font-medium text-highlighted">Vaults</h3>
        <ul class="flex flex-col gap-1">
          <li v-for="vault in status.vaults" :key="vault.path" class="flex items-center gap-2 text-sm">
            <UIcon name="i-lucide-vault" class="size-4 shrink-0 text-muted" />
            <span class="text-highlighted">{{ vault.name }}</span>
            <span class="min-w-0 flex-1 truncate text-xs text-muted">{{ vault.path }}</span>
            <UBadge
              :color="vault.open ? 'success' : 'neutral'"
              variant="subtle"
              size="sm"
              :label="vault.open ? 'Open' : 'Closed'"
            />
          </li>
        </ul>
        <p class="mt-2 text-xs text-muted">
          Only open vaults are searched: Obsidian opens any other vault osiris would search.
        </p>
      </div>
    </div>
  </UCard>
</template>
