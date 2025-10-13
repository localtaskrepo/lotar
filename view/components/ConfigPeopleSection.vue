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

      <div class="field">
        <label class="field-label">
          <span>Default tags</span>
          <span
            v-if="defaultTagsSource"
            :class="['provenance', provenanceClass(defaultTagsSource)]"
          >
            {{ provenanceLabel(defaultTagsSource) }}
          </span>
        </label>
        <TokenInput
          v-model="defaultTags"
          :suggestions="tagSuggestions"
          placeholder="Add a tag"
          @update:modelValue="handleTagsUpdate"
        />
        <p v-if="defaultTagsError" class="field-error">{{ defaultTagsError }}</p>
      </div>

      <div class="field">
        <label class="field-label">
          <span>Default category</span>
          <span
            v-if="defaultCategorySource"
            :class="['provenance', provenanceClass(defaultCategorySource)]"
          >
            {{ provenanceLabel(defaultCategorySource) }}
          </span>
        </label>
        <UiInput
          v-model="defaultCategory"
          :list="categoryOptionsId"
          maxlength="100"
          placeholder="Optional"
          @blur="handleBlur('default_category')"
        />
        <datalist :id="categoryOptionsId">
          <option v-for="option in categorySuggestions" :key="option" :value="option" />
        </datalist>
        <p v-if="showCategoryHint" class="field-hint">
          Inherit global category list unless you override it below.
        </p>
        <p v-if="defaultCategoryError" class="field-error">{{ defaultCategoryError }}</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import TokenInput from './TokenInput.vue'
import UiInput from './UiInput.vue'

const defaultReporter = defineModel<string>('defaultReporter', { required: true })
const defaultAssignee = defineModel<string>('defaultAssignee', { required: true })
const defaultTags = defineModel<string[]>('defaultTags', { required: true })
const defaultCategory = defineModel<string>('defaultCategory', { required: true })

const {
  description,
  isGlobal,
  tagSuggestions = [],
  categorySuggestions = [],
  defaultReporterError = null,
  defaultAssigneeError = null,
  defaultTagsError = null,
  defaultCategoryError = null,
  provenanceLabel,
  provenanceClass,
  defaultReporterSource,
  defaultAssigneeSource,
  defaultTagsSource,
  defaultCategorySource,
  categoryOptionsId = 'default-category-options',
} = defineProps<{
  description: string
  isGlobal: boolean
  tagSuggestions?: string[]
  categorySuggestions?: string[]
  defaultReporterError?: string | null
  defaultAssigneeError?: string | null
  defaultTagsError?: string | null
  defaultCategoryError?: string | null
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  defaultReporterSource?: ConfigSource
  defaultAssigneeSource?: ConfigSource
  defaultTagsSource?: ConfigSource
  defaultCategorySource?: ConfigSource
  categoryOptionsId?: string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'default_reporter' | 'default_assignee' | 'default_tags' | 'default_category'): void
}>()

const showCategoryHint = computed(() => categorySuggestions.length > 0 && !isGlobal)

function handleBlur(field: 'default_reporter' | 'default_assignee' | 'default_category') {
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

.field :deep(.input) {
  height: 32px;
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

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
</style>
