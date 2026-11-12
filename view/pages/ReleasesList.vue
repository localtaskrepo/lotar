<template>
  <section class="col releases-page" style="gap: 16px">
    <PageHeader reload-label="Refresh releases" :reloading="busy" @reload="refreshAll">
      <template #title>Releases</template>
      <template #toolbar>
        <UiButton class="releases-create" type="button" data-testid="release-create" @click="openCreate">
          <IconGlyph name="plus" aria-hidden="true" />
          <span>Release</span>
        </UiButton>
      </template>
    </PageHeader>

    <div v-if="selectedCount" class="card releases-bulk" role="region" aria-label="Selected tickets">
      <span class="releases-bulk__count">{{ selectedCount }} selected</span>
      <select
        v-model="bulkTarget"
        class="input releases-bulk__target"
        aria-label="Target release"
        data-testid="release-bulk-target"
      >
        <option v-for="release in releases" :key="release.id" :value="String(release.id)">
          {{ release.display_name }}
        </option>
      </select>
      <div class="releases-bulk__actions">
        <UiButton type="button" :disabled="assigning || !bulkTarget" data-testid="release-bulk-add" @click="bulkAdd">
          Add to
        </UiButton>
        <UiButton type="button" :disabled="assigning || !bulkTarget" data-testid="release-bulk-move" @click="bulkMove">
          Move to
        </UiButton>
        <UiButton
          type="button"
          :disabled="assigning || !selectedInReleases"
          :title="
            selectedInReleases ? 'Remove from the release each ticket was selected in' : 'Select tickets in a release'
          "
          data-testid="release-bulk-remove"
          @click="bulkRemove"
        >
          Remove from release
        </UiButton>
        <UiButton variant="ghost" type="button" :disabled="assigning" @click="clearSelection">Clear</UiButton>
      </div>
    </div>

    <UiLoader v-if="!initialized" size="md" />

    <div v-else-if="loadError && !releases.length" class="card releases-error" role="alert">
      <span>Could not load releases: {{ loadError }}</span>
      <UiButton variant="ghost" type="button" :disabled="busy" @click="refreshAll">Retry</UiButton>
    </div>

    <template v-else>
      <div v-if="loadError" class="card releases-error" role="alert">
        <span>Refresh failed: {{ loadError }}</span>
        <UiButton variant="ghost" type="button" :disabled="busy" @click="refreshAll">Retry</UiButton>
      </div>

      <UiEmptyState
        v-if="!releases.length"
        title="No releases yet"
        description="Create a release to plan which tickets ship in which version."
        primary-label="Create release"
        @primary="openCreate"
      />

      <div class="col releases-stack">
        <UiCard
          v-for="release in releases"
          :key="release.id"
          class="release-group"
          :class="{ 'release-group--drop': dropTarget === release.id }"
          :data-release-id="release.id"
          data-testid="release-group"
          @dragenter.prevent="onDragEnter(release.id)"
          @dragover.prevent="onDragOver($event, release.id)"
          @dragleave="onDragLeave($event, release.id)"
          @drop.prevent="onDrop(release.id)"
        >
          <header class="group-header">
            <button
              class="collapse-btn"
              type="button"
              :aria-expanded="isExpanded(release.id)"
              :aria-label="`${isExpanded(release.id) ? 'Collapse' : 'Expand'} ${release.display_name}`"
              @click="toggleExpanded(release.id)"
            >
              <span aria-hidden="true">{{ isExpanded(release.id) ? '▾' : '▸' }}</span>
            </button>
            <div class="group-title">
              <div class="group-title__heading">
                <h2>{{ release.display_name }}</h2>
                <span
                  class="badge"
                  :class="release.state === 'released' ? 'badge--success' : 'badge--info'"
                  data-testid="release-state"
                  >{{ release.state }}</span
                >
                <span v-if="release.overdue" class="badge badge--danger" data-testid="release-overdue">overdue</span>
              </div>
              <div class="group-meta">
                <div class="meta-item">
                  <span class="meta-label">ID</span>
                  <span class="meta-value">#{{ release.id }}</span>
                </div>
                <div v-if="release.planned_date" class="meta-item" :class="{ 'meta-item--danger': release.overdue }">
                  <span class="meta-label">Planned</span>
                  <span class="meta-value">{{ formatDay(release.planned_date) }}</span>
                </div>
                <div v-if="release.released_at" class="meta-item">
                  <span class="meta-label">Released</span>
                  <span class="meta-value" :title="release.released_at">{{ formatDay(release.released_at) }}</span>
                </div>
                <div v-if="release.notes" class="meta-item meta-item--notes" :title="release.notes">
                  <span class="meta-label">Notes</span>
                  <span class="meta-value meta-value--truncate">{{ release.notes }}</span>
                </div>
              </div>
            </div>
            <div class="release-progress-block">
              <span class="group-count" data-testid="release-counts"
                >{{ release.open_count }} open · {{ release.done_count }} done</span
              >
              <div
                class="release-progress"
                role="progressbar"
                :aria-label="`${release.display_name} progress`"
                aria-valuemin="0"
                :aria-valuemax="release.task_count"
                :aria-valuenow="release.done_count"
              >
                <span class="release-progress__bar" :style="{ width: `${progressPercent(release)}%` }"></span>
              </div>
            </div>
            <div class="group-actions">
              <UiLoader v-if="lifecycleBusy === release.id" size="sm" />
              <UiButton
                v-if="release.state !== 'released'"
                class="small"
                type="button"
                data-testid="release-mark-released"
                :disabled="lifecycleBusy !== null"
                @click="markReleased(release)"
              >
                Mark released
              </UiButton>
              <UiButton
                v-else
                class="small"
                type="button"
                data-testid="release-reopen"
                :disabled="lifecycleBusy !== null"
                @click="reopen(release)"
              >
                Reopen
              </UiButton>
              <UiButton
                icon-only
                type="button"
                :aria-label="`Edit ${release.display_name}`"
                title="Edit release"
                @click="openEdit(release)"
              >
                <IconGlyph name="edit" />
              </UiButton>
              <UiButton
                class="release-delete-button"
                icon-only
                type="button"
                :aria-label="`Delete ${release.display_name}`"
                title="Delete release"
                data-testid="release-delete"
                @click="openDelete(release)"
              >
                <IconGlyph name="trash" />
              </UiButton>
            </div>
          </header>
          <div v-show="isExpanded(release.id)" class="group-body">
            <p v-if="tasksLoading && !releaseTasks[release.id]" class="muted empty-placeholder">Loading tickets…</p>
            <p v-else-if="!(releaseTasks[release.id] ?? []).length" class="muted empty-placeholder">
              No tickets in this release. Drag tickets here from the backlog or another release.
            </p>
            <ReleaseTaskTable
              v-else
              :tasks="releaseTasks[release.id] ?? []"
              :source="release.id"
              :caption="`Tickets in ${release.display_name}`"
              :is-selected="isSelected"
              :dragging-id="dragging?.task ?? null"
              @toggle="toggleSelected"
              @toggle-all="toggleAll"
              @open="openTask"
              @dragstart="onTaskDragStart"
              @dragend="onTaskDragEnd"
            />
          </div>
        </UiCard>

        <UiCard
          id="backlog"
          class="release-group backlog-group"
          :class="{ 'release-group--drop': dropTarget === 'backlog' }"
          data-release-id="backlog"
          data-testid="release-backlog"
          @dragenter.prevent="onDragEnter('backlog')"
          @dragover.prevent="onDragOver($event, 'backlog')"
          @dragleave="onDragLeave($event, 'backlog')"
          @drop.prevent="onDrop('backlog')"
        >
          <header class="group-header">
            <button
              class="collapse-btn"
              type="button"
              :aria-expanded="isExpanded('backlog')"
              :aria-label="isExpanded('backlog') ? 'Collapse backlog' : 'Expand backlog'"
              @click="toggleExpanded('backlog')"
            >
              <span aria-hidden="true">{{ isExpanded('backlog') ? '▾' : '▸' }}</span>
            </button>
            <div class="group-title">
              <h2>Not in a release</h2>
              <div class="group-meta">
                <span class="muted">Open tickets in no release. Drop a ticket here to take it out of a release.</span>
              </div>
            </div>
            <span class="group-count" data-testid="release-backlog-count"
              >{{ backlogTotal }} ticket{{ backlogTotal === 1 ? '' : 's' }}</span
            >
          </header>
          <div v-show="isExpanded('backlog')" class="group-body">
            <p v-if="!backlog.length" class="muted empty-placeholder">Every open ticket is in a release.</p>
            <ReleaseTaskTable
              v-else
              :tasks="backlog"
              :source="null"
              caption="Open tickets in no release"
              :is-selected="isSelected"
              :dragging-id="dragging?.task ?? null"
              @toggle="toggleSelected"
              @toggle-all="toggleAll"
              @open="openTask"
              @dragstart="onTaskDragStart"
              @dragend="onTaskDragEnd"
            />
            <UiPagination
              :offset="backlogOffset"
              :total="backlogTotal"
              :page-size="pageSize"
              :disabled="busy"
              ariaLabel="Backlog pagination"
              @page="goToBacklogPage"
            />
          </div>
        </UiCard>
      </div>
    </template>

    <UiModal
      :open="dialog.open"
      :aria-label="dialog.mode === 'create' ? 'Create release' : 'Edit release'"
      initial-focus="[data-autofocus]"
      @close="closeDialog"
    >
      <form class="col release-form" data-testid="release-form" @submit.prevent="submitDialog">
        <header class="row release-form__header">
          <h2>{{ dialog.mode === 'create' ? 'Create release' : 'Edit release' }}</h2>
          <UiButton
            icon-only
            variant="ghost"
            type="button"
            aria-label="Close dialog"
            :disabled="dialog.submitting"
            @click="closeDialog"
          >
            <IconGlyph name="close" />
          </UiButton>
        </header>
        <div class="release-form__grid">
          <label class="release-form__field">
            <span class="muted">Version *</span>
            <input
              v-model="form.version"
              class="input"
              placeholder="0.15.0"
              required
              data-autofocus
              data-testid="release-form-version"
            />
          </label>
          <label class="release-form__field">
            <span class="muted">Label</span>
            <input v-model="form.label" class="input" placeholder="Optional name" />
          </label>
          <label class="release-form__field">
            <span class="muted">Planned date</span>
            <input v-model="form.planned_date" class="input" type="date" data-testid="release-form-planned" />
          </label>
          <label class="release-form__field">
            <span class="muted">Released on</span>
            <input v-model="form.released_at" class="input" type="date" />
          </label>
        </div>
        <label class="release-form__field">
          <span class="muted">Notes</span>
          <textarea v-model="form.notes" class="input" rows="3" placeholder="Release notes"></textarea>
        </label>
        <p v-if="dialog.error" class="release-form__error" role="alert">{{ dialog.error }}</p>
        <footer class="row release-form__footer">
          <UiButton variant="primary" type="submit" :disabled="dialog.submitting" data-testid="release-form-submit">
            {{
              dialog.submitting
                ? dialog.mode === 'create'
                  ? 'Creating…'
                  : 'Saving…'
                : dialog.mode === 'create'
                  ? 'Create release'
                  : 'Save changes'
            }}
          </UiButton>
          <UiButton variant="ghost" type="button" :disabled="dialog.submitting" @click="closeDialog">Cancel</UiButton>
        </footer>
      </form>
    </UiModal>

    <UiModal :open="deleteDialog.open" :aria-label="`Delete ${deleteDialog.name}`" @close="closeDelete">
      <div class="col release-form">
        <header class="row release-form__header">
          <h2>Delete {{ deleteDialog.name }}</h2>
        </header>
        <p>
          The release record is removed.
          <template v-if="deleteDialog.count">
            {{ deleteDialog.count }} ticket{{ deleteDialog.count === 1 ? '' : 's' }} will leave it; their other releases
            stay.
          </template>
        </p>
        <footer class="row release-form__footer">
          <UiButton
            variant="danger"
            type="button"
            :disabled="deleteDialog.submitting"
            data-testid="release-delete-confirm"
            @click="confirmDelete"
          >
            {{ deleteDialog.submitting ? 'Deleting…' : 'Delete release' }}
          </UiButton>
          <UiButton variant="ghost" type="button" :disabled="deleteDialog.submitting" @click="closeDelete"
            >Cancel</UiButton
          >
        </footer>
      </div>
    </UiModal>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { api } from '../api/client'
