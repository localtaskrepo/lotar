<template>
  <ConfigGroup title="Taxonomy" :description="description">
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
      <ChipListField
        v-model="tags"
        placeholder="frontend"
        add-label="Add tag"
        composer-label="Tag"
        empty-label="No tags configured"
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
      <ChipListField
        v-model="customFields"
        placeholder="Name a custom field"
        add-label="Add field"
        composer-label="Field"
        empty-label="No custom fields"
        @update:modelValue="handleUpdate('custom_fields')"
      />
      <p class="field-hint">
        List custom field keys (e.g., "Category", "Product Line", or "Environment"). Values are captured later when editing individual tasks.
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
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'

const tags = defineModel<string[]>('tags', { required: true })
const customFields = defineModel<string[]>('customFields', { required: true })

const {
  description,
  tagWildcard = false,
  customFieldWildcard = false,
  tagsError = null,
  customFieldsError = null,
  tagsSource,
  customFieldsSource,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  tagWildcard?: boolean
  customFieldWildcard?: boolean
  tagsError?: string | null
  customFieldsError?: string | null
  tagsSource?: ConfigSource
  customFieldsSource?: ConfigSource
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'tags' | 'custom_fields'): void
}>()

function handleUpdate(field: 'tags' | 'custom_fields') {
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

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}

.field-error {
  color: #ff8091;
  font-size: 12px;
}

</style>
