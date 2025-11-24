<template>
  <div class="task-panel__tab-panel">
    <header class="task-panel__group-header">
      <h3>Recent commits</h3>
      <div class="row" style="gap: 8px; align-items: center;">
        <ReloadButton
          variant="ghost"
          :disabled="commitsLoading || mode !== 'edit'"
          :loading="commitsLoading"
          :label="commitsLoading ? 'Refreshing commits…' : 'Refresh commits'"
          :title="commitsLoading ? 'Refreshing commits…' : 'Refresh commits'"
          @click="$emit('refresh')"
        />
      </div>
    </header>
    <template v-if="mode === 'edit'">
      <div class="task-panel__history-scroll" role="region" aria-label="Recent commits">
        <UiLoader v-if="commitsLoading && !commitHistory.length" size="sm">Loading commits…</UiLoader>
        <ul v-else class="task-panel__commits-list">
          <li v-for="event in commitHistory" :key="event.commit" class="task-panel__history-item">
            <div class="task-panel__history-meta">
              <span class="task-panel__history-commit">{{ formatCommit(event.commit) }}</span>
              <time>{{ formatDate(event.date) }}</time>
            </div>
            <div class="task-panel__history-message">{{ event.message }}</div>
            <div class="task-panel__history-author">{{ event.author }}</div>
          </li>
        </ul>
        <p v-if="!commitsLoading && !commitHistory.length" class="muted">No commits yet</p>
      </div>
    </template>
    <p v-else class="task-panel__empty-hint">Commits appear after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import ReloadButton from '../ReloadButton.vue'
import UiLoader from '../UiLoader.vue'

interface CommitEntry {
  commit: string
  author: string
  email: string
  date: string
  message: string
}

defineProps<{
  mode: 'create' | 'edit'
  commitHistory: CommitEntry[]
  commitsLoading: boolean
  formatCommit: (commit: string) => string
  formatDate: (input: string) => string
}>()

defineEmits<{ (e: 'refresh'): void }>()
</script>


