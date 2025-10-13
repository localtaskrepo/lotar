<template>
  <TaskPanel
    :open="panelState.open"
    :task-id="panelState.taskId ?? undefined"
    :initial-project="panelState.initialProject ?? undefined"
    @close="handleClose"
    @created="handleCreated"
    @updated="handleUpdated"
  />
</template>

<script setup lang="ts">
import type { TaskDTO } from '../api/types'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTasks } from '../composables/useTasks'
import TaskPanel from './TaskPanel.vue'

const { state: panelState, closeTaskPanel, notifyCreated, notifyUpdated } = useTaskPanelController()
const { items } = useTasks()

function upsertTask(taskId: string, task: TaskDTO) {
  const idx = items.value.findIndex((t) => t.id === taskId)
  if (idx >= 0) {
    items.value[idx] = task
  } else {
    items.value.unshift(task)
  }
}

function handleClose() {
  closeTaskPanel()
}

function handleCreated(task: TaskDTO) {
  upsertTask(task.id, task)
  notifyCreated(task)
}

function handleUpdated(task: TaskDTO) {
  upsertTask(task.id, task)
  notifyUpdated(task)
}
</script>
