<template>
  <Teleport to="body">
    <Transition name="task-panel">
      <div v-if="open" class="task-panel__overlay" @click.self="closePanel">
        <aside
          class="task-panel"
          role="dialog"
          aria-modal="true"
          :aria-label="mode === 'create' ? 'Create task' : `Task ${form.id || ''}`"
        >
          <header class="task-panel__header">
            <div class="task-panel__title">
              <span v-if="mode === 'edit'" class="task-panel__id">{{ form.id }}</span>
              <h2>{{ mode === 'create' ? 'Create task' : form.title || 'Task details' }}</h2>
            </div>
            <div class="task-panel__header-actions">
              <span v-if="mode === 'edit'" class="badge" :class="statusBadgeClass">{{ form.status }}</span>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close panel"
                title="Close panel"
                @click="closePanel"
              >
                <IconGlyph name="close" />
              </UiButton>
            </div>
          </header>

          <section class="task-panel__body" v-if="!loading">
            <form class="task-panel__form" @submit.prevent="handleSubmit">
              <TaskPanelSummarySection
                :form="form"
                :mode="mode"
                :projects="projects"
                :project-label="projectLabel"
                :types="typeOptions"
                :statuses="statusOptions"
                :priorities="priorityOptions"
                @fieldBlur="handleFieldBlur"
                @projectChange="onProjectChange"
                @updateStatus="updateStatus"
              />

              <fieldset class="task-panel__group">
                <legend>Details</legend>
                <textarea
                  v-model="form.description"
                  rows="6"
                  placeholder="Description"
                  @blur="() => onFieldBlur('description')"
                ></textarea>
                <TaskPanelTagEditor
                  :tags="form.tags"
                  :configured-tags="configTagOptions"
                  :known-tags="knownTags"
                  :allow-custom-tags="allowCustomTags"
                  @update:tags="setTags"
                  @discoveredTags="mergeKnownTags"
                />
              </fieldset>

              <TaskPanelOwnershipSection
                :form="form"
                :ordered-known-users="orderedKnownUsers"
                :whoami="whoami"
                :reporter-selection="reporterSelection"
                :assignee-selection="assigneeSelection"
                :reporter-mode="reporterMode"
                :assignee-mode="assigneeMode"
                :reporter-custom="reporterCustom"
                :assignee-custom="assigneeCustom"
                @update:reporterSelection="setReporterSelection"
                @update:assigneeSelection="setAssigneeSelection"
                @update:reporterCustom="setReporterCustom"
                @update:assigneeCustom="setAssigneeCustom"
                @commitReporterCustom="commitReporterCustom"
                @commitAssigneeCustom="commitAssigneeCustom"
                @resetReporterSelection="resetReporterSelection"
                @resetAssigneeSelection="resetAssigneeSelection"
                @fieldBlur="handleFieldBlur"
              />

              <fieldset v-if="sprintsLoading || hasSprints" class="task-panel__group">
                <legend>Sprints</legend>
                <div v-if="sprintsLoading" class="task-panel__sprints-loading">
                  <UiLoader size="sm">Loading sprint info…</UiLoader>
                </div>
                <div v-else class="task-panel__sprint-area">
                  <ChipListField
                    class="task-panel__sprint-field"
                    :model-value="sprintChipLabels"
                    empty-label="Not assigned to a sprint."
                    add-label="Assign sprint"
                    add-behavior="external"
                    :removable="false"
                    :chip-class="null"
                    @add-click="openSprintDialog('add')"
                  >
                    <template #chip="{ index }">
                      <span
                        v-if="assignedSprints[index]"
                        :class="[
                          'task-panel__sprint-chip',
                          `task-panel__sprint-chip--${assignedSprints[index].state}`,
                          { 'task-panel__sprint-chip--missing': assignedSprints[index].missing },
                        ]"
                      >
                        {{ assignedSprints[index].label }}
                        <button
                          type="button"
                          class="task-panel__sprint-chip-remove"
                          :aria-label="`Remove ${assignedSprints[index].label}`"
                          :title="`Remove ${assignedSprints[index].label}`"
                          :disabled="removingSprintId === assignedSprints[index].id"
                          @click.stop="removeSprintChip(assignedSprints[index].id)"
                        >
                          <IconGlyph name="close" />
                        </button>
                      </span>
                    </template>
                  </ChipListField>
                  <p v-if="assignedSprintNotice" class="task-panel__sprint-warning">
                    {{ assignedSprintNotice }}
                  </p>
                </div>
              </fieldset>

              <TaskPanelCustomFieldsSection
                :custom-fields="customFields"
                :custom-field-keys="customFieldKeys"
                :new-field-key="newField.key"
                :new-field-value="newField.value"
                @updateCustomFieldKey="updateCustomFieldKey"
                @updateCustomFieldValue="updateCustomFieldValue"
                @updateNewFieldKey="updateNewFieldKey"
                @updateNewFieldValue="updateNewFieldValue"
                @addField="addField"
                @removeField="removeField"
                @commit="commitCustomFields"
              />

              <section class="task-panel__group task-panel__activity">
                <div class="task-panel__tabs" role="tablist">
                  <button
                    v-for="tab in activityTabs"
                    :key="tab.id"
                    type="button"
                    class="task-panel__tab"
                    :class="{ 'task-panel__tab--active': activityTab === tab.id }"
                    role="tab"
                    :aria-selected="activityTab === tab.id"
                    @click="activityTab = tab.id"
                  >
                    {{ tab.label }}
                  </button>
                </div>

                <TaskPanelCommentsTab
                  v-if="activityTab === 'comments'"
                  :mode="mode"
                  :task="task"
                  :new-comment="newComment"
                  :submitting="submitting"
                  :editing-comment-index="editingCommentIndex"
                  :editing-comment-text="editingCommentText"
                  :editing-comment-submitting="editingCommentSubmitting"
                  :format-date="formatDate"
                  :set-editing-textarea="setEditingCommentTextarea"
                  @reload="reloadTask"
                  @startEdit="startEditComment"
                  @saveEdit="saveCommentEdit"
                  @cancelEdit="cancelEditComment"
                  @addComment="addComment"
                  @update:newComment="updateNewComment"
                  @update:editingCommentText="updateEditingCommentText"
                />

                <TaskPanelRelationshipsTab
                  v-else-if="activityTab === 'relationships'"
                  :mode="mode"
                  :relation-defs="relationDefs"
                  :relationships="relationships"
                  :relation-suggestions="relationSuggestions"
                  :relation-active-index="relationActiveIndex"
                  :on-relation-input="onRelationInput"
                  :on-relation-key="onRelationKey"
                  :on-relation-blur="handleRelationshipBlur"
                  :on-pick-relation="pickRelation"
                  @reload="reloadTask"
                  @update:relationship="updateRelationshipField"
                />

                <TaskPanelHistoryTab
                  v-else-if="activityTab === 'history'"
                  :mode="mode"
                  :change-log="changeLog"
                  :format-date="formatDate"
                  :format-field-name="formatFieldName"
                  :format-change-value="formatChangeValue"
                  @reload="reloadTask"
                />

                <TaskPanelCommitsTab
                  v-else-if="activityTab === 'commits'"
                  :mode="mode"
                  :commit-history="commitHistory"
                  :commits-loading="commitsLoading"
                  :format-commit="formatCommit"
                  :format-date="formatDate"
                  @refresh="refreshCommits"
                />

                <TaskPanelReferencesTab
                  v-else
                  :mode="mode"
                  :task="task"
                  :hovered-reference-code="hoveredReferenceCode"
                  :hovered-reference-style="hoveredReferenceStyle"
                  :hovered-reference-loading="hoveredReferenceLoading"
                  :hovered-reference-error="hoveredReferenceError"
                  :hovered-reference-snippet="hoveredReferenceSnippet"
                  :hovered-reference-can-expand="hoveredReferenceCanExpand"
                  :hovered-reference-can-expand-before="hoveredReferenceCanExpandBefore"
                  :hovered-reference-can-expand-after="hoveredReferenceCanExpandAfter"
                  :on-reference-enter="onReferenceEnter"
                  :on-reference-leave="onReferenceLeave"
                  :on-reference-preview-enter="onReferencePreviewEnter"
                  :on-reference-preview-leave="onReferencePreviewLeave"
                  :expand-reference-snippet="expandReferenceSnippet"
                  :is-reference-line-highlighted="isReferenceLineHighlighted"
                  :set-reference-preview-element="setReferencePreviewElement"
                  @reload="reloadTask"
                />
              </section>

              <footer class="task-panel__footer">
                <div v-if="Object.keys(errors).length" class="task-panel__errors">
                  <p v-for="(message, field) in errors" :key="field">{{ message }}</p>
                </div>
                <UiButton v-if="mode === 'create'" variant="primary" type="submit" :disabled="submitting || !(form.project || '').trim()">
                  {{ submitting ? 'Creating…' : 'Create task' }}
                </UiButton>
              </footer>
            </form>
          </section>

          <section v-else class="task-panel__loading">
            <UiLoader size="md">Loading task…</UiLoader>
          </section>
        </aside>
      </div>
    </Transition>
  </Teleport>
  <Teleport to="body">
    <div
      v-if="sprintDialogOpen"
      class="task-panel-dialog__overlay"
      role="dialog"
      aria-modal="true"
      :aria-label="sprintDialogTitle"
      @click.self="closeSprintDialog"
    >
      <UiCard class="task-panel-dialog__card">
        <form class="task-panel-dialog__form" @submit.prevent="submitSprintDialog">
          <header class="task-panel-dialog__header">
            <h2>{{ sprintDialogTitle }}</h2>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="sprintDialogSubmitting"
              aria-label="Close dialog"
              title="Close dialog"
              @click="closeSprintDialog"
            >
              <IconGlyph name="close" />
            </UiButton>
          </header>
          <label class="task-panel-dialog__field">
            <span class="muted">Sprint</span>
            <select
              class="input"
              v-model="sprintDialogSelection"
              :disabled="sprintDialogMode === 'add' && !sprintOptions.length"
            >
              <option v-for="option in sprintOptions" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
          </label>
          <p v-if="sprintDialogMode === 'add' && !sprintOptions.length" class="muted">No sprints available yet.</p>
          <label v-if="sprintDialogMode === 'add'" class="task-panel-dialog__checkbox">
            <input type="checkbox" v-model="sprintDialogAllowClosed" /> Allow assigning to closed sprints
          </label>
          <footer class="task-panel-dialog__footer">
            <UiButton
              variant="primary"
              type="submit"
              :disabled="sprintDialogSubmitting || (sprintDialogMode === 'add' && !sprintOptions.length)"
            >
              {{ sprintDialogSubmitting ? (sprintDialogMode === 'add' ? 'Assigning…' : 'Removing…') : sprintDialogTitle }}
            </UiButton>
            <UiButton variant="ghost" type="button" :disabled="sprintDialogSubmitting" @click="closeSprintDialog">
              Cancel
            </UiButton>
          </footer>
        </form>
      </UiCard>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { Teleport, Transition, computed, ref, watch } from 'vue'
