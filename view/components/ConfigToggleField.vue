<template>
  <div class="cfg-toggle">
    <div class="cfg-toggle__text">
      <span :id="labelId" class="cfg-toggle__label">
        {{ label }}
        <span v-if="sourceLabel" class="provenance" :class="sourceClass">{{ sourceLabel }}</span>
      </span>
      <span v-if="descriptionText" :id="descriptionId" class="cfg-toggle__description">{{ descriptionText }}</span>
    </div>
    <label v-if="isGlobal" class="cfg-switch">
      <input
        type="checkbox"
        role="switch"
        :checked="model === 'true'"
        :aria-labelledby="labelId"
        :aria-describedby="descriptionText ? descriptionId : undefined"
        @change="onSwitchChange"
      />
      <span class="cfg-switch__track" aria-hidden="true"><span class="cfg-switch__thumb" /></span>
    </label>
    <UiSelect
      v-else
      v-model="model"
      class="cfg-toggle__select"
      :aria-labelledby="labelId"
      :aria-describedby="descriptionText ? descriptionId : undefined"
    >
      <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
    </UiSelect>
  </div>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
import UiSelect from './UiSelect.vue'

type ToggleValue = 'inherit' | 'true' | 'false'

type ToggleOption = {
  value: ToggleValue
  label: string
}

const props = withDefaults(
  defineProps<{
    label: string
    isGlobal: boolean
    options?: ToggleOption[]
    /** One-line explanation shown under the label. */
    description?: string
    /** Legacy alias for `description`. */
    enabledLabel?: string
    /** Deprecated: the inherit option label already states the global value. */
    hint?: string
    sourceLabel?: string
    sourceClass?: string
  }>(),
  {
    options: () => [],
    description: undefined,
    enabledLabel: undefined,
    hint: undefined,
    sourceLabel: undefined,
    sourceClass: undefined,
  },
)

const model = defineModel<ToggleValue>({ required: true })

const uid = useId()
const labelId = `${uid}-label`
const descriptionId = `${uid}-description`
const descriptionText = computed(() => props.description ?? props.enabledLabel ?? '')

function onSwitchChange(event: Event) {
  model.value = (event.target as HTMLInputElement).checked ? 'true' : 'false'
}
</script>
