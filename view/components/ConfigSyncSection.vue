<template>
  <ConfigGroup :title="title" :description="description">
    <div class="sync-section">
      <p v-if="!entries.length" class="muted">No sync remotes configured for this scope.</p>
      <div v-for="entry in entries" :key="entry.name" class="sync-remote">
        <div class="sync-remote__details">
          <strong>{{ entry.name }}</strong>
          <span class="sync-remote__provider muted">
            <IconGlyph :name="remoteProviderIcon(entry.remote)" />
            <span>{{ remoteProviderLabel(entry.remote) }}</span>
          </span>
          <span v-if="checkError(entry.name)" class="sync-remote__status sync-remote__status--error">
            {{ checkError(entry.name) }}
          </span>
          <span v-else-if="checkSummary(entry.name)" class="sync-remote__status muted">
            {{ checkSummary(entry.name) }}
          </span>
        </div>
        <div class="sync-remote__actions">
          <UiButton
            variant="ghost"
            type="button"
            :disabled="busy"
            @click="$emit('pull', entry)"
          >
            Pull
          </UiButton>
          <UiButton
            variant="ghost"
            type="button"
            :disabled="busy"
            @click="$emit('push', entry)"
          >
            Push
          </UiButton>
          <UiButton
            variant="ghost"
            type="button"
            :disabled="busy"
            @click="$emit('check', entry)"
          >
            Check
          </UiButton>
        </div>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { SyncRemoteConfig, SyncResponse } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'

type SyncEntry = { name: string; remote: SyncRemoteConfig }
type SyncCheckState = { result: SyncResponse; checkedAt: string }

const props = withDefaults(defineProps<{
  entries: SyncEntry[]
  busy?: boolean
  checkStates?: Record<string, SyncCheckState>
  checkErrors?: Record<string, string | null>
  title?: string
  description?: string
}>(), {
  entries: () => [],
  busy: false,
  title: 'Remotes',
  description: 'Manual push/pull operations. Configure remotes in config YAML.',
})

defineEmits<{
  (e: 'pull', entry: SyncEntry): void
  (e: 'push', entry: SyncEntry): void
  (e: 'check', entry: SyncEntry): void
}>()

const entries = computed(() => props.entries ?? [])

function remoteProviderIcon(remote: SyncRemoteConfig): 'jira' | 'github' | 'list' {
  if (remote.provider === 'jira') return 'jira'
  if (remote.provider === 'github') return 'github'
  return 'list'
}

function remoteProviderLabel(remote: SyncRemoteConfig): string {
  if (remote.provider === 'jira') {
    return remote.project?.trim() || 'Jira'
  }
  if (remote.provider === 'github') {
    return remote.repo?.trim() || 'GitHub'
  }
  return remote.provider
}

function checkSummary(name: string): string | null {
  const state = props.checkStates?.[name]
  if (!state) return null
  const { result, checkedAt } = state
  const summary = result.summary
  const timestamp = formatTimestamp(checkedAt)
  const warningCount = result.warnings?.length ?? 0
  const infoCount = result.info?.length ?? 0
  const extras = warningCount || infoCount
    ? ` · ${warningCount} warning${warningCount === 1 ? '' : 's'}${infoCount ? `, ${infoCount} note${infoCount === 1 ? '' : 's'}` : ''}`
    : ''
  return `Last check ${timestamp}: ${summary.created} created, ${summary.updated} updated, ${summary.skipped} skipped, ${summary.failed} failed${extras}`
}

function checkError(name: string): string | null {
  const message = props.checkErrors?.[name]
  if (!message) return null
  return `Last check failed: ${message}`
}

function formatTimestamp(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}
</script>

<style scoped>
.sync-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.sync-remote {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface);
  flex-wrap: wrap;
}

.sync-remote__details {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 220px;
}

.sync-remote__provider {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.sync-remote__actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.sync-remote__status {
  font-size: 0.85rem;
}

.sync-remote__status--error {
  color: var(--color-danger);
}
</style>
