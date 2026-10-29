<template>
  <UiModal :open="open" :dismissible="!submitting" aria-label="Move task" size="md" @close="emit('close')">
    <form class="task-panel-dialog__form task-move" data-testid="task-move-dialog" @submit.prevent="submit">
      <header class="task-panel-dialog__header">
        <h2>Move {{ taskId }}</h2>
        <UiButton
          variant="ghost"
          icon-only
          type="button"
          :disabled="submitting"
          aria-label="Close dialog"
          title="Close dialog"
          @click="emit('close')"
        >
          <IconGlyph name="close" />
        </UiButton>
      </header>

      <label class="task-panel-dialog__field">
        <span class="muted">To project</span>
        <select v-model="target" class="input" data-testid="task-move-project">
          <option v-for="project in targets" :key="project.prefix" :value="project.prefix">
            {{ projectLabel(project) }}
          </option>
        </select>
      </label>

      <div class="task-move__fields">
        <label class="task-panel-dialog__field">
          <span class="muted">Status</span>
          <select v-model="status" class="input" data-testid="task-move-status" :disabled="!statuses.length">
            <option v-for="value in statuses" :key="value" :value="value">{{ value }}</option>
          </select>
        </label>
        <label class="task-panel-dialog__field">
          <span class="muted">Type</span>
          <select v-model="taskType" class="input" data-testid="task-move-type" :disabled="!types.length">
            <option v-for="value in types" :key="value" :value="value">{{ value }}</option>
          </select>
        </label>
        <label class="task-panel-dialog__field">
          <span class="muted">Priority</span>
          <select v-model="priority" class="input" data-testid="task-move-priority" :disabled="!priorities.length">
            <option v-for="value in priorities" :key="value" :value="value">{{ value }}</option>
          </select>
        </label>
      </div>

      <div class="task-move__summary" aria-live="polite">
        <p v-if="previewing" class="muted">Checking…</p>
        <p v-else-if="error" class="task-move__error" role="alert" data-testid="task-move-error">{{ error }}</p>
        <template v-else-if="preview">
          <p data-testid="task-move-new-id">
            It becomes <strong>{{ preview.to }}</strong
            >. {{ taskId }} keeps working as a link to it.
          </p>
          <ul v-if="otherChanges.length" class="task-move__changes">
            <li v-for="change in otherChanges" :key="change.field">
              {{ fieldLabel(change.field) }}: {{ change.old || 'none' }} → {{ change.new || 'none' }}
            </li>
          </ul>
        </template>
      </div>

      <footer class="task-panel-dialog__footer">
        <UiButton type="button" :disabled="submitting" @click="emit('close')">Cancel</UiButton>
        <UiButton
          variant="primary"
          type="submit"
          data-testid="task-move-submit"
          :disabled="submitting || previewing || !preview || Boolean(error)"
        >
          {{ submitting ? 'Moving…' : `Move to ${target}` }}
        </UiButton>
      </footer>
    </form>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { api } from '../api/client'
import type { TaskMoveRequest, TaskMoveResponse } from '../api/types'
import { formatProjectLabel } from '../utils/projectLabels'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiModal from './UiModal.vue'

/**
 * DEV-183: move a task to another project. Shows the new ID and what changes
 * to fit the target project (a dry run), and lets the user pick status, type
 * and priority from the target's own values before moving.
 */
const props = defineProps<{
  open: boolean
  taskId: string
  /** Project chosen in the panel; the dialog can change it. */
  initialTarget: string
  projects: Array<{ prefix: string; name?: string | null }>
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'moved', response: TaskMoveResponse): void
}>()

const target = ref('')
const status = ref('')
const taskType = ref('')
const priority = ref('')
const statuses = ref<string[]>([])
const types = ref<string[]>([])
const priorities = ref<string[]>([])
const preview = ref<TaskMoveResponse | null>(null)
const previewing = ref(false)
const submitting = ref(false)
const error = ref('')
let generation = 0

const currentProject = computed(() => props.taskId.slice(0, props.taskId.lastIndexOf('-')))
const targets = computed(() => props.projects.filter((project) => project.prefix !== currentProject.value))
const otherChanges = computed(() => (preview.value?.changes ?? []).filter((change) => change.field !== 'moved'))

function projectLabel(project: { prefix: string; name?: string | null }) {
  return formatProjectLabel(project)
}

function fieldLabel(field: string) {
  if (field.startsWith('field:')) return field.slice('field:'.length)
  return field === 'type' ? 'Type' : field.charAt(0).toUpperCase() + field.slice(1)
}

function request(dryRun: boolean): TaskMoveRequest {
  return {
    id: props.taskId,
    project: target.value,
    status: status.value || undefined,
    task_type: taskType.value || undefined,
    priority: priority.value || undefined,
    dry_run: dryRun,
  }
}

async function loadTarget() {
  const run = ++generation
  error.value = ''
  preview.value = null
  statuses.value = []
  types.value = []
  priorities.value = []
  status.value = ''
  taskType.value = ''
  priority.value = ''
  if (!target.value) return
  previewing.value = true
  try {
    const [config, first] = await Promise.all([api.showConfig(target.value), api.moveTask(request(true))])
    if (run !== generation) return
    statuses.value = config.issue_states ?? []
    types.value = config.issue_types ?? []
    priorities.value = config.issue_priorities ?? []
    // Start from what the move would do on its own; the user can change it.
    status.value = String(first.task.status ?? '')
    taskType.value = String(first.task.task_type ?? '')
    priority.value = String(first.task.priority ?? '')
    preview.value = first
  } catch (err) {
    if (run !== generation) return
    error.value = (err as Error | undefined)?.message || 'Cannot move the task there'
  } finally {
    if (run === generation) previewing.value = false
  }
}

async function refreshPreview() {
  const run = ++generation
  previewing.value = true
  error.value = ''
  try {
    const next = await api.moveTask(request(true))
    if (run === generation) preview.value = next
  } catch (err) {
    if (run === generation) error.value = (err as Error | undefined)?.message || 'Cannot move the task there'
  } finally {
    if (run === generation) previewing.value = false
  }
}

async function submit() {
  if (submitting.value || !preview.value) return
  submitting.value = true
  error.value = ''
  try {
    const response = await api.moveTask(request(false))
    emit('moved', response)
  } catch (err) {
    error.value = (err as Error | undefined)?.message || 'Move failed'
  } finally {
    submitting.value = false
  }
}

watch(
  () => [props.open, props.initialTarget] as const,
  ([isOpen, initial]) => {
    if (!isOpen) return
    target.value = initial || targets.value[0]?.prefix || ''
    void loadTarget()
  },
  { immediate: true },
)

watch(target, (value, previous) => {
  if (props.open && previous && value !== previous) void loadTarget()
})

watch([status, taskType, priority], ([nextStatus, nextType, nextPriority]) => {
  // Only user choices re-run the preview: values that already match it
  // were set from it.
  const shown = preview.value?.task
  if (!props.open || previewing.value || !shown) return
  if (nextStatus === shown.status && nextType === shown.task_type && nextPriority === shown.priority) return
  void refreshPreview()
})
</script>

<style scoped>
.task-move__fields {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-2, 0.5rem);
}

.task-move__summary {
  min-height: 3rem;
}

.task-move__summary p {
  margin: 0 0 var(--space-2, 0.5rem);
}

.task-move__changes {
  margin: 0;
  padding-left: 1.25rem;
  color: var(--color-muted);
}

.task-move__error {
  color: var(--color-danger-strong, var(--color-danger));
}

@media (max-width: 520px) {
  .task-move__fields {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