import type { ReleaseListItem, ReleaseUpdateRequest, TaskDTO } from '../api/types'
import IconGlyph from '../components/IconGlyph.vue'
import PageHeader from '../components/PageHeader.vue'
import ReleaseTaskTable from '../components/ReleaseTaskTable.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import UiModal from '../components/UiModal.vue'
import UiPagination from '../components/UiPagination.vue'
import { useReleases } from '../composables/useReleases'
import { useSse } from '../composables/useSse'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { formatTaskDate, toDateInputValue } from '../utils/date'
import { errorMessage } from '../utils/errors'
import { readTasksPageSizePreference } from '../utils/preferences'
import { projectOf } from '../utils/text'

type Section = number | 'backlog'

const { releases, refresh: refreshReleases } = useReleases()
const { openTaskPanel } = useTaskPanelController()

const initialized = ref(false)
const busy = ref(false)
const tasksLoading = ref(false)
const loadError = ref('')
const releaseTasks = ref<Record<number, TaskDTO[]>>({})
const backlog = ref<TaskDTO[]>([])
const backlogTotal = ref(0)
const backlogOffset = ref(0)
const pageSize = ref(readTasksPageSizePreference())

// Released releases start collapsed; a toggle overrides that per section.
const toggled = reactive(new Map<Section, boolean>())
function isExpanded(section: Section): boolean {
  const explicit = toggled.get(section)
  if (explicit !== undefined) return explicit
  if (section === 'backlog') return true
  return releases.value.find((release) => release.id === section)?.state !== 'released'
}
function toggleExpanded(section: Section) {
  toggled.set(section, !isExpanded(section))
}

