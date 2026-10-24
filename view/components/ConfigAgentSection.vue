<template>
  <ConfigGroup title="Agent jobs" :description="description">
    <div class="field-grid" data-testid="config-agent">
      <ConfigToggleField
        label="Keep context files"
        v-model="contextEnabled"
        :is-global="isGlobal"
        enabled-label="Store per-ticket context for resume"
        :options="toggleSelectOptions('agentContextEnabled')"
        :hint="toggleHint('agentContextEnabled')"
        :source-label="provenanceLabel(sourceFor('agent_context_enabled'))"
        :source-class="provenanceClass(sourceFor('agent_context_enabled'))"
      />
      <div class="field">
        <label class="field-label" :for="`${uid}-extension`">Context file extension</label>
        <UiInput
          :id="`${uid}-extension`"
          v-model="contextExtension"
          maxlength="64"
          :placeholder="isGlobal ? '.context' : inheritPlaceholder(globalContextExtension)"
          @blur="emit('validate', 'agent_context_extension')"
        />
        <p v-if="errors.agent_context_extension" class="field-error">{{ errors.agent_context_extension }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label" :for="`${uid}-logs`">Job logs folder</label>
        <UiInput
          :id="`${uid}-logs`"
          v-model="logsDir"
          maxlength="512"
          :placeholder="isGlobal ? 'Not persisted (in-memory only)' : inheritPlaceholder(globalLogsDir)"
        />
        <p class="field-hint">When set, job events are also written to &lt;folder&gt;/&lt;JOB_ID&gt;.jsonl so status and logs survive restarts.</p>
      </div>

      <div class="field field--full">
        <label class="field-label" :for="`${uid}-instructions-mode`">
          <span>Instructions</span>
          <span v-if="sourceFor('agent_instructions')" :class="['provenance', provenanceClass(sourceFor('agent_instructions'))]">
            {{ provenanceLabel(sourceFor('agent_instructions')) }}
          </span>
        </label>
        <UiSelect :id="`${uid}-instructions-mode`" v-model="instructionsMode" @change="emit('validate', 'agent_instructions')">
          <option v-if="!isGlobal" value="inherit">Inherit global</option>
          <option value="builtin">Built-in instructions</option>
          <option value="inline">Custom text</option>
          <option value="file">Instructions file</option>
        </UiSelect>
        <textarea
          v-if="instructionsMode === 'inline'"
          v-model="instructions"
          class="input instructions-text"
          rows="4"
          aria-label="Instructions text"
          placeholder="Follow the project's checked-in development guidance."
          @blur="emit('validate', 'agent_instructions')"
        ></textarea>
        <UiInput
          v-else-if="instructionsMode === 'file'"
          v-model="instructionsFile"
          maxlength="512"
          aria-label="Instructions file"
          placeholder="docs/agent-instructions.md"
          @blur="emit('validate', 'agent_instructions')"
        />
        <p class="field-hint">Prepended to agent prompts. File paths may be absolute or relative to the tasks directory.</p>
        <p v-if="errors.agent_instructions" class="field-error">{{ errors.agent_instructions }}</p>
      </div>

      <ConfigToggleField
        label="Run jobs in git worktrees"
        v-model="worktreeEnabled"
        :is-global="isGlobal"
        enabled-label="Isolate each job in a worktree"
        :options="toggleSelectOptions('agentWorktreeEnabled')"
        :hint="toggleHint('agentWorktreeEnabled')"
        :source-label="provenanceLabel(sourceFor('agent_worktree_enabled'))"
        :source-class="provenanceClass(sourceFor('agent_worktree_enabled'))"
      />
      <div class="field">
        <label class="field-label" :for="`${uid}-prefix`">Branch prefix</label>
        <UiInput
          :id="`${uid}-prefix`"
          v-model="worktreeBranchPrefix"
          maxlength="100"
          :placeholder="isGlobal ? 'agent/' : inheritPlaceholder(globalBranchPrefix)"
          @blur="emit('validate', 'agent_worktree_branch_prefix')"
        />
        <p v-if="errors.agent_worktree_branch_prefix" class="field-error">{{ errors.agent_worktree_branch_prefix }}</p>
      </div>
      <div class="field field--full">
        <label class="field-label" :for="`${uid}-worktree-dir`">Worktree folder</label>
        <UiInput
          :id="`${uid}-worktree-dir`"
          v-model="worktreeDir"
          maxlength="512"
          :placeholder="isGlobal ? 'Computed next to the repository' : inheritPlaceholder(globalWorktreeDir)"
        />
        <p class="field-hint">Relative paths resolve from the repository's parent folder.</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { useId } from 'vue'
import type { ConfigSource } from '../api/types'
import type { AgentInstructionsMode, ToggleField, ToggleValue } from '../composables/useConfigForm'
import ConfigGroup from './ConfigGroup.vue'
import ConfigToggleField from './ConfigToggleField.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

const contextEnabled = defineModel<ToggleValue>('contextEnabled', { required: true })
const contextExtension = defineModel<string>('contextExtension', { required: true })
const logsDir = defineModel<string>('logsDir', { required: true })
const instructionsMode = defineModel<AgentInstructionsMode>('instructionsMode', { required: true })
const instructions = defineModel<string>('instructions', { required: true })
const instructionsFile = defineModel<string>('instructionsFile', { required: true })
const worktreeEnabled = defineModel<ToggleValue>('worktreeEnabled', { required: true })
const worktreeDir = defineModel<string>('worktreeDir', { required: true })
const worktreeBranchPrefix = defineModel<string>('worktreeBranchPrefix', { required: true })

const {
  description,
  isGlobal,
  errors,
  globalContextExtension = '',
  globalLogsDir = '',
  globalBranchPrefix = '',
  globalWorktreeDir = '',
  sourceFor,
  toggleSelectOptions,
  globalToggleSummary,
  provenanceLabel,
  provenanceClass,
} = defineProps<{
  description: string
  isGlobal: boolean
  errors: Record<string, string | null>
  globalContextExtension?: string
  globalLogsDir?: string
  globalBranchPrefix?: string
  globalWorktreeDir?: string
  sourceFor: (field: string) => ConfigSource | undefined
  toggleSelectOptions: (field: ToggleField) => Array<{ value: ToggleValue; label: string }>
  globalToggleSummary: (field: ToggleField) => string
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (e: 'validate', field: 'agent_context_extension' | 'agent_instructions' | 'agent_worktree_branch_prefix'): void
}>()

const uid = useId()

function inheritPlaceholder(value: string) {
  return value ? `Inherit global (${value})` : 'Inherit global'
}

function toggleHint(field: ToggleField) {
  return isGlobal ? '' : `Global: ${globalToggleSummary(field) || 'unknown'}`
}
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
  box-sizing: border-box;
}

.field :deep(input.input) {
  height: var(--config-control-height, 40px);
}

.instructions-text {
  width: 100%;
  min-height: 6rem;
  resize: vertical;
  font: inherit;
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
