<template>
  <ConfigGroup title="People" :description="description">
    <div class="field-grid">
      <div class="field field--full" data-testid="config-members">
        <label class="field-label">
          <span>Members</span>
          <span v-if="sourceFor('members')" :class="['provenance', provenanceClass(sourceFor('members'))]">
            {{ provenanceLabel(sourceFor('members')) }}
          </span>
        </label>
        <ChipListField
          v-model="members"
          placeholder="alice"
          add-label="Add member"
          composer-label="Member"
          :empty-label="isGlobal ? 'No members listed' : `Inherit global (${globalMembersSummary})`"
          @update:modelValue="emit('validate', 'members')"
        />
        <p class="field-hint">
          Reporters and assignees allowed when "Only allow members" is on{{ isGlobal ? '' : '. Leave empty to inherit the global list' }}.
        </p>
        <p v-if="membersError" class="field-error">{{ membersError }}</p>
      </div>

      <ConfigToggleField
        label="Only allow members"
        v-model="strictMembers"
        :is-global="isGlobal"
        enabled-label="Reject reporters/assignees not in Members"
        :options="toggleSelectOptions('strictMembers')"
        :hint="toggleHint('strictMembers')"
        :source-label="provenanceLabel(sourceFor('strict_members'))"
        :source-class="provenanceClass(sourceFor('strict_members'))"
      />
      <ConfigToggleField
        label="Auto-add new members"
        v-model="autoPopulateMembers"
        :is-global="isGlobal"
        enabled-label="Add new reporters/assignees (strict mode)"
        :options="toggleSelectOptions('autoPopulateMembers')"
        :hint="toggleHint('autoPopulateMembers')"
        :source-label="provenanceLabel(sourceFor('auto_populate_members'))"
        :source-class="provenanceClass(sourceFor('auto_populate_members'))"
      />

      <div v-if="!isGlobal" class="field field--full" data-testid="config-reviewers">
        <label class="field-label">
          <span>Reviewers</span>
          <span v-if="sourceFor('review_reviewers')" :class="['provenance', provenanceClass(sourceFor('review_reviewers'))]">
            {{ provenanceLabel(sourceFor('review_reviewers')) }}
          </span>
        </label>
        <ChipListField
          v-model="reviewReviewers"
          :suggestions="members"
          placeholder="alice"
          add-label="Add reviewer"
          composer-label="Reviewer"
          empty-label="Reporter only"
          @update:modelValue="emit('validate', 'review_reviewers')"
        />
        <p class="field-hint">Named people (not @me or *) who may resolve status or assignee changes on tasks in review, besides the reporter. Leave empty for reporter-only review.</p>
        <p v-if="reviewReviewersError" class="field-error">{{ reviewReviewersError }}</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ConfigSource } from '../api/types'
import type { ToggleField, ToggleValue } from '../composables/useConfigForm'
import ChipListField from './ChipListField.vue'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'

const members = defineModel<string[]>('members', { required: true })
const strictMembers = defineModel<ToggleValue>('strictMembers', { required: true })
const autoPopulateMembers = defineModel<ToggleValue>('autoPopulateMembers', { required: true })
const reviewReviewers = defineModel<string[]>('reviewReviewers', { required: true })
const {
  description,
  isGlobal,
  provenanceLabel,
  provenanceClass,
  membersError = null,
  reviewReviewersError = null,
  globalMembers = [],
  sourceFor,
  toggleSelectOptions,
  globalToggleSummary,
} = defineProps<{
  description: string
  isGlobal: boolean
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
  membersError?: string | null
  reviewReviewersError?: string | null
  globalMembers?: string[]
  sourceFor: (field: string) => ConfigSource | undefined
  toggleSelectOptions: (field: ToggleField) => Array<{ value: ToggleValue; label: string }>
  globalToggleSummary: (field: ToggleField) => string
}>()

const globalMembersSummary = computed(() => (globalMembers.length ? globalMembers.join(', ') : 'none'))

function toggleHint(field: ToggleField) {
  return isGlobal ? '' : `Global: ${globalToggleSummary(field) || 'unknown'}`
}

const emit = defineEmits<{
  (e: 'validate', field: 'members' | 'review_reviewers'): void
}>()
</script>

<style scoped>
.field-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}

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

.field--full {
  grid-column: 1 / -1;
}

.field :deep(.input) {
  height: var(--config-control-height, 40px);
  padding: calc(var(--space-2) - 4px) var(--space-3);
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