function formatDay(value: string): string {
  return formatTaskDate(value, { year: 'numeric', month: 'short', day: 'numeric' })
}

function progressPercent(release: ReleaseListItem): number {
  return release.task_count ? Math.round((release.done_count / release.task_count) * 100) : 0
}

let loadGeneration = 0

async function loadReleaseTasks(ids: number[]) {
  const results = await Promise.all(
    ids.map(async (id) => {
      try {
        const response = await api.releaseGet(id)
        return [id, Array.isArray(response.tasks) ? response.tasks : []] as const
      } catch {
        return [id, null] as const
      }
    }),
  )
  const next: Record<number, TaskDTO[]> = {}
  for (const [id, tasks] of results) next[id] = tasks ?? releaseTasks.value[id] ?? []
  return next
}

async function loadBacklog() {
  const response = await api.releaseBacklog({ limit: pageSize.value, offset: backlogOffset.value })
  return response
}

async function refreshAll() {
  const generation = ++loadGeneration
  busy.value = true
  tasksLoading.value = true
  try {
    await refreshReleases(true)
    const [tasks, backlogPage] = await Promise.all([
      loadReleaseTasks(releases.value.map((release) => release.id)),
      loadBacklog(),
    ])
    if (generation !== loadGeneration) return
    releaseTasks.value = tasks
    backlog.value = Array.isArray(backlogPage.tasks) ? backlogPage.tasks : []
    backlogTotal.value = backlogPage.total
    if (backlogOffset.value > 0 && backlogOffset.value >= backlogPage.total) {
      backlogOffset.value = Math.max(0, Math.floor((backlogPage.total - 1) / pageSize.value) * pageSize.value)
    }
    loadError.value = ''
    pruneSelection()
  } catch (error: unknown) {
    if (generation === loadGeneration) loadError.value = errorMessage(error) || 'Failed to load releases'
  } finally {
    if (generation === loadGeneration) {
      busy.value = false
      tasksLoading.value = false
      initialized.value = true
    }
  }
}

