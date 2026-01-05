<template>
  <div class="task-panel__tab-panel">
    <ReloadButton
      class="task-panel__tab-action"
      variant="ghost"
      :disabled="mode !== 'edit'"
      label="Reload history"
      title="Reload history"
      @click="$emit('reload')"
    />
    <template v-if="mode === 'edit'">
      <div class="task-panel__history-scroll" role="region" aria-label="Task history">
        <div class="task-panel__history">
          <h4>Recent updates</h4>
          <ul class="task-panel__history-list">
            <li
              v-for="entry in changeLog"
              :key="`${entry.at}-${entry.actor || 'unknown'}`"
              class="task-panel__history-item"
            >
              <div class="task-panel__history-meta">
                <time>{{ formatDate(entry.at) }}</time>
                <span v-if="entry.actor" class="task-panel__history-actor">{{ entry.actor }}</span>
              </div>
              <ul class="task-panel__history-changes">
                <li v-for="(change, idx) in entry.changes" :key="idx" class="task-panel__history-change">
                  <strong>{{ formatFieldName(change.field) }}</strong>
                  <span class="task-panel__history-change-values">
                    <span v-if="formatChangeValue(change.old)" class="task-panel__history-old">{{ formatChangeValue(change.old) }}</span>
                    <span v-if="formatChangeValue(change.old) && formatChangeValue(change.new)" aria-hidden="true">→</span>
                    <span v-if="formatChangeValue(change.new)" class="task-panel__history-new">{{ formatChangeValue(change.new) }}</span>
                    <span v-else-if="!formatChangeValue(change.old)" class="task-panel__history-new">Set</span>
                  </span>
                </li>
              </ul>
            </li>
            <li v-if="!changeLog.length" class="muted">No updates yet</li>
          </ul>
        </div>
      </div>
    </template>
    <p v-else class="task-panel__empty-hint">History will appear after the task is created.</p>
  </div>
</template>

<script setup lang="ts">
import ReloadButton from '../ReloadButton.vue';

defineProps<{
  mode: 'create' | 'edit'
  changeLog: Array<{
    at: string
    actor?: string | null
    changes: Array<{ field: string; old?: unknown; new?: unknown }>
  }>
  formatDate: (input: string) => string
  formatFieldName: (field: string) => string
  formatChangeValue: (value: any) => string
}>()

defineEmits<{ (e: 'reload'): void }>()
</script>