import { api } from '../api/client'
import type { SprintIntegrityDiagnostics, TaskDTO } from '../api/types'
import { useTaskPanelState } from '../composables/useTaskPanelState'
import ChipListField from './ChipListField.vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiCard from './UiCard.vue'
import UiLoader from './UiLoader.vue'
import TaskPanelCommentsTab from './task-panel/TaskPanelCommentsTab.vue'
import TaskPanelCommitsTab from './task-panel/TaskPanelCommitsTab.vue'
import TaskPanelCustomFieldsSection from './task-panel/TaskPanelCustomFieldsSection.vue'
import TaskPanelHistoryTab from './task-panel/TaskPanelHistoryTab.vue'
import TaskPanelOwnershipSection from './task-panel/TaskPanelOwnershipSection.vue'
import TaskPanelReferencesTab from './task-panel/TaskPanelReferencesTab.vue'
import TaskPanelRelationshipsTab from './task-panel/TaskPanelRelationshipsTab.vue'
import TaskPanelSummarySection from './task-panel/TaskPanelSummarySection.vue'
import TaskPanelTagEditor from './task-panel/TaskPanelTagEditor.vue'
import { showToast } from './toast'

const props = defineProps<{ open: boolean; taskId?: string | null; initialProject?: string | null }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'created', task: TaskDTO): void; (e: 'updated', task: TaskDTO): void }>()

