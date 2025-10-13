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
      <TokenInput
        v-model="issueStates"
        :suggestions="statusSuggestions"
        placeholder="Add status"
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
      <TokenInput
        v-model="issueTypes"
        :suggestions="typeSuggestions"
        placeholder="Add type"
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
      <TokenInput
        v-model="issuePriorities"
        :suggestions="prioritySuggestions"
        placeholder="Add priority"
        @update:modelValue="handleUpdate('issue_priorities')"
      />
      <p v-if="issuePrioritiesError" class="field-error">{{ issuePrioritiesError }}</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import type { ConfigSource } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import TokenInput from './TokenInput.vue'

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

.field :deep(.token-input) {
  width: 100%;
}

.field :deep(.token-input .tokens) {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  min-height: 32px;
  padding: calc(var(--space-2) - 4px) var(--space-3);
  background: color-mix(in oklab, var(--color-surface) 96%, transparent);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.field :deep(.token-input:focus-within .tokens) {
  border-color: var(--color-accent);
  box-shadow: var(--focus-ring);
}

.field :deep(.token-input .tokens input) {
  color: var(--color-fg);
}

.field :deep(.token-input .tokens input::placeholder) {
  color: var(--color-muted);
  opacity: 1;
}

.field-error {
  color: #ff8091;
  font-size: 12px;
}

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
</style>
