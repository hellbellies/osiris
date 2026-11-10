<script setup lang="ts">
import { SettingsPanel } from "@/features/settings";
import { JiraSettingsPanel } from "@/features/jira";
import { GitHubSettingsPanel } from "@/features/github";
import { ObsidianStatusPanel } from "@/features/obsidian";
</script>

<template>
  <UDashboardPanel id="settings">
    <template #header>
      <UDashboardNavbar title="Settings" icon="i-lucide-settings" />
    </template>
    <template #body>
      <div class="mx-auto flex w-full max-w-xl flex-col gap-6">
        <SettingsPanel />
        <JiraSettingsPanel />
        <GitHubSettingsPanel />
        <ObsidianStatusPanel />
      </div>
    </template>
  </UDashboardPanel>
</template>
