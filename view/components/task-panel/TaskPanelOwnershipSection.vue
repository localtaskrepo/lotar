<template>
  <fieldset class="task-panel__group">
    <legend>Ownership</legend>
    <div class="task-panel__row task-panel__row--ownership">
      <div class="task-panel__ownership-column">
        <label class="task-panel__ownership-label" for="task-panel-reporter-select">Reporter</label>
        <UiSelect
          id="task-panel-reporter-select"
          :model-value="reporterSelection"
          @update:modelValue="(value) => emit('update:reporterSelection', value as string)"
        >
          <option value="">Unassigned</option>
          <option v-for="user in orderedKnownUsers" :key="`reporter-${user}`" :value="user">
            {{ user === whoami ? `${user} (you)` : user }}
          </option>
          <option value="__custom">Custom…</option>
        </UiSelect>
        <div v-if="reporterMode === 'custom'" class="task-panel__ownership-custom">
          <UiInput
            :model-value="reporterCustom"
            placeholder="Type reporter"
            @update:modelValue="(value) => emit('update:reporterCustom', value)"
            @blur="emit('commitReporterCustom')"
            @keyup.enter.prevent="emit('commitReporterCustom')"
          />
          <UiButton
            variant="ghost"
            type="button"
            class="task-panel__ownership-reset"
            @click="emit('resetReporterSelection')"
          >Use list</UiButton>
        </div>
      </div>
      <div class="task-panel__ownership-column">
        <label class="task-panel__ownership-label" for="task-panel-assignee-select">Assignee</label>
        <UiSelect
          id="task-panel-assignee-select"
          :model-value="assigneeSelection"
          @update:modelValue="(value) => emit('update:assigneeSelection', value as string)"
        >
          <option value="">Unassigned</option>
          <option v-for="user in orderedKnownUsers" :key="`assignee-${user}`" :value="user">
            {{ user === whoami ? `${user} (you)` : user }}
          </option>
          <option value="__custom">Custom…</option>
        </UiSelect>
        <div v-if="assigneeMode === 'custom'" class="task-panel__ownership-custom">
          <UiInput
            :model-value="assigneeCustom"
            placeholder="Type assignee"
            @update:modelValue="(value) => emit('update:assigneeCustom', value)"
            @blur="emit('commitAssigneeCustom')"
            @keyup.enter.prevent="emit('commitAssigneeCustom')"
          />
          <UiButton
            variant="ghost"
            type="button"
            class="task-panel__ownership-reset"
            @click="emit('resetAssigneeSelection')"
          >Use list</UiButton>
        </div>
      </div>
    </div>
    <div class="task-panel__row">
      <UiInput
        v-model="form.due_date"
        type="date"
        placeholder="Due date"
        @blur="emitFieldBlur('due_date')"
      />
      <UiInput
        v-model="form.effort"
        placeholder="Effort (e.g., 3d, 5h)"
        @blur="emitFieldBlur('effort')"
      />
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import UiButton from '../UiButton.vue';
import UiInput from '../UiInput.vue';
import UiSelect from '../UiSelect.vue';

const props = defineProps<{
  form: Record<string, any>
  orderedKnownUsers: string[]
  whoami: string | null
  reporterSelection: string
  assigneeSelection: string
  reporterMode: 'select' | 'custom'
  assigneeMode: 'select' | 'custom'
  reporterCustom: string
  assigneeCustom: string
}>()

const emit = defineEmits<{
  (e: 'update:reporterSelection', value: string): void
  (e: 'update:assigneeSelection', value: string): void
  (e: 'update:reporterCustom', value: string): void
  (e: 'update:assigneeCustom', value: string): void
  (e: 'commitReporterCustom'): void
  (e: 'commitAssigneeCustom'): void
  (e: 'resetReporterSelection'): void
  (e: 'resetAssigneeSelection'): void
  (e: 'fieldBlur', field: string): void
}>()

function emitFieldBlur(field: string) {
  emit('fieldBlur', field)
}
</script>


