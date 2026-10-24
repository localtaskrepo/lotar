<template>
  <ConfigGroup title="Sprint defaults" description="Applied when a new sprint omits these values. Global only.">
    <div class="field-grid" data-testid="config-sprints">
      <div class="field">
        <label class="field-label" :for="`${uid}-length`">
          <span>Length</span>
          <span v-if="sourceFor('sprints_defaults_length')" :class="['provenance', provenanceClass(sourceFor('sprints_defaults_length'))]">
            {{ provenanceLabel(sourceFor('sprints_defaults_length')) }}
          </span>
        </label>
        <UiInput :id="`${uid}-length`" v-model="length" maxlength="32" placeholder="2w" />
        <p class="field-hint">Days or weeks, e.g. 14d or 2w. Leave empty for no default.</p>
      </div>
      <div class="field">
        <label class="field-label" :for="`${uid}-overdue`">
          <span>Overdue after</span>
          <span v-if="sourceFor('sprints_defaults_overdue_after')" :class="['provenance', provenanceClass(sourceFor('sprints_defaults_overdue_after'))]">
            {{ provenanceLabel(sourceFor('sprints_defaults_overdue_after')) }}
          </span>
        </label>
        <UiInput :id="`${uid}-overdue`" v-model="overdueAfter" maxlength="32" placeholder="2d" />
        <p class="field-hint">Grace period past the planned end before a sprint is reported overdue.</p>
      </div>
      <div class="field">
        <label class="field-label" :for="`${uid}-points`">Capacity (points)</label>
        <UiInput
          :id="`${uid}-points`"
          v-model="capacityPoints"
          type="number"
          min="0"
          step="1"
          inputmode="numeric"
          placeholder="No default"
          @blur="emit('validate', 'sprints_defaults_capacity_points')"
        />
        <p v-if="errors.sprints_defaults_capacity_points" class="field-error">{{ errors.sprints_defaults_capacity_points }}</p>
      </div>
      <div class="field">
        <label class="field-label" :for="`${uid}-hours`">Capacity (hours)</label>
        <UiInput
          :id="`${uid}-hours`"
          v-model="capacityHours"
          type="number"
          min="0"
          step="1"
          inputmode="numeric"
          placeholder="No default"
          @blur="emit('validate', 'sprints_defaults_capacity_hours')"
        />
        <p v-if="errors.sprints_defaults_capacity_hours" class="field-error">{{ errors.sprints_defaults_capacity_hours }}</p>
      </div>
      <ConfigToggleField
        class="field--full"
        label="Sprint warnings"
        v-model="notificationsEnabled"
        :is-global="true"
        enabled-label="Warn about overdue, future or overlapping sprints"
        :source-label="provenanceLabel(sourceFor('sprints_notifications_enabled'))"
        :source-class="provenanceClass(sourceFor('sprints_notifications_enabled'))"
      />
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { useId } from 'vue'
import type { ConfigSource } from '../api/types'
import type { ToggleValue } from '../composables/useConfigForm'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'
import UiInput from './UiInput.vue'

const length = defineModel<string>('length', { required: true })
const overdueAfter = defineModel<string>('overdueAfter', { required: true })
const capacityPoints = defineModel<string>('capacityPoints', { required: true })
const capacityHours = defineModel<string>('capacityHours', { required: true })
const notificationsEnabled = defineModel<ToggleValue>('notificationsEnabled', { required: true })

const { errors, sourceFor, provenanceLabel, provenanceClass } = defineProps<{
  errors: Record<string, string | null>
  sourceFor: (field: string) => ConfigSource | undefined
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'sprints_defaults_capacity_points' | 'sprints_defaults_capacity_hours'): void
}>()

const uid = useId()
</script>

<style scoped>
.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field--full {
  grid-column: 1 / -1;
}

.field-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.field :deep(.input) {
  height: var(--config-control-height, 40px);
  box-sizing: border-box;
}

.field-error {
  color: var(--color-danger);
  font-size: 12px;
}

.field-hint {
  color: var(--color-muted);
  font-size: 12px;
}
</style>
