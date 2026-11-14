<template>
  <ConfigGroup title="Tags & custom fields" :description="description">
    <div class="field-grid">
      <div class="field field--full" data-testid="config-tags">
        <label class="field-label" :for="`${uid}-tags-mode`">
          <span>Tags</span>
          <span v-if="tagsSource" :class="['provenance', provenanceClass(tagsSource)]">
            {{ provenanceLabel(tagsSource) }}
          </span>
        </label>
        <UiSelect :id="`${uid}-tags-mode`" v-model="tagsMode" class="cfg-mode-select" @change="handleUpdate('tags')">
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

      <ConfigTagColorsField
        v-model:entries="tagColors"
        :is-global="isGlobal"
        :project="project"
        :suggestions="tagColorSuggestions"
        :inherited-properties="globalTagProperties"
        :error="tagColorsError"
        :source="tagColorsSource"
        :provenance-label="provenanceLabel"
        :provenance-class="provenanceClass"
        @add="(tag?: string, color?: string) => emit('addTagColor', tag, color)"
        @remove="emit('removeTagColor', $event)"
        @validate="emit('validate', 'tag_properties')"
      />

      <div class="field field--full" data-testid="config-custom-fields">
        <label class="field-label" :for="`${uid}-custom-fields-mode`">
          <span>Custom fields</span>
          <span v-if="customFieldsSource" :class="['provenance', provenanceClass(customFieldsSource)]">
            {{ provenanceLabel(customFieldsSource) }}
          </span>
        </label>
        <UiSelect
          :id="`${uid}-custom-fields-mode`"
          v-model="customFieldsMode"
          class="cfg-mode-select"
          @change="handleUpdate('custom_fields')"
        >
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

      <div v-if="customFieldsMode !== 'none'" class="field field--full" data-testid="config-custom-fields-from">
        <label class="field-label" :for="`${uid}-custom-fields-from`">
          <span>Fields offered in pickers</span>
          <span v-if="customFieldsFromSource" :class="['provenance', provenanceClass(customFieldsFromSource)]">
            {{ provenanceLabel(customFieldsFromSource) }}
          </span>
        </label>
        <UiSelect :id="`${uid}-custom-fields-from`" v-model="uiCustomFieldsFrom" class="cfg-mode-select">
          <option v-if="!isGlobal" value="">
            Inherit global (currently {{ CUSTOM_FIELDS_FROM_LABELS[globalCustomFieldsFrom].toLowerCase() }})
          </option>
          <option v-for="(label, value) in CUSTOM_FIELDS_FROM_LABELS" :key="value" :value="value">{{ label }}</option>
        </UiSelect>
        <p class="field-hint">
          Which custom fields the board, task list, calendar and dependency cards offer as columns, filters and
          group-by. Used fields are the ones the tasks have.
        </p>
      </div>

      <div v-if="customFieldsMode !== 'none'" class="field field--full" data-testid="config-version-fields">
        <span class="field-label">
          <span>Version fields</span>
          <span v-if="versionFieldsSource" :class="['provenance', provenanceClass(versionFieldsSource)]">
            {{ provenanceLabel(versionFieldsSource) }}
          </span>
        </span>
        <ChipListField
          v-model="versionFields"
          placeholder="affects"
          add-label="Add version field"
          composer-label="Version field"
          empty-label="No version fields"
          :suggestions="versionFieldSuggestions"
          @update:modelValue="emit('validate', 'custom_field_properties')"
        />
        <p class="field-hint">
          These fields hold versions such as 1.4.2, 0.13 (every 0.13.x) or 1.0.0-rc.1. Values are checked when saved,
          and searches compare them: <code>affects&gt;=0.13 affects&lt;0.14</code>.
          <template v-if="inheritedVersionFields.length">
            Global version fields: {{ inheritedVersionFields.join(', ') }}.
          </template>
        </p>
        <p v-if="unlistedVersionFields.length" class="field-hint">
          Not in the custom field list, so not used: {{ unlistedVersionFields.join(', ') }}.
        </p>
        <p v-if="versionFieldsError" class="field-error">{{ versionFieldsError }}</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