async function goToBacklogPage(page: number) {
  backlogOffset.value = Math.max(0, (page - 1) * pageSize.value)
  await refreshAll()
}

function openTask(task: TaskDTO) {
  const prefix = projectOf(task.id)
  openTaskPanel({ taskId: task.id, initialProject: prefix || null, onUpdated: () => void refreshAll() })
}

// --- Selection: a ticket is selected in the section it was picked from ---

const selection = ref(new Map<string, { task: string; source: number | null }>())
const bulkTarget = ref('')
const assigning = ref(false)

const selectionKey = (task: string, source: number | null) => `${source ?? 'backlog'}:${task}`
const selectedCount = computed(() => new Set([...selection.value.values()].map((entry) => entry.task)).size)
const selectedInReleases = computed(() => [...selection.value.values()].some((entry) => entry.source !== null))

function isSelected(task: string, source: number | null): boolean {
  return selection.value.has(selectionKey(task, source))
}

function toggleSelected(task: string, source: number | null) {
  const next = new Map(selection.value)
  const key = selectionKey(task, source)
  if (next.has(key)) next.delete(key)
  else next.set(key, { task, source })
  selection.value = next
  ensureBulkTarget()
}

function toggleAll(tasks: string[], source: number | null, checked: boolean) {
  const next = new Map(selection.value)
  for (const task of tasks) {
    const key = selectionKey(task, source)
    if (checked) next.set(key, { task, source })
    else next.delete(key)
  }
  selection.value = next
  ensureBulkTarget()
}

