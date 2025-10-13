<template>
  <details class="task-panel__group" open>
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
      <div class="task-panel__custom-row">
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
import UiButton from '../UiButton.vue';
import UiInput from '../UiInput.vue';

defineProps<{
  customFields: Record<string, string>
  customFieldKeys: Record<string, string>
  newFieldKey: string
  newFieldValue: string
}>()

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


