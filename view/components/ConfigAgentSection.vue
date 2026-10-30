<template>
  <ConfigGroup title="Agent jobs" :description="description">
    <div class="field-grid" data-testid="config-agent">
      <h3 class="cfg-group-heading field--full">Context</h3>
      <div class="cfg-toggles">
        <ConfigToggleField
          label="Keep context files"
          v-model="contextEnabled"
          :is-global="isGlobal"
          description="Store a per-task context file so agents can resume earlier work."
          :options="toggleSelectOptions('agentContextEnabled')"
          :source-label="provenanceLabel(sourceFor('agent_context_enabled'))"
          :source-class="provenanceClass(sourceFor('agent_context_enabled'))"
        />
      </div>
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

      <h3 class="cfg-group-heading field--full">Instructions</h3>
      <div class="field">
        <label class="field-label" :for="`${uid}-instructions-mode`">
          <span>Source</span>
          <span
            v-if="sourceFor('agent_instructions')"
            :class="['provenance', provenanceClass(sourceFor('agent_instructions'))]"
          >
            {{ provenanceLabel(sourceFor('agent_instructions')) }}
          </span>
        </label>
        <UiSelect
          :id="`${uid}-instructions-mode`"
          v-model="instructionsMode"
          @change="emit('validate', 'agent_instructions')"
        >
          <option v-if="!isGlobal" value="inherit">Inherit global</option>
          <option value="builtin">Built-in instructions</option>
          <option value="inline">Custom text</option>
          <option value="file">Instructions file</option>
        </UiSelect>
      </div>
      <div v-if="instructionsMode === 'inline'" class="field field--full">
        <label class="field-label" :for="`${uid}-instructions-text`">Instructions text</label>
        <textarea
          :id="`${uid}-instructions-text`"
          v-model="instructions"
          class="input"
          rows="4"
          placeholder="Follow the project's checked-in development guidance."
          @blur="emit('validate', 'agent_instructions')"
        ></textarea>
      </div>
      <div v-else-if="instructionsMode === 'file'" class="field field--full">
        <label class="field-label" :for="`${uid}-instructions-file`">Instructions file</label>
        <UiInput
          :id="`${uid}-instructions-file`"
          v-model="instructionsFile"
          maxlength="512"
          placeholder="docs/agent-instructions.md"
          @blur="emit('validate', 'agent_instructions')"
        />
      </div>
      <p class="field-hint field--full">
        Prepended to every agent prompt. File paths may be absolute or relative to the tasks directory.
      </p>
      <p v-if="errors.agent_instructions" class="field-error field--full">{{ errors.agent_instructions }}</p>

      <h3 class="cfg-group-heading field--full">Logs</h3>
      <div class="field field--full">
        <label class="field-label" :for="`${uid}-logs`">Job logs folder</label>
        <UiInput
          :id="`${uid}-logs`"
          v-model="logsDir"
          maxlength="512"
          :placeholder="
            isGlobal ? '<tasks dir>/@agent-logs' : inheritPlaceholder(globalLogsDir || '<tasks dir>/@agent-logs')
          "
        />
        <p class="field-hint">
          Job events are written to &lt;folder&gt;/&lt;JOB_ID&gt;.jsonl so status and logs survive restarts. Leave empty
          for &lt;tasks dir&gt;/@agent-logs, or enter <code>off</code> to turn logging off.
        </p>
      </div>

      <h3 class="cfg-group-heading field--full">Worktrees</h3>
      <div class="cfg-toggles">
        <ConfigToggleField
          label="Run jobs in git worktrees"
          v-model="worktreeEnabled"
          :is-global="isGlobal"
          description="Give each agent job its own worktree and branch."
          :options="toggleSelectOptions('agentWorktreeEnabled')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_enabled'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_enabled'))"
        />
      </div>
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
          :placeholder="isGlobal ? 'Next to the repository' : inheritPlaceholder(globalWorktreeDir)"
        />
        <p class="field-hint">Relative paths resolve from the repository's parent folder.</p>
      </div>

      <h3 class="cfg-group-heading field--full">Running jobs</h3>
      <div class="field">
        <label class="field-label" :for="`${uid}-max-jobs`">
          <span>Jobs at once</span>
          <span
            v-if="sourceFor('agent_worktree_max_parallel_jobs')"
            :class="['provenance', provenanceClass(sourceFor('agent_worktree_max_parallel_jobs'))]"
          >
            {{ provenanceLabel(sourceFor('agent_worktree_max_parallel_jobs')) }}
          </span>
        </label>
        <UiInput
          :id="`${uid}-max-jobs`"
          v-model="worktreeMaxParallelJobs"
          inputmode="numeric"
          maxlength="6"
          data-testid="config-agent-max-jobs"
          :placeholder="maxJobsPlaceholder"
          @blur="emit('validate', 'agent_worktree_max_parallel_jobs')"
        />
        <p class="field-hint">Further jobs wait in the queue. 0 pauses new jobs; leave empty for no limit.</p>
        <p v-if="errors.agent_worktree_max_parallel_jobs" class="field-error">
          {{ errors.agent_worktree_max_parallel_jobs }}
        </p>
      </div>

      <h3 class="cfg-group-heading field--full">Clean up worktrees</h3>
      <div class="cfg-toggles" data-testid="config-agent-cleanup">
        <ConfigToggleField
          label="When the task is done"
          v-model="worktreeCleanupOnDone"
          :is-global="isGlobal"
          description="Remove the worktree after a successful job once its task is in a done state."
          :options="toggleSelectOptions('agentWorktreeCleanupOnDone')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_cleanup_on_done'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_cleanup_on_done'))"
        />
        <ConfigToggleField
          label="After a failed job"
          v-model="worktreeCleanupOnFailure"
          :is-global="isGlobal"
          description="Remove the worktree when a job fails."
          :options="toggleSelectOptions('agentWorktreeCleanupOnFailure')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_cleanup_on_failure'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_cleanup_on_failure'))"
        />
        <ConfigToggleField
          label="After a cancelled job"
          v-model="worktreeCleanupOnCancel"
          :is-global="isGlobal"
          description="Remove the worktree when a job is cancelled."
          :options="toggleSelectOptions('agentWorktreeCleanupOnCancel')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_cleanup_on_cancel'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_cleanup_on_cancel'))"
        />
        <ConfigToggleField
          label="Delete the branch too"
          v-model="worktreeCleanupDeleteBranches"
          :is-global="isGlobal"
          description="Also delete the job's branch when its worktree is removed."
          :options="toggleSelectOptions('agentWorktreeCleanupDeleteBranches')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_cleanup_delete_branches'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_cleanup_delete_branches'))"
        />
        <ConfigToggleField
          label="Ignore build output"
          v-model="worktreeCleanupIgnoredFiles"
          :is-global="isGlobal"
          description="Git-ignored files (build and test output) don't keep a worktree. Untracked and changed files always do."
          :options="toggleSelectOptions('agentWorktreeCleanupIgnoredFiles')"
          :source-label="provenanceLabel(sourceFor('agent_worktree_cleanup_ignored_files'))"
          :source-class="provenanceClass(sourceFor('agent_worktree_cleanup_ignored_files'))"
        />
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed, useId } from 'vue'
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
const worktreeMaxParallelJobs = defineModel<string>('worktreeMaxParallelJobs', { default: '' })
const worktreeCleanupOnDone = defineModel<ToggleValue>('worktreeCleanupOnDone', { default: 'false' })
const worktreeCleanupOnFailure = defineModel<ToggleValue>('worktreeCleanupOnFailure', { default: 'false' })
const worktreeCleanupOnCancel = defineModel<ToggleValue>('worktreeCleanupOnCancel', { default: 'false' })
const worktreeCleanupDeleteBranches = defineModel<ToggleValue>('worktreeCleanupDeleteBranches', {
  default: 'false',
})
const worktreeCleanupIgnoredFiles = defineModel<ToggleValue>('worktreeCleanupIgnoredFiles', { default: 'true' })

