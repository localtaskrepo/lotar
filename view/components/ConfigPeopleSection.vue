<template>
  <ConfigGroup title="People &amp; defaults" :description="description">
    <div class="field-grid">
      <div class="field">
        <label class="field-label">
          <span>Default reporter</span>
          <span
            v-if="defaultReporterSource"
            :class="['provenance', provenanceClass(defaultReporterSource)]"
          >
            {{ provenanceLabel(defaultReporterSource) }}
          </span>
        </label>
        <UiInput
          v-model="defaultReporter"
          maxlength="100"
          placeholder="@me"
          @blur="handleBlur('default_reporter')"
        />
        <p v-if="defaultReporterError" class="field-error">{{ defaultReporterError }}</p>
      </div>

      <div class="field">
        <label class="field-label">
          <span>Default assignee</span>
          <span
            v-if="defaultAssigneeSource"
            :class="['provenance', provenanceClass(defaultAssigneeSource)]"
          >
            {{ provenanceLabel(defaultAssigneeSource) }}
          </span>
        </label>
        <UiInput
          v-model="defaultAssignee"
          maxlength="100"
          placeholder="Optional"
          @blur="handleBlur('default_assignee')"
        />
        <p v-if="defaultAssigneeError" class="field-error">{{ defaultAssigneeError }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label">
          <span>Default tags</span>
          <span
            v-if="defaultTagsSource"
            :class="['provenance', provenanceClass(defaultTagsSource)]"
          >
            {{ provenanceLabel(defaultTagsSource) }}
          </span>
        </label>
        <ChipListField
          v-model="defaultTags"
          :suggestions="tagSuggestions"
          placeholder="Add a tag"
          add-label="Add tag"
          composer-label="Tag"
          empty-label="No default tags"
          @update:modelValue="handleTagsUpdate"
        />
        <p v-if="defaultTagsError" class="field-error">{{ defaultTagsError }}</p>
      </div>

    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import type { ConfigSource } from '../api/types'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import UiInput from './UiInput.vue'

const defaultReporter = defineModel<string>('defaultReporter', { required: true })
const defaultAssignee = defineModel<string>('defaultAssignee', { required: true })
const defaultTags = defineModel<string[]>('defaultTags', { required: true })
const {
  description,
  isGlobal,
  tagSuggestions = [],
  defaultReporterError = null,
  defaultAssigneeError = null,
  defaultTagsError = null,
  provenanceLabel,
  provenanceClass,
  defaultReporterSource,
  defaultAssigneeSource,
  defaultTagsSource,
} = defineProps<{
  description: string
  isGlobal: boolean
  tagSuggestions?: string[]
  defaultReporterError?: string | null
  defaultAssigneeError?: string | null
  defaultTagsError?: string | null
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  defaultReporterSource?: ConfigSource
  defaultAssigneeSource?: ConfigSource
  defaultTagsSource?: ConfigSource
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'default_reporter' | 'default_assignee' | 'default_tags'): void
}>()

function handleBlur(field: 'default_reporter' | 'default_assignee') {
  emit('validate', field)
}

function handleTagsUpdate() {
  emit('validate', 'default_tags')
}
</script>

<style scoped>
.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}

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

.field--full {
  grid-column: 1 / -1;
}

.field :deep(.input) {
  height: var(--config-control-height, 40px);
  padding: calc(var(--space-2) - 4px) var(--space-3);
  box-sizing: border-box;
}

.field-error {
  color: #ff8091;
  font-size: 12px;
}

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}
</style>