const {
  mode,
  projects,
  statusOptions,
  priorityOptions,
  typeOptions,
  loading,
  submitting,
  task,
  form,
  hoveredReferenceCode,
  hoveredReferenceStyle,
  hoveredReferenceLoading,
  hoveredReferenceError,
  hoveredReferenceSnippet,
  hoveredReferenceCanExpand,
  hoveredReferenceCanExpandBefore,
  hoveredReferenceCanExpandAfter,
  onReferenceEnter,
  onReferenceLeave,
  onReferencePreviewEnter,
  onReferencePreviewLeave,
  expandReferenceSnippet,
  isReferenceLineHighlighted,
  setReferencePreviewElement,
  errors,
  knownTags,
  allowCustomTags,
  configTagOptions,
  customFields,
  customFieldKeys,
  newField,
  activityTabs,
  activityTab,
  changeLog,
  commitHistory,
  commitsLoading,
  statusBadgeClass,
  sprintsLoading,
  assignedSprints,
  hasAssignedSprints,
  assignedSprintNotice,
  sprintOptions,
  hasSprints,
  relationDefs,
  relationships,
  relationSuggestions,
  relationActiveIndex,
  handleRelationshipBlur,
  onRelationInput,
  onRelationKey,
  pickRelation,
  updateRelationshipField,
  mergeKnownTags,
  projectLabel,
  commitCustomFields,
  addField,
  removeField,
  setTags,
  updateCustomFieldKey,
  updateCustomFieldValue,
  updateNewFieldKey,
  updateNewFieldValue,
  handleFieldBlur,
  onFieldBlur,
  closePanel,
  handleSubmit,
  updateStatus,
  reloadTask,
  formatDate,
  formatCommit,
  formatFieldName,
  formatChangeValue,
  onProjectChange,
  refreshCommits,
  newComment,
  editingCommentIndex,
  editingCommentText,
  editingCommentSubmitting,
  setEditingCommentTextarea,
  updateNewComment,
  updateEditingCommentText,
  addComment,
  startEditComment,
  cancelEditComment,
  saveCommentEdit,
  reporterMode,
  assigneeMode,
  reporterCustom,
  assigneeCustom,
  orderedKnownUsers,
  reporterSelection,
  assigneeSelection,
  setReporterSelection,
  setAssigneeSelection,
  setReporterCustom,
  setAssigneeCustom,
  commitReporterCustom,
  commitAssigneeCustom,
  resetReporterSelection,
  resetAssigneeSelection,
  whoami,
  refreshSprints,
} = useTaskPanelState(props, emit)

