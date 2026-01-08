<template>
  <ConfigGroup
    title="Server"
    description="Port used by the embedded HTTP server."
    :source="groupSource"
  >
    <div class="field">
      <label class="field-label">
        <span>Server port</span>
        <span v-if="fieldSourceLabel" class="provenance" :class="fieldSourceClass">{{ fieldSourceLabel }}</span>
      </label>
      <UiInput
        v-model="serverPort"
        class="server-port-input"
        inputmode="numeric"
        maxlength="5"
        placeholder="8080"
        @blur="onBlur"
      />
      <p v-if="error" class="field-error">{{ error }}</p>
      <p class="field-hint">Any value &gt;= 1024 works. Restart the server if you change this.</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import UiInput from './UiInput.vue'

const serverPort = defineModel<string>({ required: true })

const props = defineProps<{
  error?: string | null
  groupSource?: ConfigSource
  fieldSourceLabel?: string
  fieldSourceClass?: string
}>()

const error = computed(() => props.error ?? null)
const groupSource = computed<ConfigSource | undefined>(() =>
  props.groupSource === 'built_in' ? undefined : props.groupSource,
)
const fieldSourceLabel = computed(() => props.fieldSourceLabel ?? '')
const fieldSourceClass = computed(() => props.fieldSourceClass ?? '')

const emit = defineEmits<{ (e: 'validate'): void }>()

function onBlur() {
  emit('validate')
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

.field-error {
  color: var(--color-danger);
  font-size: 12px;
}

.field-hint {
  color: var(--color-muted);
  font-size: 12px;
}

.server-port-input {
  width: min(120px, 100%);
}
</style>