function ensureBulkTarget() {
  if (releases.value.some((release) => String(release.id) === bulkTarget.value)) return
  const planned = releases.value.find((release) => release.state === 'planned') ?? releases.value[0]
  bulkTarget.value = planned ? String(planned.id) : ''
}

function clearSelection() {
  selection.value = new Map()
}

/** Drop selections whose ticket left the section it was selected in. */
function pruneSelection() {
  const present = new Set<string>()
  for (const [id, tasks] of Object.entries(releaseTasks.value)) {
    for (const task of tasks) present.add(selectionKey(task.id, Number(id)))
  }
  for (const task of backlog.value) present.add(selectionKey(task.id, null))
  const next = new Map([...selection.value].filter(([key]) => present.has(key)))
  if (next.size !== selection.value.size) selection.value = next
}

function releaseName(id: number): string {
  return releases.value.find((release) => release.id === id)?.display_name ?? `#${id}`
}

async function assign(action: 'add' | 'move' | 'remove', release: number, tasks: string[]): Promise<boolean> {
  if (!tasks.length) return false
  const payload = { release, tasks }
  const response =
    action === 'add'
      ? await api.releaseAdd(payload)
      : action === 'move'
        ? await api.releaseMove(payload)
        : await api.releaseRemove(payload)
  const count = response.modified.length
  const noun = `${count} ticket${count === 1 ? '' : 's'}`
  const name = releaseName(response.release_id)
  if (!count) showToast(action === 'remove' ? 'Nothing to remove' : `Already in ${name}`)
  else if (action === 'add') showToast(`Added ${noun} to ${name}`)
  else if (action === 'move') showToast(`Moved ${noun} to ${name}`)
  else showToast(`Removed ${noun} from ${name}`)
  return count > 0
}

async function runAssignment(run: () => Promise<boolean[]>) {
  if (assigning.value) return
  assigning.value = true
  try {
    const changed = await run()
    if (changed.some(Boolean)) {
      clearSelection()
      await refreshAll()
    }
  } catch (error: unknown) {
    showToast(errorMessage(error) || 'Failed to update releases')
    await refreshAll()
  } finally {
    assigning.value = false
  }
}

