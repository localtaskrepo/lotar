<template>
  <section class="col" style="gap: 16px;">
    <header class="row" style="justify-content: space-between; align-items: baseline; gap: 12px; flex-wrap: wrap;">
      <div>
        <h1>Sprint backlog</h1>
        <p class="muted">Tasks without an assigned sprint. Use filters to narrow the list, then assign in bulk.</p>
      </div>
      <RouterLink class="btn ghost" to="/">Back to tasks</RouterLink>
    </header>

    <div class="card" style="display: flex; flex-direction: column; gap: 12px;">
      <div class="row" style="gap: 12px; flex-wrap: wrap; align-items: center;">
        <label class="col" style="gap: 4px; min-width: 180px;">
          <span class="muted">Project</span>
          <UiSelect v-model="projectFilter">
            <option value="">All projects</option>
            <option v-for="project in projects" :key="project.prefix" :value="project.prefix">
              {{ projectLabel(project) }}
            </option>
          </UiSelect>
        </label>
        <label class="col" style="gap: 4px; min-width: 180px;">
          <span class="muted">Statuses</span>
          <input class="input" v-model="statusFilter" placeholder="Todo, InProgress" />
        </label>
        <label class="col" style="gap: 4px; min-width: 180px;">
          <span class="muted">Tags</span>
          <input class="input" v-model="tagFilter" placeholder="frontend, bug" />
        </label>
        <label class="col" style="gap: 4px; min-width: 160px;">
          <span class="muted">Assignee</span>
          <input class="input" v-model="assigneeFilter" placeholder="@me or username" />
        </label>
        <label class="col" style="gap: 4px; width: 100px;">
          <span class="muted">Limit</span>
          <input class="input" type="number" min="1" max="200" v-model.number="limit" />
        </label>
        <button class="btn" type="button" @click="refreshBacklog">Refresh</button>
      </div>
      <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
        <strong>Assign selected</strong>
        <UiSelect v-model="sprintSelection">
          <option v-for="opt in sprintOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
        </UiSelect>
        <label class="row" style="gap: 6px; align-items: center;">
          <input type="checkbox" v-model="allowClosed" />
          Allow closed
        </label>
        <button class="btn primary" :disabled="!selectedIds.length" type="button" @click="assignSelected">Assign to sprint</button>
        <span class="muted">Selected: {{ selectedIds.length }}</span>
        <UiLoader v-if="loading || sprintsLoading" size="sm" />
      </div>
      <div v-if="hasMissingSprints" class="alert warn">
        <p>
          Missing sprint files detected for:
          <strong>{{ missingSprintMessage }}</strong>
          — assignments will offer automatic cleanup here as well.
        </p>
      </div>
    </div>

    <UiEmptyState
      v-if="!loading && !tasks.length"
      title="Backlog is clear"
      description="No tasks match the filters."
      primary-label="Refresh"
      @primary="refreshBacklog"
    />

    <UiLoader v-else-if="loading && !tasks.length" size="md" />

    <div v-else class="card" style="overflow-x: auto;">
      <table class="table" style="width: 100%; min-width: 720px;">
        <thead>
          <tr>
            <th style="width: 32px;">
              <input type="checkbox" :checked="allSelected" @change="toggleAll($event)" />
            </th>
            <th>ID</th>
            <th>Title</th>
            <th>Status</th>
            <th>Priority</th>
            <th>Assignee</th>
            <th>Due</th>
            <th>Tags</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="task in tasks" :key="task.id">
            <td>
              <input type="checkbox" :checked="selectedIds.includes(task.id)" @change="toggleOne(task.id, $event)" />
            </td>
            <td>{{ task.id }}</td>
            <td>{{ task.title }}</td>
            <td>{{ task.status }}</td>
            <td>{{ task.priority }}</td>
            <td>{{ task.assignee || '—' }}</td>
            <td>{{ task.due_date || '—' }}</td>
            <td>
              <span v-for="tag in task.tags" :key="tag" class="chip small">{{ tag }}</span>
              <span v-if="!task.tags.length" class="muted">—</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { api } from '../api/client'
import type { SprintBacklogTask } from '../api/types'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { showToast } from '../components/toast'
import { useProjects } from '../composables/useProjects'
import { useSprints } from '../composables/useSprints'

const tasks = ref<SprintBacklogTask[]>([])
const loading = ref(false)
const selectedIds = ref<string[]>([])
const projectFilter = ref('')
const statusFilter = ref('')
const tagFilter = ref('')
const assigneeFilter = ref('')
const limit = ref(50)
const sprintSelection = ref('active')
const allowClosed = ref(false)

const { projects, refresh: refreshProjects } = useProjects()
const { sprints, loading: sprintsLoading, refresh: refreshSprints, active: activeSprints, missingSprints, hasMissing: hasMissingSprints } = useSprints()
const missingSprintMessage = computed(() => {
  if (!hasMissingSprints.value) return ''
  return missingSprints.value.map((id) => `#${id}`).join(', ')
})