const {
  description,
  isGlobal,
  errors,
  globalContextExtension = '',
  globalLogsDir = '',
  globalBranchPrefix = '',
  globalWorktreeDir = '',
  globalMaxParallelJobs = null,
  sourceFor,
  toggleSelectOptions,
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
  globalMaxParallelJobs?: number | null
  sourceFor: (field: string) => ConfigSource | undefined
  toggleSelectOptions: (field: ToggleField) => Array<{ value: ToggleValue; label: string }>
  /** Unused since the inherit option states the global value; kept for callers. */
  globalToggleSummary?: (field: ToggleField) => string
  provenanceLabel: (source: ConfigSource | undefined) => string
  provenanceClass: (source: ConfigSource | undefined) => string
}>()

const emit = defineEmits<{
  (
    e: 'validate',
    field:
      | 'agent_context_extension'
      | 'agent_instructions'
      | 'agent_worktree_branch_prefix'
      | 'agent_worktree_max_parallel_jobs',
  ): void
}>()

const uid = useId()

const maxJobsPlaceholder = computed(() => {
  if (isGlobal) return 'No limit'
  return globalMaxParallelJobs === null || globalMaxParallelJobs === undefined
    ? 'Inherit global (no limit)'
    : `Inherit global (${globalMaxParallelJobs})`
})

function inheritPlaceholder(value: string) {
  return value ? `Inherit global (${value})` : 'Inherit global'
}
</script>
