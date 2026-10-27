<script setup lang="ts">
import { computed } from 'vue'
import type { RelationshipNode } from '../api/types'
import BoardCardMeta from './BoardCardMeta.vue'
import { formatRelativeTime, parseTaskDate, startOfLocalDay } from '../utils/date'
const props = defineProps<{ node: RelationshipNode; fields?: Partial<Record<string, boolean>> }>()
const task = computed(() => props.node.task)
const title = computed(() => task.value?.title ?? props.node.title)
const visible = (key: string) => props.fields?.[key] !== false
const hasHeader = computed(
  () => visible('id') || (visible('title') && !!title.value) || (visible('priority') && !!task.value?.priority),
)
const dueInfo = computed(() => {
  const raw = task.value?.due_date
  const date = raw ? parseTaskDate(raw) : null
  return {
    label: raw ? `Due ${date ? date.toLocaleDateString() : raw}` : '',
    overdue: !!date && !props.node.is_done && startOfLocalDay(date) < startOfLocalDay(new Date()),
  }
})
const sprint = { label: (id: number) => `#${id}`, stateClass: () => '', tooltip: (id: number) => `Sprint #${id}` }
</script>
<template>
  <div class="dependency-card-details">
    <header v-if="hasHeader" class="task-header">
      <div class="task-header__left">
        <span v-if="visible('id')" class="muted id">{{ node.id }}</span
        ><strong v-if="visible('title') && title" class="title">{{ title }}</strong>
      </div>
      <span
        v-if="visible('priority') && task?.priority"
        class="priority"
        :class="`priority--${task.priority.toLowerCase()}`"
        >{{ task.priority }}</span
      >
    </header>
    <BoardCardMeta
      v-if="task"
      :task="task"
      :fields="fields ?? {}"
      :has-header="hasHeader"
      :due-info="dueInfo"
      :modified-info="task.modified ? `Updated ${formatRelativeTime(task.modified)}` : ''"
      :sprint="sprint"
    />
    <span v-else-if="visible('status') && node.status" class="muted">{{ node.status }}</span>
    <span v-if="!hasHeader && !Object.entries(fields ?? {}).some(([, on]) => on)" class="muted id">{{ node.id }}</span>
  </div>
</template>
<style scoped>
.dependency-card-details {
  min-width: 0;
  width: 100%;
  overflow-wrap: anywhere;
  color: var(--color-fg);
}
.task-header,
.task-header__left {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: baseline;
  min-width: 0;
}
.task-header {
  justify-content: space-between;
  align-items: flex-start;
}
.task-header__left {
  flex: 1;
}
.id {
  font-size: 0.75rem;
}
.title {
  color: var(--color-accent);
  line-height: 1.35;
}
.priority {
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 1px 6px;
  font-size: 0.7rem;
}
.priority--critical,
.priority--high {
  color: var(--color-danger);
}
.priority--low {
  color: var(--color-muted);
}
.dependency-card-details :deep(.task-meta) {
  display: grid;
  gap: 6px;
  margin-top: 6px;
  font-size: 0.75rem;
}
.dependency-card-details :deep(.row) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}
.dependency-card-details :deep(.member-inline) {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.dependency-card-details :deep(.member-badge) {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  /* Member hues are fixed mid-tones (utils/member.ts), so white text stays legible in every theme. */
  color: white;
  font-size: 10px;
  flex: 0 0 auto;
}
.dependency-card-details :deep(.tag),
.dependency-card-details :deep(.sprint-chip) {
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 1px 6px;
}
.dependency-card-details :deep(.task-meta--no-header) {
  margin-top: 0;
}
.dependency-card-details :deep(.is-overdue) {
  color: var(--color-danger);
}
</style>
