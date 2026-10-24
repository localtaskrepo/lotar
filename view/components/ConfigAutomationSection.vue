<template>
  <ConfigGroup title="Automation" :description="description" :source="sanitizedGroupSource">
    <div class="toggle-grid">
      <ConfigToggleField
        label="Auto-set reporter"
        v-model="autoSetReporter"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoSetReporter')"
        :hint="hint('autoSetReporter')"
        :source-label="provenanceLabel(sourceFor('auto_set_reporter'))"
        :source-class="provenanceClass(sourceFor('auto_set_reporter'))"
      />
      <ConfigToggleField
        label="Auto-assign on status"
        v-model="autoAssignOnStatus"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoAssignOnStatus')"
        :hint="hint('autoAssignOnStatus')"
        :source-label="provenanceLabel(sourceFor('auto_assign_on_status'))"
        :source-class="provenanceClass(sourceFor('auto_assign_on_status'))"
      />
      <ConfigToggleField
        label="Auto CODEOWNERS assignee"
        v-model="autoCodeownersAssign"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoCodeownersAssign')"
        :hint="hint('autoCodeownersAssign')"
        :source-label="provenanceLabel(sourceFor('auto_codeowners_assign'))"
        :source-class="provenanceClass(sourceFor('auto_codeowners_assign'))"
      />
      <ConfigToggleField
        label="Auto tags from path"
        v-model="autoTagsFromPath"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoTagsFromPath')"
        :hint="hint('autoTagsFromPath')"
        :source-label="provenanceLabel(sourceFor('auto_tags_from_path'))"
        :source-class="provenanceClass(sourceFor('auto_tags_from_path'))"
      />
      <ConfigToggleField
        label="Infer type from branch"
        v-model="autoBranchInferType"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoBranchInferType')"
        :hint="hint('autoBranchInferType')"
        :source-label="provenanceLabel(sourceFor('auto_branch_infer_type'))"
        :source-class="provenanceClass(sourceFor('auto_branch_infer_type'))"
      />
      <ConfigToggleField
        label="Infer status from branch"
        v-model="autoBranchInferStatus"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoBranchInferStatus')"
        :hint="hint('autoBranchInferStatus')"
        :source-label="provenanceLabel(sourceFor('auto_branch_infer_status'))"
        :source-class="provenanceClass(sourceFor('auto_branch_infer_status'))"
      />
      <ConfigToggleField
        label="Infer priority from branch"
        v-model="autoBranchInferPriority"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoBranchInferPriority')"
        :hint="hint('autoBranchInferPriority')"
        :source-label="provenanceLabel(sourceFor('auto_branch_infer_priority'))"
        :source-class="provenanceClass(sourceFor('auto_branch_infer_priority'))"
      />
      <ConfigToggleField
        label="Auto identity resolution"
        v-model="autoIdentity"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoIdentity')"
        :hint="hint('autoIdentity')"
        :source-label="provenanceLabel(sourceFor('auto_identity'))"
        :source-class="provenanceClass(sourceFor('auto_identity'))"
      />
      <ConfigToggleField
        label="Use git identity fallback"
        v-model="autoIdentityGit"
        :is-global="isGlobal"
        :options="toggleSelectOptions('autoIdentityGit')"
        :hint="hint('autoIdentityGit')"
        :source-label="provenanceLabel(sourceFor('auto_identity_git'))"
        :source-class="provenanceClass(sourceFor('auto_identity_git'))"
      />
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import type { ToggleField, ToggleValue } from '../composables/useConfigForm'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'

type ToggleOption = { value: ToggleValue; label: string }

const autoSetReporter = defineModel<ToggleValue>('autoSetReporter', { required: true })
const autoAssignOnStatus = defineModel<ToggleValue>('autoAssignOnStatus', { required: true })
const autoCodeownersAssign = defineModel<ToggleValue>('autoCodeownersAssign', { required: true })
const autoTagsFromPath = defineModel<ToggleValue>('autoTagsFromPath', { required: true })
const autoBranchInferType = defineModel<ToggleValue>('autoBranchInferType', { required: true })
const autoBranchInferStatus = defineModel<ToggleValue>('autoBranchInferStatus', { required: true })
const autoBranchInferPriority = defineModel<ToggleValue>('autoBranchInferPriority', { required: true })
const autoIdentity = defineModel<ToggleValue>('autoIdentity', { required: true })
const autoIdentityGit = defineModel<ToggleValue>('autoIdentityGit', { required: true })

const props = defineProps<{
  description: string
  groupSource?: ConfigSource
  isGlobal: boolean
  toggleSelectOptions: (field: ToggleField) => ToggleOption[]
  globalToggleSummary: (field: ToggleField) => string
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  sourceFor: (field: string) => ConfigSource | undefined
}>()

const sanitizedGroupSource = computed<ConfigSource | undefined>(() =>
  props.groupSource === 'built_in' ? undefined : props.groupSource,
)

function hint(field: ToggleField) {
  if (props.isGlobal) return ''
  const summary = props.globalToggleSummary(field)
  return `Global: ${summary || 'unknown'}`
}
</script>

<style scoped>
.toggle-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}
</style>
