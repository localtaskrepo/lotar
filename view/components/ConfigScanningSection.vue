<template>
  <ConfigGroup title="Scanning &amp; parsing" :description="description">
    <div class="field">
      <label class="field-label">
        <span>Signal words</span>
        <span
          v-if="signalWordsSource"
          :class="['provenance', provenanceClass(signalWordsSource)]"
        >
          {{ provenanceLabel(signalWordsSource) }}
        </span>
      </label>
      <ChipListField
        v-model="scanSignalWords"
        placeholder="TODO"
        add-label="Add word"
        composer-label="Signal word"
        empty-label="No signal words"
        @update:modelValue="handleUpdate('scan_signal_words')"
      />
      <p class="field-hint">Found words highlight source code for review. Keep them short.</p>
      <p v-if="scanSignalWordsError" class="field-error">{{ scanSignalWordsError }}</p>
    </div>

    <div class="field">
      <label class="field-label">
        <span>Ticket patterns</span>
        <span
          v-if="ticketPatternsSource"
          :class="['provenance', provenanceClass(ticketPatternsSource)]"
        >
          {{ provenanceLabel(ticketPatternsSource) }}
        </span>
      </label>
      <ChipListField
        v-model="scanTicketPatterns"
        placeholder="ABC-\d+"
        add-label="Add pattern"
        composer-label="Pattern"
        empty-label="No ticket patterns"
        @update:modelValue="handleUpdate('scan_ticket_patterns')"
      />
      <p class="field-hint">Regex patterns matched in commits or code trigger ticket discovery.</p>
      <p v-if="scanTicketPatternsError" class="field-error">{{ scanTicketPatternsError }}</p>
    </div>

    <div class="toggle-grid">
      <ConfigToggleField
        label="Flag tickets as signal words"
        v-model="scanEnableTicketWords"
        :is-global="isGlobal"
        :options="toggleSelectOptions('scanEnableTicketWords')"
        :hint="hint('scanEnableTicketWords')"
        :source-label="provenanceLabel(sourceFor('scan_enable_ticket_words'))"
        :source-class="provenanceClass(sourceFor('scan_enable_ticket_words'))"
      />
      <ConfigToggleField
        label="Emit mention references"
        v-model="scanEnableMentions"
        :is-global="isGlobal"
        :options="toggleSelectOptions('scanEnableMentions')"
        :hint="hint('scanEnableMentions')"
        :source-label="provenanceLabel(sourceFor('scan_enable_mentions'))"
        :source-class="provenanceClass(sourceFor('scan_enable_mentions'))"
      />
      <ConfigToggleField
        label="Strip attributes when scanning"
        v-model="scanStripAttributes"
        :is-global="isGlobal"
        :options="toggleSelectOptions('scanStripAttributes')"
        :hint="hint('scanStripAttributes')"
        :source-label="provenanceLabel(sourceFor('scan_strip_attributes'))"
        :source-class="provenanceClass(sourceFor('scan_strip_attributes'))"
      />
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import type { ConfigSource } from '../api/types'
import type { ToggleField, ToggleValue } from '../composables/useConfigForm'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'

type ToggleOption = { value: ToggleValue; label: string }

type ValidateField = 'scan_signal_words' | 'scan_ticket_patterns'

type ToggleFieldMap =
  | 'scanEnableTicketWords'
  | 'scanEnableMentions'
  | 'scanStripAttributes'

const scanSignalWords = defineModel<string[]>('scanSignalWords', { required: true })
const scanTicketPatterns = defineModel<string[]>('scanTicketPatterns', { required: true })
const scanEnableTicketWords = defineModel<ToggleValue>('scanEnableTicketWords', { required: true })
const scanEnableMentions = defineModel<ToggleValue>('scanEnableMentions', { required: true })
const scanStripAttributes = defineModel<ToggleValue>('scanStripAttributes', { required: true })

const props = defineProps<{
  description: string
  isGlobal: boolean
  toggleSelectOptions: (field: ToggleField) => ToggleOption[]
  globalToggleSummary: (field: ToggleField) => string
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  sourceFor: (field: string) => ConfigSource | undefined
  scanSignalWordsError?: string | null
  scanTicketPatternsError?: string | null
  signalWordsSource?: ConfigSource
  ticketPatternsSource?: ConfigSource
}>()

const emit = defineEmits<{
  (e: 'validate', field: ValidateField): void
}>()

function handleUpdate(field: ValidateField) {
  emit('validate', field)
}

function hint(field: ToggleField) {
  if (props.isGlobal) return ''
  const summary = props.globalToggleSummary(field)
  return `Global: ${summary || 'unknown'}`
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

.field-hint {
  color: var(--color-muted);
  font-size: 12px;
}

.field-error {
  color: var(--color-danger);
  font-size: 12px;
}

.toggle-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}

</style>
