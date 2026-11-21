<template>
  <fieldset class="task-panel__group">
    <legend>Ownership</legend>
    <div class="task-panel__row task-panel__row--ownership">
      <div class="task-panel__ownership-column">
        <label class="task-panel__ownership-label" for="task-panel-reporter-select">Reporter</label>
        <AssigneePicker
          id="task-panel-reporter-select"
          :model-value="form.reporter"
          aria-label="Reporter"
          empty-label="Unassigned"
          custom-verb="Set reporter to"
          :people="orderedKnownUsers"
          :profiles="profiles"
          :whoami="whoami"
          @update:model-value="(value) => emit('setOwner', 'reporter', value)"
        />
      </div>
      <div class="task-panel__ownership-column">
        <label class="task-panel__ownership-label" for="task-panel-assignee-select">Assignee</label>
        <AssigneePicker
          id="task-panel-assignee-select"
          :model-value="form.assignee"
          aria-label="Assignee"
          empty-label="Unassigned"
          :people="orderedKnownUsers"
          :profiles="profiles"
          :whoami="whoami"
          @update:model-value="(value) => emit('setOwner', 'assignee', value)"
        />
      </div>
    </div>
    <div class="task-panel__row">
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Due date</span>
        <UiInput
          v-model="form.due_date"
          type="date"
          placeholder="Due date"
          aria-label="Due date"
          @blur="emitFieldBlur('due_date')"
        />
      </label>
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Effort</span>
        <UiInput
          v-model="form.effort"
          placeholder="Effort (e.g., 3d, 5h)"
          aria-label="Effort"
          @blur="emitFieldBlur('effort')"
        />
      </label>
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import type { AgentProfileInfo } from '../../api/types'
import type { TaskPanelFormState } from '../../composables/task-panel/useTaskPanelPersistence'
import AssigneePicker from '../AssigneePicker.vue'
import UiInput from '../UiInput.vue'

defineProps<{
  form: TaskPanelFormState
  orderedKnownUsers: string[]
  profiles: AgentProfileInfo[]
  whoami: string | null
  /** Show visible labels for due date and effort (large view). */
  labelled?: boolean
}>()

const emit = defineEmits<{
  (e: 'setOwner', field: 'reporter' | 'assignee', value: string): void
  (e: 'fieldBlur', field: string): void
}>()

function emitFieldBlur(field: string) {
  emit('fieldBlur', field)
}
</script>
