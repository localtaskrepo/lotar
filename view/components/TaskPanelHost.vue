<template>
  <TaskPanel
    :open="panelState.open"
    :task-id="panelState.taskId ?? undefined"
    :initial-project="panelState.initialProject ?? undefined"
    :initial-status="panelState.initialStatus ?? undefined"
    :initial-due-date="panelState.initialDueDate ?? undefined"
    :focus-section="panelState.focusSection ?? undefined"
    :lifecycle-reload="lifecycleTick"
    @close="handleClose"
    @created="handleCreated"
    @updated="handleUpdated"
    @restored="handleRestored"
  />
</template>

<script setup lang="ts">
import type { TaskDTO } from '../api/types'
import { computed, ref, watch } from 'vue'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTaskStore } from '../composables/useTaskStore'
import { isDeletedTask } from '../utils/text'
import { showToast } from './toast'
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

function handleRestored(task: TaskDTO) {
  // DEV-92: restores clear the tombstone authoritatively; plain mutation
  // snapshots must not be able to.
  store.upsert(task, { restore: true })
  notifyUpdated(task)
}

// -- DEV-92: external lifecycle reactivity for the OPEN panel ----------------
//
// The shared store already owns the SSE connection and lifecycle guards; this
// watcher rides its reactivity (no duplicate event stream). Only transitions
// of the CURRENTLY open task's lifecycle are observed — never unrelated
// entity writes — and the initial observation of a panel session never acts
// (opening a trash row deliberately must not auto-close/reload the panel).

type PanelLifecycle = 'inactive' | 'unknown' | 'active' | 'deleted' | 'gone'

const lifecycleState = computed<PanelLifecycle>(() => {
  const id = panelState.taskId
  if (!panelState.open || !id || id === 'new') return 'inactive'
  void store.version.value
  const entity = store._map.value.get(id)
  if (entity) return isDeletedTask(entity) ? 'deleted' : 'active'
  return store.isHardDeleted(id) ? 'gone' : 'unknown'
})

/** Bumped when the open panel must reload its task (external delete/restore). */
const lifecycleTick = ref(0)

watch(lifecycleState, (state, previous) => {
  if (previous === undefined || previous === state) return
  if (previous === 'inactive') return // first observation of this panel session
  if (state === 'inactive' || state === 'unknown') return
  if (state === 'gone') {
    // Hard disappearance while the panel is open: no details exist anymore.
    showToast(`Task ${panelState.taskId} was permanently deleted`)
    closeTaskPanel()
    return
  }
  if (state === 'deleted' || previous === 'deleted') {
    // External soft delete (authoritative stamp + read-only) or an external
    // restore of the open trash row — reconcile by reloading the task.
    lifecycleTick.value += 1
  }
})
</script>
