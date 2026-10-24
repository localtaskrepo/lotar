<template>
  <ConfigGroup title="Defaults for new tasks" :description="description">
    <div class="field-grid" data-testid="config-defaults">
      <div class="field">
        <label class="field-label" :for="`${uid}-status`">
          <span>Default status</span>
          <span v-if="sourceFor('default_status')" :class="['provenance', provenanceClass(sourceFor('default_status'))]">
            {{ provenanceLabel(sourceFor('default_status')) }}
          </span>
        </label>
        <UiSelect :id="`${uid}-status`" v-model="defaultStatus" @change="emit('validate', 'default_status')">
          <option v-if="isGlobal" value="">First status{{ statusOptions[0] ? ` (${statusOptions[0]})` : '' }}</option>
          <option v-else value="">Inherit global ({{ globalStatus || statusOptions[0] || 'first status' }})</option>
          <option v-for="option in statusOptions" :key="option" :value="option">{{ option }}</option>
        </UiSelect>
        <p v-if="errors.default_status" class="field-error">{{ errors.default_status }}</p>
      </div>

      <div class="field">
        <label class="field-label" :for="`${uid}-priority`">
          <span>Default priority</span>
          <span v-if="sourceFor('default_priority')" :class="['provenance', provenanceClass(sourceFor('default_priority'))]">
            {{ provenanceLabel(sourceFor('default_priority')) }}
          </span>
        </label>
        <UiSelect :id="`${uid}-priority`" v-model="defaultPriority" @change="emit('validate', 'default_priority')">
          <option v-if="!isGlobal" value="">Inherit global ({{ globalPriority || 'not set' }})</option>
          <option v-for="option in priorityOptions" :key="option" :value="option">{{ option }}</option>
        </UiSelect>
        <p v-if="errors.default_priority" class="field-error">{{ errors.default_priority }}</p>
      </div>

      <div class="field">
        <label class="field-label" :for="`${uid}-reporter`">
          <span>Default reporter</span>
          <span v-if="sourceFor('default_reporter')" :class="['provenance', provenanceClass(sourceFor('default_reporter'))]">
            {{ provenanceLabel(sourceFor('default_reporter')) }}
          </span>
        </label>
        <UiInput
          :id="`${uid}-reporter`"
          v-model="defaultReporter"
          maxlength="100"
          placeholder="@me"
          @blur="emit('validate', 'default_reporter')"
        />
        <p v-if="errors.default_reporter" class="field-error">{{ errors.default_reporter }}</p>
      </div>

      <div class="field">
        <label class="field-label" :for="`${uid}-assignee`">
          <span>Default assignee</span>
          <span v-if="sourceFor('default_assignee')" :class="['provenance', provenanceClass(sourceFor('default_assignee'))]">
            {{ provenanceLabel(sourceFor('default_assignee')) }}
          </span>
        </label>
        <UiInput
          :id="`${uid}-assignee`"
          v-model="defaultAssignee"
          maxlength="100"
          placeholder="Optional"
          @blur="emit('validate', 'default_assignee')"
        />
        <p v-if="errors.default_assignee" class="field-error">{{ errors.default_assignee }}</p>
      </div>

      <div v-if="isGlobal" class="field">
        <label class="field-label" :for="`${uid}-project`">
          <span>Default project prefix</span>
          <span v-if="sourceFor('default_project')" :class="['provenance', provenanceClass(sourceFor('default_project'))]">
            {{ provenanceLabel(sourceFor('default_project')) }}
          </span>
        </label>
        <UiInput
          :id="`${uid}-project`"
          v-model="defaultProject"
          maxlength="20"
          placeholder="ACME"
          @blur="emit('validate', 'default_project')"
        />
        <p class="field-hint">Used when a command does not name a project.</p>
        <p v-if="errors.default_project" class="field-error">{{ errors.default_project }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label">
          <span>Default tags</span>
          <span v-if="sourceFor('default_tags')" :class="['provenance', provenanceClass(sourceFor('default_tags'))]">
            {{ provenanceLabel(sourceFor('default_tags')) }}
          </span>
        </label>
        <ChipListField
          v-model="defaultTags"
          :suggestions="tagSuggestions"
          placeholder="Add a tag"
          add-label="Add tag"
          composer-label="Tag"
          empty-label="No default tags"
          @update:modelValue="emit('validate', 'default_tags')"
        />
        <p class="field-hint">Applied when a new task is created without tags.</p>
        <p v-if="errors.default_tags" class="field-error">{{ errors.default_tags }}</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { useId } from 'vue'
import type { ConfigSource } from '../api/types'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

type DefaultsField =
  | 'default_project'
  | 'default_status'
  | 'default_priority'
  | 'default_reporter'
  | 'default_assignee'
  | 'default_tags'

const defaultProject = defineModel<string>('defaultProject', { required: true })
const defaultStatus = defineModel<string>('defaultStatus', { required: true })
const defaultPriority = defineModel<string>('defaultPriority', { required: true })
const defaultReporter = defineModel<string>('defaultReporter', { required: true })
const defaultAssignee = defineModel<string>('defaultAssignee', { required: true })
const defaultTags = defineModel<string[]>('defaultTags', { required: true })

const {
  description,
  isGlobal,
  errors,
  statusOptions,
  priorityOptions,
  tagSuggestions = [],
  globalStatus = null,
  globalPriority = null,
  sourceFor,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  isGlobal: boolean
  errors: Record<string, string | null>
  statusOptions: string[]
  priorityOptions: string[]
  tagSuggestions?: string[]
  /** Effective global values, shown in the project inherit options. */
  globalStatus?: string | null
  globalPriority?: string | null
  sourceFor: (field: string) => ConfigSource | undefined
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{ (e: 'validate', field: DefaultsField): void }>()

const uid = useId()
</script>

