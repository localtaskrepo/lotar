<template>
  <div class="toggle-field">
    <label class="field-label">
      <span>{{ label }}</span>
      <span v-if="sourceLabel" class="provenance" :class="sourceClass">{{ sourceLabel }}</span>
    </label>
    <template v-if="isGlobal">
      <label class="toggle-control">
        <input type="checkbox" :checked="modelValue === 'true'" @change="onCheckboxChange" />
        <span>{{ enabledLabel }}</span>
      </label>
    </template>
    <template v-else>
      <UiSelect v-model="modelValue">
        <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
      </UiSelect>
      <p v-if="hint" class="field-hint">{{ hint }}</p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, toRefs } from 'vue'
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
    hint?: string
    sourceLabel?: string
    sourceClass?: string
    enabledLabel?: string
  }>(),
  {
    options: () => [],
    hint: undefined,
    sourceLabel: undefined,
    sourceClass: undefined,
    enabledLabel: 'Enabled',
  },
)

const model = defineModel<ToggleValue>({
  required: true,
})

const { label, isGlobal, options, hint, sourceLabel, sourceClass, enabledLabel } = toRefs(props)

const modelValue = computed({
  get: () => model.value,
  set: (value: ToggleValue) => {
    model.value = value
  },
})

function onCheckboxChange(event: Event) {
  const target = event.target as HTMLInputElement
  model.value = target.checked ? 'true' : 'false'
}
</script>

<style scoped>
.toggle-field {
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

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.source-project {
  background: rgba(0, 180, 120, 0.25);
  color: #9ef0d0;
}

.source-global {
  background: rgba(0, 120, 255, 0.2);
  color: #8bc0ff;
}

.source-built_in {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.85);
}

.toggle-control {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--color-fg);
}

.toggle-control input {
  width: 14px;
  height: 14px;
}

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}
</style>