function selectedTaskIds(): string[] {
  return [...new Set([...selection.value.values()].map((entry) => entry.task))]
}

function bulkAdd() {
  const target = Number(bulkTarget.value)
  void runAssignment(async () => [await assign('add', target, selectedTaskIds())])
}

function bulkMove() {
  const target = Number(bulkTarget.value)
  void runAssignment(async () => [await assign('move', target, selectedTaskIds())])
}

function bulkRemove() {
  const bySource = new Map<number, string[]>()
  for (const entry of selection.value.values()) {
    if (entry.source === null) continue
    bySource.set(entry.source, [...(bySource.get(entry.source) ?? []), entry.task])
  }
  void runAssignment(async () => {
    const changed: boolean[] = []
    for (const [release, tasks] of bySource) changed.push(await assign('remove', release, tasks))
    return changed
  })
}

// --- Drag and drop: backlog → release adds, release → release moves, release → backlog removes ---

const dragging = ref<{ task: string; source: number | null } | null>(null)
const dropTarget = ref<Section | null>(null)

function onTaskDragStart(event: DragEvent, task: string, source: number | null) {
  dragging.value = { task, source }
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('text/plain', task)
  }
}

function onTaskDragEnd() {
  dragging.value = null
  dropTarget.value = null
}

function canDrop(section: Section): boolean {
  const drag = dragging.value
  if (!drag) return false
  if (section === 'backlog') return drag.source !== null
  return drag.source !== section
}

function onDragEnter(section: Section) {
  if (canDrop(section)) dropTarget.value = section
}

function onDragOver(event: DragEvent, section: Section) {
  if (!canDrop(section)) {
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'none'
    return
  }
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = section
}

function onDragLeave(event: DragEvent, section: Section) {
  const current = event.currentTarget as HTMLElement | null
  const related = event.relatedTarget as Node | null
  if (current && related && current.contains(related)) return
  if (dropTarget.value === section) dropTarget.value = null
}

function onDrop(section: Section) {
  const drag = dragging.value
  const allowed = canDrop(section)
  dragging.value = null
  dropTarget.value = null
  if (!drag || !allowed) return
  if (section === 'backlog') {
    const source = drag.source
    if (source === null) return
    void runAssignment(async () => [await assign('remove', source, [drag.task])])
  } else if (drag.source === null) {
    void runAssignment(async () => [await assign('add', section, [drag.task])])
  } else {
    void runAssignment(async () => [await assign('move', section, [drag.task])])
  }
}

// --- Mark released / reopen ---

const lifecycleBusy = ref<number | null>(null)

async function markReleased(release: ReleaseListItem) {
  if (lifecycleBusy.value !== null) return
  lifecycleBusy.value = release.id
  try {
    await api.releaseMarkReleased({ release: release.id })
    showToast(`Released ${release.display_name}`)
    await refreshAll()
  } catch (error: unknown) {
    showToast(errorMessage(error) || 'Failed to mark the release released')
  } finally {
    lifecycleBusy.value = null
  }
}

async function reopen(release: ReleaseListItem) {
  if (lifecycleBusy.value !== null) return
  lifecycleBusy.value = release.id
  try {
    await api.releaseReopen({ release: release.id })
    showToast(`Reopened ${release.display_name}`)
    await refreshAll()
  } catch (error: unknown) {
    showToast(errorMessage(error) || 'Failed to reopen the release')
  } finally {
    lifecycleBusy.value = null
  }
}

// --- Create / edit ---

const dialog = reactive({
  open: false,
  mode: 'create' as 'create' | 'edit',
  release: null as ReleaseListItem | null,
  submitting: false,
  error: '',
})
const form = reactive({ version: '', label: '', planned_date: '', released_at: '', notes: '' })