const sprintDialogOpen = ref(false)
const sprintDialogSubmitting = ref(false)
const sprintDialogMode = ref<'add' | 'remove'>('add')
const sprintDialogSelection = ref('active')
const sprintDialogAllowClosed = ref(false)
const removingSprintId = ref<number | null>(null)
const sprintChipLabels = computed(() => assignedSprints.value.map((entry) => entry.label))

function handleIntegrityFeedback(integrity?: SprintIntegrityDiagnostics | null) {
  if (!integrity) return
  const autoCleanup = integrity.auto_cleanup
  if (autoCleanup?.removed_references) {
    showToast(`Automatically cleaned ${autoCleanup.removed_references} dangling sprint reference${autoCleanup.removed_references === 1 ? '' : 's'}.`)
  }
  if (Array.isArray(integrity.missing_sprints) && integrity.missing_sprints.length) {
    const ids = integrity.missing_sprints.map((id) => `#${id}`).join(', ')
    showToast(`Missing sprint IDs still detected: ${ids}`)
  }
}

watch(
  sprintOptions,
  (options) => {
    if (!options.length) {
      sprintDialogSelection.value = 'active'
      return
    }
    if (!options.some((opt) => opt.value === sprintDialogSelection.value)) {
      sprintDialogSelection.value = options[0].value
    }
  },
  { immediate: true },
)

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen && sprintDialogOpen.value) {
      closeSprintDialog(true)
    }
    if (!isOpen && removingSprintId.value !== null) {
      removingSprintId.value = null
    }
  },
)

const sprintDialogTitle = computed(() =>
  sprintDialogMode.value === 'add'
    ? 'Assign to sprint'
    : 'Remove from sprint',
)

function openSprintDialog(mode: 'add' | 'remove') {
  if (!hasSprints.value && mode === 'add') {
    showToast('No sprints available yet')
    return
  }
  if (!hasAssignedSprints.value && mode === 'remove') {
    showToast('This task is not assigned to any sprint')
    return
  }
  sprintDialogMode.value = mode
  sprintDialogSelection.value = sprintOptions.value[0]?.value ?? 'active'
  sprintDialogAllowClosed.value = false
  sprintDialogOpen.value = true
}

function closeSprintDialog(force?: boolean | Event) {
  const forced = force === true
  if (sprintDialogSubmitting.value && !forced) return
  sprintDialogOpen.value = false
}

async function refreshSprintDataAfterMutation() {
  try {
    await refreshSprints(true)
  } catch (refreshError) {
    console.warn('Failed to refresh sprints', refreshError)
    showToast('Updated sprint list may be stale; refresh later.')
  }
  try {
    await reloadTask()
  } catch (reloadError) {
    console.warn('Failed to reload task after sprint update', reloadError)
    showToast('Task view may be out of date; refresh to confirm changes.')
  }
}

