<template>
  <ConfigGroup title="Taxonomy" :description="description">
    <div class="field">
      <label class="field-label">
        <span>Categories</span>
        <span
          v-if="categoriesSource"
          :class="['provenance', provenanceClass(categoriesSource)]"
        >
          {{ provenanceLabel(categoriesSource) }}
        </span>
      </label>
      <TokenInput
        v-model="categories"
        placeholder="backend"
        @update:modelValue="handleUpdate('categories')"
      />
      <p v-if="categoryWildcard" class="field-hint">
        Wildcard enabled — leave empty to accept any category.
      </p>
      <p v-if="categoriesError" class="field-error">{{ categoriesError }}</p>
    </div>

    <div class="field">
      <label class="field-label">
        <span>Tags</span>
        <span
          v-if="tagsSource"
          :class="['provenance', provenanceClass(tagsSource)]"
        >
          {{ provenanceLabel(tagsSource) }}
        </span>
      </label>
      <TokenInput
        v-model="tags"
        placeholder="frontend"
        @update:modelValue="handleUpdate('tags')"
      />
      <p v-if="tagWildcard" class="field-hint">
        Wildcard enabled — leave empty to keep "any tag" behaviour.
      </p>
      <p v-if="tagsError" class="field-error">{{ tagsError }}</p>
    </div>

    <div class="field">
      <label class="field-label">
        <span>Custom fields</span>
        <span
          v-if="customFieldsSource"
          :class="['provenance', provenanceClass(customFieldsSource)]"
        >
          {{ provenanceLabel(customFieldsSource) }}
        </span>
      </label>
      <TokenInput
        v-model="customFields"
        placeholder="Name a custom field"
        @update:modelValue="handleUpdate('custom_fields')"
      />
      <p class="field-hint">
        List custom field keys (e.g., "Story Points"). Values are captured later when editing individual tasks.
      </p>
      <p v-if="customFieldWildcard" class="field-hint">
        Wildcard enabled — leave empty to accept any custom field name.
      </p>
      <p v-if="customFieldsError" class="field-error">{{ customFieldsError }}</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import type { ConfigSource } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import TokenInput from './TokenInput.vue'

const categories = defineModel<string[]>('categories', { required: true })
const tags = defineModel<string[]>('tags', { required: true })
const customFields = defineModel<string[]>('customFields', { required: true })

const {
  description,
  categoryWildcard = false,
  tagWildcard = false,
  customFieldWildcard = false,
  categoriesError = null,
  tagsError = null,
  customFieldsError = null,
  categoriesSource,
  tagsSource,
  customFieldsSource,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  categoryWildcard?: boolean
  tagWildcard?: boolean
  customFieldWildcard?: boolean
  categoriesError?: string | null
  tagsError?: string | null
  customFieldsError?: string | null
  categoriesSource?: ConfigSource
  tagsSource?: ConfigSource
  customFieldsSource?: ConfigSource
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'categories' | 'tags' | 'custom_fields'): void
}>()

function handleUpdate(field: 'categories' | 'tags' | 'custom_fields') {
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

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
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