const sprintOptions = computed(() => {
  const options: Array<{ value: string; label: string }> = []
  const activeList = activeSprints.value
  const autoLabel = (() => {
    if (!activeList.length) return 'Auto (requires an active sprint)'
    if (activeList.length === 1) {
      const sprint = activeList[0]
      const name = sprint.label || sprint.display_name || `Sprint ${sprint.id}`
      return `Auto (active: #${sprint.id} ${name})`
    }
    return 'Auto (multiple active sprints – specify one)'
  })()
  options.push({ value: 'active', label: autoLabel })
  options.push({ value: 'next', label: 'Next sprint' })
  options.push({ value: 'previous', label: 'Previous sprint' })
  const sorted = [...sprints.value].sort((a, b) => a.id - b.id)
  sorted.forEach((item) => {
    const name = item.label || item.display_name || `Sprint ${item.id}`
    const state = item.state.charAt(0).toUpperCase() + item.state.slice(1)
    options.push({ value: String(item.id), label: `#${item.id} ${name} (${state})` })
  })
  if (!options.some((opt) => opt.value === sprintSelection.value)) {
    sprintSelection.value = options[0]?.value ?? 'active'
  }
  return options
})

const allSelected = computed(() => tasks.value.length > 0 && tasks.value.every((task) => selectedIds.value.includes(task.id)))

function toggleAll(event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  selectedIds.value = checked ? tasks.value.map((task) => task.id) : []
}

function toggleOne(id: string, event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  const set = new Set(selectedIds.value)
  if (checked) set.add(id)
  else set.delete(id)
  selectedIds.value = Array.from(set)
}

function parseList(value: string): string[] {
  return value
    .split(',')
    .map((token) => token.trim())
    .filter(Boolean)
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

function projectLabel(project: { prefix: string; name?: string | null }) {
  const prefix = project?.prefix || ''
  const name = (project?.name || '').trim()
  if (name && name !== prefix) {
    return `${name} (${prefix})`
  }
  return prefix
}

async function fetchBacklog() {
  loading.value = true
  try {
    const statuses = parseList(statusFilter.value)
    const tags = parseList(tagFilter.value)
    const response = await api.sprintBacklog({
      project: projectFilter.value || undefined,
      status: statuses.length ? statuses : undefined,
      tags: tags.length ? tags : undefined,
      assignee: assigneeFilter.value || undefined,
      limit: limit.value,
      cleanup_missing: true,
    })
    tasks.value = response.tasks || []
    selectedIds.value = []
    if (response.truncated) {
      showToast('Backlog truncated by limit; adjust filters to see more tasks')
    }
    const autoCleanup = response.integrity?.auto_cleanup
    if (autoCleanup?.removed_references) {
      showToast(`Automatically cleaned ${autoCleanup.removed_references} dangling sprint reference(s).`)
    }
    if (response.integrity?.missing_sprints?.length) {
      showToast(`Missing sprint IDs still detected: ${response.integrity.missing_sprints.map((id) => `#${id}`).join(', ')}`)
    }
  } catch (err: any) {
    showToast(err?.message || 'Failed to load sprint backlog')
    tasks.value = []
  } finally {
    loading.value = false
  }
}

async function refreshBacklog() {
  await fetchBacklog()
}

async function assignSelected() {
  if (!selectedIds.value.length) {
    showToast('Select at least one task to assign')
    return
  }
  const payload: any = { tasks: [...selectedIds.value] }
  const sprintRef = parseSprintToken(sprintSelection.value)
  if (sprintRef !== undefined) payload.sprint = sprintRef
  if (allowClosed.value) payload.allow_closed = true
  payload.cleanup_missing = true
  try {
    const response = await api.sprintAdd(payload)
    const changed = response.modified.length
    const sprintName = response.sprint_label || `Sprint #${response.sprint_id}`
    if (changed > 0) {
      showToast(`Added ${changed} task(s) to ${sprintName}`)
      tasks.value = tasks.value.filter((task) => !response.modified.includes(task.id))
      selectedIds.value = []
      await refreshSprints(true)
    } else {
      showToast('No backlog tasks were assigned')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    if (messages.length) {
      messages.forEach((message) => showToast(message))
    } else if (Array.isArray(response.replaced) && response.replaced.length) {
      response.replaced.forEach((entry) => {
        if (!entry?.previous?.length) return
        const prev = entry.previous.map((id) => `#${id}`).join(', ')
        showToast(`${entry.task_id} moved from ${prev}`)
      })
    }
    const autoCleanup = response.integrity?.auto_cleanup
    if (autoCleanup?.removed_references) {
      showToast(`Automatically cleaned ${autoCleanup.removed_references} dangling sprint reference(s).`)
    }
    if (response.integrity?.missing_sprints?.length) {
      showToast(`Missing sprint IDs still detected: ${response.integrity.missing_sprints.map((id) => `#${id}`).join(', ')}`)
    }
  } catch (err: any) {
    showToast(err?.message || 'Failed to assign to sprint')
  }
}

onMounted(async () => {
  await refreshProjects()
  await refreshSprints(true)
  await refreshBacklog()
})
</script>