function parseSprintToken(token: string): number | string | undefined {
  const trimmed = (token || '').trim()
  if (!trimmed || trimmed === 'active' || trimmed === 'auto') return undefined
  if (trimmed === 'next') return 'next'
  if (trimmed === 'previous' || trimmed === 'prev') return 'previous'
  const numeric = Number(trimmed)
  if (Number.isInteger(numeric) && numeric > 0) return numeric
  return trimmed
}

async function submitSprintDialog() {
  if (sprintDialogSubmitting.value) return
  const taskId = form.id || props.taskId
  if (!taskId || taskId === 'new') {
    showToast('Save the task before managing sprints')
    return
  }
  sprintDialogSubmitting.value = true
  try {
    const payload: Record<string, unknown> = {
      tasks: [taskId],
      cleanup_missing: true,
    }
    const sprintRef = parseSprintToken(sprintDialogSelection.value)
    if (sprintRef !== undefined) payload.sprint = sprintRef
    if (sprintDialogMode.value === 'add' && sprintDialogAllowClosed.value) {
      payload.allow_closed = true
    }
    const response =
      sprintDialogMode.value === 'add'
        ? await api.sprintAdd(payload as any)
        : await api.sprintRemove(payload as any)
    const changed = response.modified.length
    if (changed) {
      const verb = sprintDialogMode.value === 'add' ? 'Assigned' : 'Removed'
      const preposition = sprintDialogMode.value === 'add' ? 'to' : 'from'
      const label = response.sprint_label || `Sprint #${response.sprint_id}`
      showToast(`${verb} ${changed} task${changed === 1 ? '' : 's'} ${preposition} ${label}`)
    } else {
      showToast(sprintDialogMode.value === 'add' ? 'No changes applied' : 'No sprints removed')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    if (messages.length) {
      messages.forEach((message) => showToast(message))
    } else if (
      sprintDialogMode.value === 'add' &&
      Array.isArray(response.replaced) &&
      response.replaced.length
    ) {
      response.replaced.forEach((entry) => {
        if (!entry?.previous?.length) return
        const prev = entry.previous.map((id) => `#${id}`).join(', ')
        showToast(`${entry.task_id} moved from ${prev}`)
      })
    }
    handleIntegrityFeedback(response.integrity)
    await refreshSprintDataAfterMutation()
    closeSprintDialog(true)
  } catch (error: any) {
    showToast(error?.message || (sprintDialogMode.value === 'add' ? 'Failed to assign sprint' : 'Failed to remove sprint'))
  } finally {
    sprintDialogSubmitting.value = false
  }
}

async function removeSprintChip(sprintId: number) {
  if (removingSprintId.value !== null) return
  const taskId = form.id || props.taskId
  if (!taskId || taskId === 'new') {
    showToast('Save the task before managing sprints')
    return
  }
  removingSprintId.value = sprintId
  try {
    const response = await api.sprintRemove({
      tasks: [taskId],
      sprint: sprintId,
      cleanup_missing: true,
    } as any)
    const changed = response.modified.length
    const label = response.sprint_label || `Sprint #${response.sprint_id || sprintId}`
    if (changed) {
      showToast(`Removed ${changed} task${changed === 1 ? '' : 's'} from ${label}`)
    } else {
      showToast('No sprints removed')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    messages.forEach((message) => showToast(message))
    handleIntegrityFeedback(response.integrity)
    await refreshSprintDataAfterMutation()
  } catch (error: any) {
    showToast(error?.message || 'Failed to remove sprint')
  } finally {
    removingSprintId.value = null
  }
}
</script>

<style>
.task-panel__overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 15, 15, 0.55);
  display: flex;
  justify-content: flex-end;
  z-index: 40;
}

.task-panel {
  width: min(640px, 100%);
  height: 100%;
  background: var(--color-bg, var(--bg));
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-lg, 0 10px 30px rgba(15, 23, 42, 0.3));
  border-left: 1px solid var(--color-border, var(--border));
}

.task-panel__header {
  padding: var(--space-4, 1rem) var(--space-4, 1rem) var(--space-3, 0.75rem);
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--color-border, var(--border));
  gap: var(--space-3, 0.75rem);
}

.task-panel__title h2 {
  margin: 0;
  font-size: var(--text-lg, 1.25rem);
}

