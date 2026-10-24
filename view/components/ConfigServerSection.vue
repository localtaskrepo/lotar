<template>
  <ConfigGroup
    title="Server"
    description="Embedded HTTP server settings. Restart the server after changing them."
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
        type="number"
        min="1024"
        max="65535"
        step="1"
        inputmode="numeric"
        placeholder="8080"
        @blur="onBlur"
      />
      <p v-if="error" class="field-error">{{ error }}</p>
      <p class="field-hint">Any value from 1024 to 65535.</p>
    </div>
    <div class="field">
      <label class="field-label" for="config-web-ui-path">Web UI folder</label>
      <UiInput
        id="config-web-ui-path"
        v-model="webUiPath"
        maxlength="512"
        placeholder="Built-in web UI"
      />
      <p class="field-hint">Serve web UI files from this folder first, falling back to the built-in UI. Leave empty for the built-in UI only.</p>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import ConfigGroup from './ConfigGroup.vue'
import UiInput from './UiInput.vue'

const serverPort = defineModel<string>({ required: true })
const webUiPath = defineModel<string>('webUiPath', { required: true })

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

.field + .field {
  margin-top: 12px;
}
</style>
