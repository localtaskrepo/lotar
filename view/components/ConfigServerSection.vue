<template>
  <ConfigGroup
    title="Server"
    description="Embedded web server settings. Restart the server after changing them."
    :source="groupSource"
  >
    <div class="field-grid" data-testid="config-server">
      <div class="field">
        <label class="field-label" for="config-server-port">
          <span>Server port</span>
          <span v-if="fieldSourceLabel" class="provenance" :class="fieldSourceClass">{{ fieldSourceLabel }}</span>
        </label>
        <UiInput
          id="config-server-port"
          v-model="serverPort"
          type="number"
          min="1024"
          max="65535"
          step="1"
          inputmode="numeric"
          placeholder="8080"
          @blur="onBlur"
        />
        <p class="field-hint">1024–65535.</p>
        <p v-if="error" class="field-error">{{ error }}</p>
      </div>
      <div class="field field--full">
        <label class="field-label" for="config-web-ui-path">Web UI folder</label>
        <UiInput
          id="config-web-ui-path"
          v-model="webUiPath"
          maxlength="512"
          placeholder="Built-in web UI"
        />
        <p class="field-hint">Serve web UI files from this folder first, falling back to the built-in UI. Leave empty for the built-in UI only.</p>
      </div>
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
