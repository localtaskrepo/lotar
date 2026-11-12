<template>
  <details v-if="showSection" class="task-panel__group" :open="detailsOpen" @toggle="onToggle">
    <summary>Custom fields</summary>
    <div class="task-panel__custom-fields">
      <template v-for="(value, key) in customFields" :key="key">
        <div class="task-panel__custom-row" :data-version-field="isVersionField(key) ? 'true' : undefined">
          <UiInput
            :model-value="customFieldKeys[key]"
            placeholder="Field name"
            @update:modelValue="(val) => emit('updateCustomFieldKey', key, val)"
            @blur="commit"
          />
          <UiInput
            :model-value="value"
            :placeholder="isVersionField(key) ? 'Version, e.g. 0.13.2' : 'Value'"
            :aria-invalid="valueError(key, value) ? 'true' : undefined"
            :aria-describedby="valueError(key, value) ? errorId(key) : undefined"
            :list="versionSuggestions(key).length ? suggestionsId(key) : undefined"
            @update:modelValue="(val) => emit('updateCustomFieldValue', key, val)"
            @blur="commit"
          />
          <datalist v-if="versionSuggestions(key).length" :id="suggestionsId(key)">
            <option v-for="version in versionSuggestions(key)" :key="version" :value="version" />
          </datalist>
          <UiButton variant="ghost" type="button" @click="emit('removeField', key)">Remove</UiButton>
        </div>
        <p v-if="valueError(key, value)" :id="errorId(key)" class="task-panel__custom-error" role="alert">
          {{ valueError(key, value) }}
        </p>
      </template>
      <div v-if="allowNewFields" class="task-panel__custom-row">
        <UiInput
          :model-value="newFieldKey"
          placeholder="New field"
          @update:modelValue="(val) => emit('updateNewFieldKey', val)"
        />
        <UiInput
          :model-value="newFieldValue"
          placeholder="Value"
          @update:modelValue="(val) => emit('updateNewFieldValue', val)"
        />
        <UiButton type="button" @click="emit('addField')">Add</UiButton>
      </div>
    </div>
  </details>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { compareVersionText, isVersion, versionValueError } from '../../utils/versions'
import UiButton from '../UiButton.vue'
import UiInput from '../UiInput.vue'

const props = defineProps<{
  customFields: Record<string, string>
  customFieldKeys: Record<string, string>
  newFieldKey: string
  newFieldValue: string
  allowNewFields: boolean
  /** Fields that hold versions (DEV-373): their values are checked here. */
  versionFields?: string[]
  /** Values the project's tasks hold, by field name: a version input suggests them (DEV-395). */
  knownValues?: Record<string, string[]>
}>()

const uid = useId()
const versionNames = computed(() => new Set((props.versionFields ?? []).map((name) => name.toLowerCase())))

function isVersionField(key: string): boolean {
  return versionNames.value.has((props.customFieldKeys[key] || key).trim().toLowerCase())
}

function valueError(key: string, value: string): string | null {
  return isVersionField(key) ? versionValueError(String(value ?? '')) : null
}

// Versions already used in this field, so `0.15` and `0.15.0` don't drift apart.
function versionSuggestions(key: string): string[] {
  if (!isVersionField(key)) return []
  const name = (props.customFieldKeys[key] || key).trim().toLowerCase()
  const values = Object.entries(props.knownValues ?? {})
    .filter(([field]) => field.toLowerCase() === name)
    .flatMap(([, list]) => list)
  return [...new Set(values.filter(isVersion))].sort(compareVersionText)
}

function suggestionsId(key: string): string {
  return `${errorId(key)}-versions`
}

function errorId(key: string): string {
  return `${uid}-custom-${key.replace(/\W+/g, '-')}`
}

// A value the server would reject stays in the editor, flagged, until fixed.
function commit() {
  const invalid = Object.entries(props.customFields || {}).some(([key, value]) => valueError(key, value))
  if (!invalid) emit('commit')
}

const hasAnyField = computed(() => Object.keys(props.customFields || {}).length > 0)
const hasAnyValue = computed(() =>
  Object.entries(props.customFields || {}).some(([key, value]) => (key || '').trim() && String(value ?? '').trim()),
)

const showSection = computed(() => props.allowNewFields || hasAnyField.value)

const detailsOpen = ref(hasAnyValue.value)
const userToggled = ref(false)

watch(hasAnyValue, (next) => {
  if (!userToggled.value) {
    detailsOpen.value = next
  }
})

function onToggle(event: Event) {
  userToggled.value = true
  detailsOpen.value = (event.target as HTMLDetailsElement).open
}

const emit = defineEmits<{
  (e: 'updateCustomFieldKey', key: string, value: string): void
  (e: 'updateCustomFieldValue', key: string, value: string): void
  (e: 'updateNewFieldKey', value: string): void
  (e: 'updateNewFieldValue', value: string): void
  (e: 'addField'): void
  (e: 'removeField', key: string): void
  (e: 'commit'): void
}>()
</script>

<style scoped>
.task-panel__custom-error {
  margin: calc(-1 * var(--space-1, 0.25rem)) 0 0;
  color: var(--color-danger);
  font-size: var(--text-sm, 0.875rem);
}
</style>
