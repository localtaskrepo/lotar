<template>
  <ConfigGroup title="Branch aliases" :description="description">
    <ConfigAliasSection
      title="Type aliases"
      v-model:entries="typeEntries"
      :error="typeError"
      key-placeholder="feature"
      value-placeholder="Feature"
      add-label="Add alias"
      clear-label="Clear overrides"
      :show-clear="typeShowClear"
      :source-label="typeSourceLabel"
      :source-class="typeSourceClass"
      @add="handleAdd('branchTypeAliases')"
      @remove="handleRemove('branchTypeAliases', $event)"
      @clear="handleClear('branchTypeAliases')"
      @field-blur="handleValidate('branch_type_aliases')"
    />

    <ConfigAliasSection
      title="Status aliases"
      v-model:entries="statusEntries"
      :error="statusError"
      key-placeholder="ready"
      value-placeholder="InProgress"
      add-label="Add alias"
      clear-label="Clear overrides"
      :show-clear="statusShowClear"
      :source-label="statusSourceLabel"
      :source-class="statusSourceClass"
      @add="handleAdd('branchStatusAliases')"
      @remove="handleRemove('branchStatusAliases', $event)"
      @clear="handleClear('branchStatusAliases')"
      @field-blur="handleValidate('branch_status_aliases')"
    />

    <ConfigAliasSection
      title="Priority aliases"
      v-model:entries="priorityEntries"
      :error="priorityError"
      key-placeholder="hotfix"
      value-placeholder="High"
      add-label="Add alias"
      clear-label="Clear overrides"
      :show-clear="priorityShowClear"
      :source-label="prioritySourceLabel"
      :source-class="prioritySourceClass"
      @add="handleAdd('branchPriorityAliases')"
      @remove="handleRemove('branchPriorityAliases', $event)"
      @clear="handleClear('branchPriorityAliases')"
      @field-blur="handleValidate('branch_priority_aliases')"
    />
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import ConfigAliasSection, { type AliasEntry } from './ConfigAliasSection.vue'
import ConfigGroup from './ConfigGroup.vue'

const typeEntries = defineModel<AliasEntry[]>('typeEntries', { required: true })
const statusEntries = defineModel<AliasEntry[]>('statusEntries', { required: true })
const priorityEntries = defineModel<AliasEntry[]>('priorityEntries', { required: true })

const props = defineProps<{
  description: string
  isGlobal: boolean
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  sourceFor: (field: string) => ConfigSource | undefined
  typeError?: string | null
  statusError?: string | null
  priorityError?: string | null
}>()

type BranchAliasField = 'branchTypeAliases' | 'branchStatusAliases' | 'branchPriorityAliases'
type BranchAliasConfigField = 'branch_type_aliases' | 'branch_status_aliases' | 'branch_priority_aliases'

const emit = defineEmits<{
  (e: 'add', field: BranchAliasField): void
  (e: 'remove', field: BranchAliasField, index: number): void
  (e: 'clear', field: BranchAliasField): void
  (e: 'validate', field: BranchAliasConfigField): void
}>()

const typeError = computed(() => props.typeError ?? null)
const statusError = computed(() => props.statusError ?? null)
const priorityError = computed(() => props.priorityError ?? null)

const typeSource = computed(() => props.sourceFor('branch_type_aliases'))
const statusSource = computed(() => props.sourceFor('branch_status_aliases'))
const prioritySource = computed(() => props.sourceFor('branch_priority_aliases'))

const typeSourceLabel = computed(() => props.provenanceLabel(typeSource.value))
const statusSourceLabel = computed(() => props.provenanceLabel(statusSource.value))
const prioritySourceLabel = computed(() => props.provenanceLabel(prioritySource.value))

const typeSourceClass = computed(() => props.provenanceClass(typeSource.value))
const statusSourceClass = computed(() => props.provenanceClass(statusSource.value))
const prioritySourceClass = computed(() => props.provenanceClass(prioritySource.value))

const typeShowClear = computed(() => !props.isGlobal && typeEntries.value.length > 0)
const statusShowClear = computed(() => !props.isGlobal && statusEntries.value.length > 0)
const priorityShowClear = computed(() => !props.isGlobal && priorityEntries.value.length > 0)

function handleAdd(field: BranchAliasField) {
  emit('add', field)
}

function handleRemove(field: BranchAliasField, index: number) {
  emit('remove', field, index)
}

function handleClear(field: BranchAliasField) {
  emit('clear', field)
}

function handleValidate(field: BranchAliasConfigField) {
  emit('validate', field)
}
</script>