.task-panel__id {
  display: block;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.badge {
  padding: 0.2rem 0.6rem;
  border-radius: 999px;
  font-size: var(--text-xs, 0.75rem);
  letter-spacing: 0.02em;
}

.badge--success {
  background: color-mix(in oklab, #16a34a 14%, transparent);
  color: #166534;
}

.badge--info {
  background: color-mix(in oklab, #2563eb 14%, transparent);
  color: #1d4ed8;
}

.badge--danger {
  background: color-mix(in oklab, #ef4444 14%, transparent);
  color: #b91c1c;
}

.badge--muted {
  background: color-mix(in oklab, var(--color-muted, #6b7280) 16%, transparent);
  color: var(--color-bg, #fff);
}

.task-panel__body {
  flex: 1;
  overflow-y: auto;
  overflow-x: visible;
  padding: var(--space-4, 1rem);
}

.task-panel__sprints-loading {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  color: var(--color-muted, #64748b);
}

.task-panel__sprint-area {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__sprint-chip-remove {
  margin-left: var(--space-1, 0.25rem);
  border: 0;
  background: transparent;
  color: inherit;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: color 0.15s ease;
}

.task-panel__sprint-chip-remove .icon-glyph {
  width: 0.75rem;
  height: 0.75rem;
}

.task-panel__sprint-chip-remove:hover:not(:disabled),
.task-panel__sprint-chip-remove:focus-visible {
  color: var(--color-danger, #ef4444);
}

.task-panel__sprint-chip-remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.task-panel__sprint-field :deep(.chip-field__control) {
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
}

.task-panel__sprint-field :deep(.chip-field__chip) {
  padding: 0;
  background: transparent;
}

.task-panel__sprint-field :deep(.chip-field__add) {
  margin-left: 0;
}

.task-panel__sprint-chip {
  display: inline-flex;
  align-items: center;
  padding: calc(var(--space-1, 0.25rem)) var(--space-2, 0.5rem);
  border-radius: 999px;
  font-size: var(--text-xs, 0.75rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  color: var(--color-muted, #6b7280);
  border: 1px solid color-mix(in oklab, var(--color-border, #e2e8f0) 70%, transparent);
}

.task-panel__sprint-chip--active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  color: var(--color-accent, #0ea5e9);
  border-color: color-mix(in oklab, var(--color-accent, #0ea5e9) 55%, transparent);
}

.task-panel__sprint-chip--overdue {
  background: color-mix(in oklab, var(--color-danger, #ef4444) 18%, transparent);
  color: var(--color-danger, #ef4444);
  border-color: color-mix(in oklab, var(--color-danger, #ef4444) 55%, transparent);
}

.task-panel__sprint-chip--complete {
  background: color-mix(in oklab, var(--color-success, #16a34a) 18%, transparent);
  color: var(--color-success, #166534);
  border-color: color-mix(in oklab, var(--color-success, #16a34a) 55%, transparent);
}

.task-panel__sprint-chip--pending,
.task-panel__sprint-chip--unknown {
  background: color-mix(in oklab, var(--color-muted, #6b7280) 18%, transparent);
  color: var(--color-muted, #6b7280);
  border-color: color-mix(in oklab, var(--color-muted, #6b7280) 55%, transparent);
}

.task-panel__sprint-chip--missing {
  background: color-mix(in oklab, var(--color-danger, #ef4444) 12%, transparent);
  color: var(--color-danger, #ef4444);
  border-color: color-mix(in oklab, var(--color-danger, #ef4444) 55%, transparent);
  border-style: dashed;
}

.task-panel__sprint-warning {
  margin-top: var(--space-2, 0.5rem);
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-danger, #ef4444);
}

.task-panel__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

.task-panel__group {
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-lg, 0.75rem);
  padding: var(--space-3, 0.75rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__group legend,
.task-panel__group summary {
  font-weight: 600;
  font-size: var(--text-sm, 0.875rem);
  margin-bottom: var(--space-2, 0.5rem);
}

.task-panel__row {
  display: flex;
  gap: var(--space-2, 0.5rem);
}

.task-panel__row > * {
  flex: 1;
  min-width: 0;
}

.task-panel__tags-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__tags-label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 500;
  color: var(--color-muted, var(--muted));
}

.task-panel__tags-section :deep(.chip-field__control) {
  background: color-mix(in oklab, var(--color-surface, #1f2937) 94%, transparent);
}

.task-panel__tag-dialog .task-panel-dialog__card {
  width: min(520px, 100%);
}

.task-panel__tag-dialog-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__tag-suggestions {
  list-style: none;
  margin: 0;
  padding: var(--space-2, 0.5rem);
  background: color-mix(in oklab, var(--color-surface, #1f2937) 96%, transparent);
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.5rem);
  box-shadow: var(--shadow-md, 0 10px 24px rgba(15, 23, 42, 0.25));
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  max-height: 300px;
  overflow-y: auto;
}

.task-panel__tag-suggestions-item {
  margin: 0;
}

.task-panel__tag-suggestion {
  width: 100%;
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-1, 0.25rem) var(--space-3, 0.75rem);
  border: none;
  border-radius: var(--radius-sm, 0.25rem);
  background: transparent;
  color: inherit;
  font-size: var(--text-sm, 0.875rem);
  text-align: left;
  cursor: pointer;
  transition: background 0.16s ease, color 0.16s ease;
}

.task-panel__tag-suggestion:hover,
.task-panel__tag-suggestion:focus-visible {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 12%, transparent);
}

.task-panel__tag-suggestion.active,
.task-panel__tag-suggestion.active:hover,
.task-panel__tag-suggestion.active:focus-visible {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 20%, transparent);
  color: var(--color-fg, var(--fg));
}

.task-panel__tag-suggestion-label {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 0;
}

.task-panel__tag-suggestion-part {
  font-weight: 400;
}

.task-panel__tag-suggestion-part--match {
  font-weight: 600;
  color: color-mix(in oklab, var(--color-accent, #0ea5e9) 70%, var(--color-fg, var(--fg)) 30%);
}

.task-panel__tag-info {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__tag-hint {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-danger, #b91c1c);
}

.task-panel__row--ownership {
  align-items: stretch;
}

.task-panel__activity {
  gap: var(--space-4, 1rem);
}

.task-panel-dialog__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5, 1.25rem);
  background: color-mix(in oklab, var(--color-bg, #0f172a) 22%, transparent);
  z-index: 1000;
}

.task-panel-dialog__card {
  width: min(440px, 100%);
  max-height: calc(100vh - var(--space-6, 1.5rem));
  overflow-y: auto;
}

.task-panel-dialog__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

.task-panel-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__header h2 {
  margin: 0;
  font-size: var(--text-lg, 1.25rem);
}

.task-panel-dialog__field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__checkbox {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__footer {
  display: flex;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.task-panel__tabs {
  display: flex;
  gap: var(--space-2, 0.5rem);
  border-bottom: 1px solid var(--color-border, var(--border));
  padding-bottom: var(--space-2, 0.5rem);
  margin-bottom: var(--space-3, 0.75rem);
  flex-wrap: wrap;
}

.task-panel__tab {
  border: none;
  background: transparent;
  color: var(--color-muted, var(--muted));
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-radius: 999px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.2s ease, color 0.2s ease;
}

.task-panel__tab:hover {
  color: var(--color-fg, #0f172a);
  background: color-mix(in oklab, var(--color-surface, #f8fafc) 60%, transparent);
}

.task-panel__tab--active {
  color: var(--color-fg, #0f172a);
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
}

.task-panel__tab-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__ownership-column {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  flex: 1;
  min-width: 0;
}

.task-panel__ownership-label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 500;
  color: var(--color-muted, var(--muted));
}

.task-panel__ownership-custom {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel__ownership-custom :deep(.input) {
  flex: 1;
}

.task-panel__ownership-reset {
  flex: 0 0 auto;
  white-space: nowrap;
}

textarea {
  width: 100%;
  max-width: 100%;
  font: inherit;
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  background: var(--color-surface, var(--bg));
  color: inherit;
  box-sizing: border-box;
}

.task-panel :deep(.input),
.task-panel :deep(select) {
  max-width: 100%;
  box-sizing: border-box;
}

.task-panel__custom-fields {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__custom-row {
  display: flex;
  gap: var(--space-2, 0.5rem);
  align-items: center;
}

.task-panel__custom-row > *:first-child,
.task-panel__custom-row > *:nth-child(2) {
  flex: 1;
}

.task-panel__relations {
  display: grid;
  gap: var(--space-2, 0.5rem);
}

.task-panel__relation {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__relation-input {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__relation-input :deep(.input) {
  width: 100%;
}

.task-panel__relation-suggest {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  list-style: none;
  margin: 0;
  padding: var(--space-1, 0.25rem) 0;
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-bg, var(--bg));
  box-shadow: var(--shadow-sm, 0 4px 12px rgba(15, 23, 42, 0.12));
  max-height: 220px;
  overflow-y: auto;
  z-index: 5;
}

.task-panel__relation-suggest li {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  cursor: pointer;
}

.task-panel__relation-suggest li:hover,
.task-panel__relation-suggest li.active {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 82%, transparent);
}

.task-panel__relation-suggest li strong {
  font-weight: 600;
}

.task-panel__relation-suggest li span {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__group-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.task-panel__history-scroll {
  max-height: clamp(240px, 55vh, 520px);
  overflow-y: auto;
  width: 100%;
  padding-right: calc(var(--space-3, 0.75rem) + 12px);
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
  scrollbar-gutter: stable both-edges;
}

.task-panel__history h4,
.task-panel__references h4 {
  margin: 0 0 var(--space-2, 0.5rem);
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, var(--muted));
  font-weight: 600;
}

.task-panel__history-list,
.task-panel__references-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__history-item {
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 96%, transparent);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  min-width: 0;
}

.task-panel__history-meta {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__history-actor {
  font-weight: 600;
}

.task-panel__history-commit {
  font-family: var(--font-mono, 'SFMono-Regular', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace);
  letter-spacing: 0.02em;
}

.task-panel__history-changes {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__history-change {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__history-change strong {
  font-size: var(--text-xs, 0.75rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-muted, var(--muted));
}

.task-panel__history-change-values {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__history-old {
  color: var(--color-muted, var(--muted));
  text-decoration: line-through;
}

.task-panel__history-new {
  font-weight: 600;
}

.task-panel__commits-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__history-message {
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__history-author {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__references-list {
  gap: var(--space-2, 0.5rem);
}

.task-panel__reference-item {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  position: relative;
  overflow: visible;
}

.task-panel__reference-item--interactive {
  cursor: pointer;
}

.task-panel__reference-item--interactive:focus {
  outline: 2px solid color-mix(in oklab, var(--color-primary, #2563eb) 60%, transparent);
  outline-offset: 2px;
}

.task-panel__reference-code {
  font-family: var(--font-mono, 'SFMono-Regular', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace);
  font-size: var(--text-xs, 0.75rem);
  padding: 0.1rem 0.4rem;
  border-radius: var(--radius-sm, 0.25rem);
  background: color-mix(in oklab, var(--color-border, rgba(148, 163, 184, 0.4)) 24%, transparent);
}

.task-panel__reference-link {
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-primary, #2563eb);
  text-decoration: none;
  word-break: break-word;
}

.task-panel__reference-link:hover {
  text-decoration: underline;
}

.task-panel__reference-preview {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 90;
  width: min(560px, calc(100vw - 3rem));
  max-height: calc(100vh - 3rem);
  overflow: auto;
  padding: var(--space-4, 1rem);
  border-radius: var(--radius-lg, 0.75rem);
  border: 1px solid var(--color-border, var(--border));
  background: var(--color-surface, var(--bg));
  box-shadow: var(--shadow-lg, 0 18px 44px rgba(15, 23, 42, 0.22));
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__reference-meta {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
  display: flex;
  gap: var(--space-1, 0.25rem);
  align-items: baseline;
}

.task-panel__reference-snippet {
  max-height: 320px;
  overflow: auto;
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 88%, transparent);
  border: 1px solid color-mix(in oklab, var(--color-border, var(--border)) 60%, transparent);
  font-family: var(--font-mono, 'SFMono-Regular', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace);
}

.task-panel__reference-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
  margin-top: var(--space-2, 0.5rem);
}

.task-panel__reference-expand {
  font-size: var(--text-xs, 0.75rem);
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
}

.task-panel__reference-line {
  display: grid;
  grid-template-columns: minmax(2.5rem, auto) 1fr;
  gap: var(--space-2, 0.5rem);
  padding: 0 var(--space-3, 0.75rem);
  white-space: pre;
  font-size: var(--text-xs, 0.75rem);
}

.task-panel__reference-line-number {
  color: var(--color-muted, var(--muted));
  text-align: right;
  user-select: none;
}

.task-panel__reference-line-text {
  color: var(--color-fg, var(--fg));
}

.task-panel__reference-line--highlight {
  background: color-mix(in oklab, var(--color-primary, #2563eb) 18%, transparent);
}

.task-panel__reference-error {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-danger, #ef4444);
}

.task-panel__footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-3, 0.75rem);
}

.task-panel__errors {
  color: #b91c1c;
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__loading {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: center;
}

.task-panel__empty-hint {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, var(--muted));
}

.task-panel-enter-from,
.task-panel-leave-to {
  opacity: 0;
  transform: translateX(40px);
}

.task-panel-enter-active,
.task-panel-leave-active {
  transition: all 180ms ease;
}
</style>
