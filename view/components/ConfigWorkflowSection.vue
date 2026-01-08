<template>
  <ConfigGroup title="Workflow" :description="description">
    <div class="field">
      <label class="field-label">
        <span>Statuses (column order)</span>
        <span
          v-if="issueStatesSource"
          :class="['provenance', provenanceClass(issueStatesSource)]"
        >
          {{ provenanceLabel(issueStatesSource) }}
        </span>
      </label>
      <ChipListField
        v-model="issueStates"
        :suggestions="statusSuggestions"
        placeholder="Add status"
        add-label="Add status"
        composer-label="Status"
        empty-label="No statuses defined"
        @update:modelValue="handleUpdate('issue_states')"
      />
      <p v-if="issueStatesError" class="field-error">{{ issueStatesError }}</p>
    </div>

    <div class="field">
      <label class="field-label">
        <span>Types</span>
        <span
          v-if="issueTypesSource"
          :class="['provenance', provenanceClass(issueTypesSource)]"
        >
          {{ provenanceLabel(issueTypesSource) }}
        </span>
      </label>
      <ChipListField
        v-model="issueTypes"
        :suggestions="typeSuggestions"
        placeholder="Add type"
        add-label="Add type"
        composer-label="Type"
        empty-label="No types defined"
        @update:modelValue="handleUpdate('issue_types')"
      />
      <p v-if="issueTypesError" class="field-error">{{ issueTypesError }}</p>
    </div>

    <div class="field">
      <label class="field-label">
        <span>Priorities</span>
        <span
          v-if="issuePrioritiesSource"
          :class="['provenance', provenanceClass(issuePrioritiesSource)]"
        >
          {{ provenanceLabel(issuePrioritiesSource) }}
        </span>
      </label>
      <ChipListField
        v-model="issuePriorities"
        :suggestions="prioritySuggestions"
        placeholder="Add priority"
        add-label="Add priority"
        composer-label="Priority"
        empty-label="No priorities defined"
        @update:modelValue="handleUpdate('issue_priorities')"
      />
      <p v-if="issuePrioritiesError" class="field-error">{{ issuePrioritiesError }}</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import type { ConfigSource } from '../api/types'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'

const issueStates = defineModel<string[]>('issueStates', { required: true })
const issueTypes = defineModel<string[]>('issueTypes', { required: true })
const issuePriorities = defineModel<string[]>('issuePriorities', { required: true })

const {
  description,
  statusSuggestions = [],
  typeSuggestions = [],
  prioritySuggestions = [],
  issueStatesError = null,
  issueTypesError = null,
  issuePrioritiesError = null,
  issueStatesSource,
  issueTypesSource,
  issuePrioritiesSource,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  statusSuggestions?: string[]
  typeSuggestions?: string[]
  prioritySuggestions?: string[]
  issueStatesError?: string | null
  issueTypesError?: string | null
  issuePrioritiesError?: string | null
  issueStatesSource?: ConfigSource
  issueTypesSource?: ConfigSource
  issuePrioritiesSource?: ConfigSource
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'issue_states' | 'issue_types' | 'issue_priorities'): void
}>()

function handleUpdate(field: 'issue_states' | 'issue_types' | 'issue_priorities') {
  emit('validate', field)
}
</script>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.field-error {
  color: var(--color-danger);
  font-size: 12px;
}

</style>
