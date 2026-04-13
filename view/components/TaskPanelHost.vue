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
import { useTaskStore } from '../composables/useTaskStore'
import TaskPanel from './TaskPanel.vue'

const { state: panelState, closeTaskPanel, notifyCreated, notifyUpdated } = useTaskPanelController()
const store = useTaskStore()

function handleClose() {
  closeTaskPanel()
}

function handleCreated(task: TaskDTO) {
  store.upsert(task)
  notifyCreated(task)
}

function handleUpdated(task: TaskDTO) {
  store.upsert(task)
  notifyUpdated(task)
}
</script>
