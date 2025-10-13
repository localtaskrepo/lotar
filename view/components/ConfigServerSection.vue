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
      <UiInput v-model="serverPort" inputmode="numeric" placeholder="8080" @blur="onBlur" />
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
const groupSource = computed(() => props.groupSource)
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

.provenance {
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.field-error {
  color: #ff8091;
  font-size: 12px;
}

.field-hint {
  color: rgba(255, 255, 255, 0.6);
  font-size: 12px;
}
</style>
