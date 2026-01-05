<template>
  <details v-if="showSection" class="task-panel__group" :open="detailsOpen" @toggle="onToggle">
    <summary>Custom fields</summary>
    <div class="task-panel__custom-fields">
      <div v-for="(value, key) in customFields" :key="key" class="task-panel__custom-row">
        <UiInput
          :model-value="customFieldKeys[key]"
          placeholder="Field name"
          @update:modelValue="(val) => emit('updateCustomFieldKey', key, val)"
          @blur="emit('commit')"
        />
        <UiInput
          :model-value="value"
          placeholder="Value"
          @update:modelValue="(val) => emit('updateCustomFieldValue', key, val)"
          @blur="emit('commit')"
        />
        <UiButton variant="ghost" type="button" @click="emit('removeField', key)">Remove</UiButton>
      </div>
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
import { computed, ref, watch } from 'vue';
import UiButton from '../UiButton.vue';
import UiInput from '../UiInput.vue';

const props = defineProps<{
  customFields: Record<string, string>
  customFieldKeys: Record<string, string>
  newFieldKey: string
  newFieldValue: string
  allowNewFields: boolean
}>()

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


