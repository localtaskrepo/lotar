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

    <div class="field">
      <label class="field-label">
        <span>Done states</span>
        <span
          v-if="issueDoneStatesSource"
          :class="['provenance', provenanceClass(issueDoneStatesSource)]"
        >
          {{ provenanceLabel(issueDoneStatesSource) }}
        </span>
      </label>
      <select
        class="done-states-mode"
        :value="doneStatesMode"
        aria-label="Done states mode"
        @change="onDoneStatesModeChange(($event.target as HTMLSelectElement).value)"
      >
        <option value="automatic">{{ automaticLabel }}</option>
        <option value="explicit">Explicit statuses</option>
      </select>
      <ChipListField
        v-if="doneStatesMode === 'explicit'"
        v-model="doneStatesChips"
        :suggestions="statusSuggestions"
        placeholder="Add done state"
        add-label="Add done state"
        composer-label="Done state"
        empty-label="No done states defined"
        @update:modelValue="handleUpdate('issue_done_states')"
      />
      <p v-if="effectiveSummary" class="field-hint">{{ effectiveSummary }}</p>
      <p v-if="issueDoneStatesError" class="field-error">{{ issueDoneStatesError }}</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'

const issueStates = defineModel<string[]>('issueStates', { required: true })
const issueTypes = defineModel<string[]>('issueTypes', { required: true })
const issuePriorities = defineModel<string[]>('issuePriorities', { required: true })
// null = automatic/inherit; an explicit list is one-or-more (empty flips back
// to automatic/inherit instead of saving an invalid explicit-empty value).
const issueDoneStates = defineModel<string[] | null>('issueDoneStates', { required: true })

const {
  description,
  statusSuggestions = [],
  typeSuggestions = [],
  prioritySuggestions = [],
  issueStatesError = null,
  issueTypesError = null,
  issuePrioritiesError = null,
  issueDoneStatesError = null,
  issueStatesSource,
  issueTypesSource,
  issuePrioritiesSource,
  issueDoneStatesSource,
  automaticLabel = 'Automatic',
  effectiveSummary = '',
  effectiveDoneLabels = [],
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
  issueDoneStatesError?: string | null
  issueStatesSource?: ConfigSource
  issueTypesSource?: ConfigSource
  issuePrioritiesSource?: ConfigSource
  issueDoneStatesSource?: ConfigSource
  /** Label for the automatic/inherit option (shows the current effective list). */
  automaticLabel?: string
  /** Effective done states summary shown under the control. */
  effectiveSummary?: string
  /** Effective labels used to seed the list when switching to explicit. */
  effectiveDoneLabels?: string[]
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'issue_states' | 'issue_types' | 'issue_priorities' | 'issue_done_states'): void
}>()

const doneStatesMode = computed<'automatic' | 'explicit'>(() => (issueDoneStates.value === null ? 'automatic' : 'explicit'))

const doneStatesChips = computed<string[]>({
  get: () => issueDoneStates.value ?? [],
  set: (value) => {
    // Clearing every chip resets to automatic/inherit, never explicit-empty.
    issueDoneStates.value = value.length ? value : null
  },
})

function onDoneStatesModeChange(mode: string) {
  if (mode === 'explicit') {
    issueDoneStates.value = effectiveDoneLabels.length ? [...effectiveDoneLabels] : []
    if (!effectiveDoneLabels.length) {
      emit('validate', 'issue_done_states')
    }
    return
  }
  issueDoneStates.value = null
}

function handleUpdate(field: 'issue_states' | 'issue_types' | 'issue_priorities' | 'issue_done_states') {
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

.field-hint {
  color: var(--color-muted);
  font-size: 12px;
}

.done-states-mode {
  width: fit-content;
  min-height: 34px;
}

</style>
