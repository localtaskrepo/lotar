<template>
  <ConfigGroup title="Taxonomy" :description="description">
    <div class="field" data-testid="config-tags">
      <label class="field-label" :for="`${uid}-tags-mode`">
        <span>Tags</span>
        <span
          v-if="tagsSource"
          :class="['provenance', provenanceClass(tagsSource)]"
        >
          {{ provenanceLabel(tagsSource) }}
        </span>
      </label>
      <UiSelect :id="`${uid}-tags-mode`" v-model="tagsMode" @change="handleUpdate('tags')">
        <option v-for="option in modeOptions(globalTags, 'tags')" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </UiSelect>
      <ChipListField
        v-if="tagsMode === 'list'"
        v-model="tags"
        placeholder="frontend"
        add-label="Add tag"
        composer-label="Tag"
        empty-label="No tags yet"
        @update:modelValue="handleUpdate('tags')"
      />
      <p class="field-hint">{{ modeHint(tagsMode, 'tags') }}</p>
      <p v-if="tagsError" class="field-error">{{ tagsError }}</p>
    </div>

    <div class="field" data-testid="config-custom-fields">
      <label class="field-label" :for="`${uid}-custom-fields-mode`">
        <span>Custom fields</span>
        <span
          v-if="customFieldsSource"
          :class="['provenance', provenanceClass(customFieldsSource)]"
        >
          {{ provenanceLabel(customFieldsSource) }}
        </span>
      </label>
      <UiSelect :id="`${uid}-custom-fields-mode`" v-model="customFieldsMode" @change="handleUpdate('custom_fields')">
        <option
          v-for="option in modeOptions(globalCustomFields, 'custom fields')"
          :key="option.value"
          :value="option.value"
        >
          {{ option.label }}
        </option>
      </UiSelect>
      <ChipListField
        v-if="customFieldsMode === 'list'"
        v-model="customFields"
        placeholder="Name a custom field"
        add-label="Add field"
        composer-label="Field"
        empty-label="No custom fields yet"
        @update:modelValue="handleUpdate('custom_fields')"
      />
      <p class="field-hint">{{ modeHint(customFieldsMode, 'custom fields') }}</p>
      <p v-if="customFieldsError" class="field-error">{{ customFieldsError }}</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { useId } from 'vue'
import type { ConfigSource } from '../api/types'
import type { ListMode } from '../composables/useConfigForm'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import UiSelect from './UiSelect.vue'

const tagsMode = defineModel<ListMode>('tagsMode', { required: true })
const tags = defineModel<string[]>('tags', { required: true })
const customFieldsMode = defineModel<ListMode>('customFieldsMode', { required: true })
const customFields = defineModel<string[]>('customFields', { required: true })

const {
  description,
  isGlobal,
  globalTags = [],
  globalCustomFields = [],
  tagsError = null,
  customFieldsError = null,
  tagsSource,
  customFieldsSource,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  isGlobal: boolean
  /** Effective global values, summarized in the project inherit option. */
  globalTags?: string[]
  globalCustomFields?: string[]
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

const uid = useId()

function summarize(values: string[]): string {
  if (values.includes('*')) return 'any'
  if (!values.length) return 'none'
  return values.length > 4 ? `${values.slice(0, 4).join(', ')}, +${values.length - 4} more` : values.join(', ')
}

function modeOptions(globalValues: string[], noun: string): Array<{ value: ListMode; label: string }> {
  const options: Array<{ value: ListMode; label: string }> = [
    { value: 'any', label: `Any ${noun}` },
    { value: 'list', label: `Only these ${noun}` },
    { value: 'none', label: `No ${noun}` },
  ]
  if (!isGlobal) {
    options.unshift({ value: 'inherit', label: `Inherit global (currently ${summarize(globalValues)})` })
  }
  return options
}

function modeHint(mode: ListMode, noun: 'tags' | 'custom fields'): string {
  switch (mode) {
    case 'inherit':
      return `Uses the global ${noun} setting.`
    case 'any':
      return noun === 'tags'
        ? 'Tasks may use any tag.'
        : 'Tasks may add any custom field (stored as the "*" wildcard).'
    case 'none':
      return noun === 'tags'
        ? 'Tags are turned off: tasks cannot be tagged.'
        : 'Custom fields are turned off: no fields are offered when editing tasks.'
    default:
      return noun === 'tags'
        ? 'Only the listed tags are accepted.'
        : 'Only the listed fields (e.g., "Category" or "Environment") are offered; values are set per task.'
  }
}

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
  color: var(--color-muted);
  font-size: 12px;
}

.field-error {
  color: var(--color-danger);
  font-size: 12px;
}
</style>
