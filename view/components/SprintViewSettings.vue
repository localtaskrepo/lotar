<script setup lang="ts">
import UiSelect from './UiSelect.vue'

export type SprintTimeRangeKey = 'current' | '30' | '90' | '180' | 'all'

defineProps<{
  timeRange: SprintTimeRangeKey
  choices: Array<{ value: SprintTimeRangeKey; label: string }>
  showAllowClosed: boolean
  allowClosed: boolean
  highlightMultiSprint: boolean
}>()

const emit = defineEmits<{
  'update:timeRange': [value: SprintTimeRangeKey]
  'update:allowClosed': [value: boolean]
  'update:highlightMultiSprint': [value: boolean]
}>()
</script>

<template>
  <div class="row sprints-view-settings">
    <label class="filter-field">
      <span class="muted">Sprint window</span>
      <UiSelect
        :model-value="timeRange"
        @update:model-value="emit('update:timeRange', $event as SprintTimeRangeKey)"
      >
        <option v-for="option in choices" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </UiSelect>
    </label>
    <label v-if="showAllowClosed" class="filter-checkbox">
      <input
        type="checkbox"
        :checked="allowClosed"
        @change="emit('update:allowClosed', ($event.target as HTMLInputElement).checked)"
      />
      Allow editing closed sprints
    </label>
    <label class="filter-checkbox">
      <input
        type="checkbox"
        :checked="highlightMultiSprint"
        @change="emit('update:highlightMultiSprint', ($event.target as HTMLInputElement).checked)"
      />
      Highlight tasks in multiple sprints
    </label>
  </div>
</template>