function fillForm(release: ReleaseListItem | null) {
  form.version = release?.version ?? ''
  form.label = release?.label ?? ''
  form.planned_date = release?.planned_date ?? ''
  form.released_at = toDateInputValue(release?.released_at)
  form.notes = release?.notes ?? ''
}

function openCreate() {
  dialog.mode = 'create'
  dialog.release = null
  dialog.error = ''
  fillForm(null)
  dialog.open = true
}

function openEdit(release: ReleaseListItem) {
  dialog.mode = 'edit'
  dialog.release = release
  dialog.error = ''
  fillForm(release)
  dialog.open = true
}

function closeDialog() {
  if (dialog.submitting) return
  dialog.open = false
}

/** Fields the user changed; an emptied field is sent as '' to clear it. */
function updatePayload(release: ReleaseListItem): ReleaseUpdateRequest {
  const payload: ReleaseUpdateRequest = { release: release.id }
  const version = form.version.trim()
  if (version !== release.version) payload.version = version
  if (form.label.trim() !== (release.label ?? '')) payload.label = form.label.trim()
  if (form.planned_date !== (release.planned_date ?? '')) payload.planned_date = form.planned_date
  if (form.released_at !== toDateInputValue(release.released_at)) payload.released_at = form.released_at
  if (form.notes.trim() !== (release.notes ?? '').trim()) payload.notes = form.notes.trim()
  return payload
}

async function submitDialog() {
  if (dialog.submitting) return
  const version = form.version.trim()
  if (!version) {
    dialog.error = 'Enter a version, for example 0.15.0.'
    return
  }
  dialog.submitting = true
  dialog.error = ''
  try {
    if (dialog.mode === 'create') {
      const response = await api.releaseCreate({
        version,
        ...(form.label.trim() ? { label: form.label.trim() } : {}),
        ...(form.notes.trim() ? { notes: form.notes.trim() } : {}),
        ...(form.planned_date ? { planned_date: form.planned_date } : {}),
        ...(form.released_at ? { released_at: form.released_at } : {}),
      })
      showToast(`Created ${response.release.display_name}`)
    } else if (dialog.release) {
      const payload = updatePayload(dialog.release)
      if (Object.keys(payload).length > 1) {
        const response = await api.releaseUpdate(payload)
        showToast(`Saved ${response.release.display_name}`)
      }
    }
    dialog.submitting = false
    dialog.open = false
    await refreshAll()
  } catch (error: unknown) {
    dialog.error = errorMessage(error) || 'Failed to save the release'
  } finally {
    dialog.submitting = false
  }
}

// --- Delete ---

const deleteDialog = reactive({ open: false, id: 0, name: '', count: 0, submitting: false })

function openDelete(release: ReleaseListItem) {
  deleteDialog.id = release.id
  deleteDialog.name = release.display_name
  deleteDialog.count = release.task_count
  deleteDialog.open = true
}

function closeDelete() {
  if (deleteDialog.submitting) return
  deleteDialog.open = false
}

async function confirmDelete() {
  if (deleteDialog.submitting) return
  deleteDialog.submitting = true
  try {
    await api.releaseDelete({ release: deleteDialog.id })
    showToast(`Deleted ${deleteDialog.name}`)
    deleteDialog.submitting = false
    deleteDialog.open = false
    await refreshAll()
  } catch (error: unknown) {
    showToast(errorMessage(error) || 'Failed to delete the release')
  } finally {
    deleteDialog.submitting = false
  }
}

// --- Live updates ---

let sse: ReturnType<typeof useSse> | null = null
let eventTimer: ReturnType<typeof setTimeout> | null = null
let disposed = false

function scheduleRefresh() {
  if (disposed) return
  if (eventTimer !== null) clearTimeout(eventTimer)
  eventTimer = setTimeout(() => {
    eventTimer = null
    if (!disposed) void refreshAll()
  }, 150)
}