import type { ConfigSource, CustomFieldProperties, CustomFieldsFrom, TagProperties } from '../api/types'
import type { ListMode } from '../composables/useConfigForm'
import type { TagColorEntry } from '../composables/useConfigForm'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import ConfigTagColorsField from './ConfigTagColorsField.vue'
import UiSelect from './UiSelect.vue'

const tagsMode = defineModel<ListMode>('tagsMode', { required: true })
const tags = defineModel<string[]>('tags', { required: true })
const customFieldsMode = defineModel<ListMode>('customFieldsMode', { required: true })
const customFields = defineModel<string[]>('customFields', { required: true })
const tagColors = defineModel<TagColorEntry[]>('tagColors', { required: true })
const versionFields = defineModel<string[]>('versionFields', { required: true })
/** `ui.custom_fields_from`; empty inherits in a project (DEV-485). */
const uiCustomFieldsFrom = defineModel<string>('uiCustomFieldsFrom', { required: true })

const CUSTOM_FIELDS_FROM_LABELS: Record<CustomFieldsFrom, string> = {
  both: 'Listed fields, then used ones',
  config: 'Only the listed fields',
  scanned: 'Only the fields tasks use',
}

const {
  description,
  isGlobal,
  project = '',
  globalTags = [],
  globalCustomFields = [],
  tagsError = null,
  customFieldsError = null,
  tagsSource,
  customFieldsSource,
  tagSuggestions = [],
  globalTagProperties = null,
  tagColorsError = null,
  tagColorsSource,
  globalFieldProperties = null,
  versionFieldsError = null,
  versionFieldsSource,
  globalCustomFieldsFrom = 'both',
  customFieldsFromSource,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  isGlobal: boolean
  /** Project prefix of the scope; empty for global. */
  project?: string
  /** Effective global values, summarized in the project inherit option. */
  globalTags?: string[]
  globalCustomFields?: string[]
  tagsError?: string | null
  customFieldsError?: string | null
  tagsSource?: ConfigSource
  customFieldsSource?: ConfigSource
  /** Known tags, offered when naming a tag colour. */
  tagSuggestions?: string[]
  /** Project scope: global tag properties, shown for reference. */
  globalTagProperties?: Record<string, TagProperties> | null
  tagColorsError?: string | null
  tagColorsSource?: ConfigSource
  /** Project scope: global custom field properties, shown for reference. */
  globalFieldProperties?: Record<string, CustomFieldProperties> | null
  versionFieldsError?: string | null
  versionFieldsSource?: ConfigSource
  /** Project scope: the global `ui.custom_fields_from`, named in the inherit option. */
  globalCustomFieldsFrom?: CustomFieldsFrom
  customFieldsFromSource?: ConfigSource
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'tags' | 'custom_fields' | 'tag_properties' | 'custom_field_properties'): void
  (e: 'addTagColor', tag?: string, color?: string): void
  (e: 'removeTagColor', index: number): void
}>()

// The listed tags first, then other known tags, without the wildcard.
const tagColorSuggestions = computed(() =>
  [...new Set([...tags.value, ...tagSuggestions])].filter((tag) => tag && tag !== '*'),
)

const versionFieldSuggestions = computed(() =>
  customFields.value.filter((name) => name && name !== '*' && !versionFields.value.includes(name)),
)

const inheritedVersionFields = computed(() =>
  Object.entries(globalFieldProperties ?? {})
    .filter(([, props]) => props.type === 'version')
    .map(([name]) => name),
)

// With a fixed list, a version field outside it is never offered.
const unlistedVersionFields = computed(() => {
  if (customFieldsMode.value !== 'list') return []
  const listed = new Set(customFields.value.map((name) => name.toLowerCase()))
  return versionFields.value.filter((name) => !listed.has(name.toLowerCase()))
})

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
      return noun === 'tags' ? 'Tasks may use any tag.' : 'Tasks may add any custom field (stored as the "*" wildcard).'
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
.cfg-mode-select {
  max-width: 420px;
}
</style>
