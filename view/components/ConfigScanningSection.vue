<template>
  <ConfigGroup title="Source scanning" :description="description">
    <div class="field-grid" data-testid="config-scanning">
      <div class="field field--full">
        <label class="field-label">
          <span>Signal words</span>
          <span v-if="signalWordsSource" :class="['provenance', provenanceClass(signalWordsSource)]">
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
        <p class="field-hint">Comments starting with these words (such as TODO or FIXME) can become tasks.</p>
        <p v-if="scanSignalWordsError" class="field-error">{{ scanSignalWordsError }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label">
          <span>Ticket patterns</span>
          <span v-if="ticketPatternsSource" :class="['provenance', provenanceClass(ticketPatternsSource)]">
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
        <p class="field-hint">Regular expressions that recognise ticket keys in source code.</p>
        <p v-if="scanTicketPatternsError" class="field-error">{{ scanTicketPatternsError }}</p>
      </div>

      <div class="cfg-toggles">
        <ConfigToggleField
          label="Type names count as signal words"
          v-model="scanEnableTicketWords"
          :is-global="isGlobal"
          description="Treat issue type words such as Bug or Feature like TODO and FIXME."
          :options="toggleSelectOptions('scanEnableTicketWords')"
          :source-label="provenanceLabel(sourceFor('scan_enable_ticket_words'))"
          :source-class="provenanceClass(sourceFor('scan_enable_ticket_words'))"
        />
        <ConfigToggleField
          label="Record ticket mentions"
          v-model="scanEnableMentions"
          :is-global="isGlobal"
          description="Add a code reference to existing tasks whose key appears in source files."
          :options="toggleSelectOptions('scanEnableMentions')"
          :source-label="provenanceLabel(sourceFor('scan_enable_mentions'))"
          :source-class="provenanceClass(sourceFor('scan_enable_mentions'))"
        />
        <ConfigToggleField
          label="Remove inline attributes"
          v-model="scanStripAttributes"
          :is-global="isGlobal"
          description="Strip [key=value] blocks from comments after inserting the ticket key."
          :options="toggleSelectOptions('scanStripAttributes')"
          :source-label="provenanceLabel(sourceFor('scan_strip_attributes'))"
          :source-class="provenanceClass(sourceFor('scan_strip_attributes'))"
        />
      </div>
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

const scanSignalWords = defineModel<string[]>('scanSignalWords', { required: true })
const scanTicketPatterns = defineModel<string[]>('scanTicketPatterns', { required: true })
const scanEnableTicketWords = defineModel<ToggleValue>('scanEnableTicketWords', { required: true })
const scanEnableMentions = defineModel<ToggleValue>('scanEnableMentions', { required: true })
const scanStripAttributes = defineModel<ToggleValue>('scanStripAttributes', { required: true })

defineProps<{
  description: string
  isGlobal: boolean
  toggleSelectOptions: (field: ToggleField) => ToggleOption[]
  /** Unused since the inherit option states the global value; kept for callers. */
  globalToggleSummary?: (field: ToggleField) => string
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
</script>