onMounted(() => {
  sse = useSse(
    '/api/events',
    { kinds: 'release_changed,task_created,task_updated,task_deleted' },
    { onReconnect: scheduleRefresh },
  )
  for (const kind of ['release_changed', 'task_created', 'task_updated', 'task_deleted']) sse.on(kind, scheduleRefresh)
  void refreshAll()
})

onUnmounted(() => {
  disposed = true
  if (eventTimer !== null) clearTimeout(eventTimer)
  sse?.close()
})
</script>

<style scoped>
.releases-create {
  font-weight: 600;
  gap: var(--space-2, 0.5rem);
}

.releases-bulk {
  position: sticky;
  top: var(--space-2, 0.5rem);
  z-index: var(--z-popover, 20);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
}

.releases-bulk__count {
  font-weight: 600;
}

.releases-bulk__target {
  min-width: 160px;
  max-width: 100%;
}

.releases-bulk__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.releases-error {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border-left: 4px solid var(--color-danger);
}

.releases-stack {
  gap: 12px;
  width: 100%;
}

.release-group {
  position: relative;
  min-width: 0;
  transition:
    border-color var(--duration-med) var(--ease-standard),
    background var(--duration-med) var(--ease-standard);
}

.release-group--drop {
  border: 1px dashed var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
}

.group-header {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.collapse-btn {
  border: none;
  background: transparent;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--control-height);
  height: var(--control-height);
  flex: 0 0 auto;
  align-self: flex-start;
  cursor: pointer;
  color: inherit;
}

.group-title {
  display: flex;
  flex: 1 1 160px;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.group-title__heading {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  min-width: 0;
}

.group-title h2 {
  margin: 0;
  font-size: var(--text-lg, 1.125rem);
  overflow-wrap: anywhere;
}

.group-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 18px;
  align-items: center;
}

.meta-item {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  padding: 2px 0;
  font-size: var(--text-sm, 0.875rem);
  min-width: 0;
}

.meta-label {
  font-size: var(--text-xs, 0.75rem);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: color-mix(in oklab, var(--color-muted) 90%, transparent);
}

.meta-value {
  font-weight: 600;
  color: var(--color-fg, var(--fg));
}

.meta-item--danger .meta-value {
  color: var(--color-danger);
}

.meta-value--truncate {
  display: inline-block;
  max-width: clamp(140px, 30vw, 320px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
}

.release-progress-block {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  margin-left: auto;
  min-width: 120px;
}

.group-count {
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted);
  white-space: nowrap;
}

.release-progress {
  width: 120px;
  height: 6px;
  border-radius: var(--radius-pill);
  background: color-mix(in oklab, var(--color-muted) 22%, transparent);
  overflow: hidden;
}

.release-progress__bar {
  display: block;
  height: 100%;
  background: var(--color-success);
  border-radius: inherit;
}

.group-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
}

.group-body {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.empty-placeholder {
  margin: 0;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.15rem 0.45rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.badge--info {
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  color: var(--color-accent-strong);
}

.badge--danger {
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  color: var(--color-danger);
}

.badge--success {
  background: color-mix(in oklab, var(--color-success) 18%, transparent);
  color: var(--color-success-strong);
}

.release-delete-button:hover {
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  color: var(--color-danger);
  border-color: color-mix(in oklab, var(--color-danger) 45%, transparent);
}

.release-form {
  gap: var(--space-4, 1rem);
}

.release-form__header {
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--space-3, 0.75rem);
}

.release-form__header h2 {
  margin: 0;
}

.release-form__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.release-form__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.release-form__field .input {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
}

.release-form__error {
  margin: 0;
  color: var(--color-danger);
}

.release-form__footer {
  gap: 8px;
  flex-wrap: wrap;
}

@media (max-width: 720px) {
  .group-actions {
    width: 100%;
    justify-content: flex-start;
  }

  .release-progress-block {
    align-items: flex-start;
    margin-left: 0;
    width: 100%;
  }

  .release-progress {
    width: 100%;
  }
}

@media (max-width: 460px) {
  .release-form__grid {
    grid-template-columns: 1fr;
  }
}
</style>
