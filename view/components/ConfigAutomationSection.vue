<template>
  <ConfigGroup title="Automatic behavior" :description="description" :source="sanitizedGroupSource">
    <div class="field-grid" data-testid="config-automation">
      <div v-for="group in groups" :key="group.title" class="field field--full">
        <h3 class="cfg-group-heading">{{ group.title }}</h3>
        <div class="cfg-toggles">
          <ConfigToggleField
            v-for="toggle in group.toggles"
            :key="toggle.field"
            :label="toggle.label"
            :description="toggle.description"
            :model-value="values[toggle.field].value"
            :is-global="isGlobal"
            :options="toggleSelectOptions(toggle.field)"
            :source-label="provenanceLabel(sourceFor(toggle.key))"
            :source-class="provenanceClass(sourceFor(toggle.key))"
            @update:model-value="values[toggle.field].value = $event"
          />
        </div>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed, type Ref } from 'vue'
import type { ConfigSource } from '../api/types'
import type { ToggleField, ToggleValue } from '../composables/useConfigForm'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'

type ToggleOption = { value: ToggleValue; label: string }
type AutomationField =
  | 'autoSetReporter'
  | 'autoAssignOnStatus'
  | 'autoCodeownersAssign'
  | 'autoTagsFromPath'
  | 'autoBranchInferType'
  | 'autoBranchInferStatus'
  | 'autoBranchInferPriority'
  | 'autoIdentity'
  | 'autoIdentityGit'

const autoSetReporter = defineModel<ToggleValue>('autoSetReporter', { required: true })
const autoAssignOnStatus = defineModel<ToggleValue>('autoAssignOnStatus', { required: true })
const autoCodeownersAssign = defineModel<ToggleValue>('autoCodeownersAssign', { required: true })
const autoTagsFromPath = defineModel<ToggleValue>('autoTagsFromPath', { required: true })
const autoBranchInferType = defineModel<ToggleValue>('autoBranchInferType', { required: true })
const autoBranchInferStatus = defineModel<ToggleValue>('autoBranchInferStatus', { required: true })
const autoBranchInferPriority = defineModel<ToggleValue>('autoBranchInferPriority', { required: true })
const autoIdentity = defineModel<ToggleValue>('autoIdentity', { required: true })
const autoIdentityGit = defineModel<ToggleValue>('autoIdentityGit', { required: true })

const values: Record<AutomationField, Ref<ToggleValue>> = {
  autoSetReporter,
  autoAssignOnStatus,
  autoCodeownersAssign,
  autoTagsFromPath,
  autoBranchInferType,
  autoBranchInferStatus,
  autoBranchInferPriority,
  autoIdentity,
  autoIdentityGit,
}

const props = defineProps<{
  description: string
  groupSource?: ConfigSource
  isGlobal: boolean
  toggleSelectOptions: (field: ToggleField) => ToggleOption[]
  /** Unused since the inherit option states the global value; kept for callers. */
  globalToggleSummary?: (field: ToggleField) => string
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  sourceFor: (field: string) => ConfigSource | undefined
}>()

type ToggleDefinition = { field: AutomationField; key: string; label: string; description: string }

const groups: Array<{ title: string; toggles: ToggleDefinition[] }> = [
  {
    title: 'Ownership',
    toggles: [
      {
        field: 'autoSetReporter',
        key: 'auto_set_reporter',
        label: 'Set the reporter automatically',
        description: 'Fill in the reporter from the current identity when a task has none.',
      },
      {
        field: 'autoAssignOnStatus',
        key: 'auto_assign_on_status',
        label: 'Assign on first status change',
        description: 'Assign an unassigned task to the current person when its status first changes.',
      },
      {
        field: 'autoCodeownersAssign',
        key: 'auto_codeowners_assign',
        label: 'Prefer CODEOWNERS',
        description: 'When assigning on a status change, prefer the CODEOWNERS owner.',
      },
    ],
  },
  {
    title: 'Identity',
    toggles: [
      {
        field: 'autoIdentity',
        key: 'auto_identity',
        label: 'Detect who “@me” is',
        description: 'Look beyond the configured default reporter to resolve the current person.',
      },
      {
        field: 'autoIdentityGit',
        key: 'auto_identity_git',
        label: 'Use git identity',
        description: 'Allow the git user name and email to resolve “@me”.',
      },
    ],
  },
  {
    title: 'Inference from branches and paths',
    toggles: [
      {
        field: 'autoBranchInferType',
        key: 'auto_branch_infer_type',
        label: 'Type from branch name',
        description: 'Infer the task type from prefixes such as feat/, fix/ or chore/.',
      },
      {
        field: 'autoBranchInferStatus',
        key: 'auto_branch_infer_status',
        label: 'Status from branch name',
        description: 'Infer the status using the status branch aliases.',
      },
      {
        field: 'autoBranchInferPriority',
        key: 'auto_branch_infer_priority',
        label: 'Priority from branch name',
        description: 'Infer the priority using the priority branch aliases.',
      },
      {
        field: 'autoTagsFromPath',
        key: 'auto_tags_from_path',
        label: 'Tags from monorepo path',
        description: 'Tag new tasks from paths like packages/<name> when no tags or defaults apply.',
      },
    ],
  },
]

const sanitizedGroupSource = computed<ConfigSource | undefined>(() =>
  props.groupSource === 'built_in' ? undefined : props.groupSource,
)
</script>
