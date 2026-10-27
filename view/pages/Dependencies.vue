<template>
  <section class="dependencies-page">
    <PageHeader
      clear-label="Clear filters"
      :can-clear="hasFilters || !!scope.root"
      reload-label="Refresh dependencies"
      :reloading="graphLoading"
      @clear="clearFilters"
      @reload="graphRef?.refresh()"
    >
      <template #title>
        Dependencies
        <span v-if="scope.project" class="muted">— {{ scope.project }}</span>
      </template>
      <template #subtitle>Explore prerequisites, dependents, and connected tasks.</template>
      <template #view>
        <ColumnsMenu
          :open="fieldsMenuOpen"
          :options="cardFields.fieldOptions.value"
          :is-visible="cardFields.isVisible"
          :set-visible="cardFields.toggleColumn"
          label="Card fields"
          @update:open="fieldsMenuOpen = $event"
          @reset="cardFields.resetColumns"
        >
          <template #trigger="{ open, toggle }"
            ><UiButton
              class="responsive-action"
              type="button"
              aria-label="Configure card fields"
              title="Choose details displayed on task cards"
              :aria-expanded="open"
              @click="toggle"
              ><IconGlyph name="list" aria-hidden="true" /><span class="responsive-action__label"
                >Fields</span
              ></UiButton
            ></template
          >
        </ColumnsMenu>
      </template>
    </PageHeader>
    <div class="filter-card">
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :custom-presets="customFilterPresets"
        :sprint-options="sprintOptions"
        :tag-options="filterOptions.tags"
        :assignee-options="filterOptions.assignees"
        :custom-field-values="filterOptions.customFieldValues"
        :custom-field-options="customFields"
        :value="filterPayload"
        storage-key="lotar.dependencies.filter"
        emit-project-key
        show-order
        :enable-due-soon="false"
        :enable-recent="false"
        :task-suggestions="suggestTasks"
        @task-selected="selectTask"
        @update:value="updateFilters"
      >
        <template #actions>
          <label class="toolbar-field">
            <span class="toolbar-field__label">Sort by</span>
            <select
              class="input toolbar-select"
              aria-label="Sort tasks"
              :value="filter.sort_by || 'modified'"
              @change="setSort(($event.target as HTMLSelectElement).value)"
            >
              <option value="id">ID</option>
              <option value="title">Title</option>
              <option value="priority">Priority</option>
              <option value="status">Status</option>
              <option value="due">Due date</option>
              <option value="created">Created</option>
              <option value="modified">Updated</option>
            </select>
          </label>
          <UiButton
            :disabled="busy"
            :title="`Validate all relationships in ${scope.project ? `project ${scope.project}` : 'the workspace'}, not only this graph`"
            @click="checkIssues"
            >{{ scope.project ? 'Validate project' : 'Validate workspace' }}</UiButton
          >
          <UiButton :disabled="busy" @click="previewRepair()"><IconGlyph name="edit" />Fix relationships</UiButton>
        </template>
        <template #panel>
          <div class="dependency-view-options" role="group" aria-label="Graph display controls">
            <fieldset class="dependency-relationships">
              <legend>Relationship types</legend>
              <label class="dependency-relationships-all"
                ><input
                  type="checkbox"
                  aria-label="All relationship types"
                  :checked="scope.kinds.length === connectionOptions.length"
                  :indeterminate.prop="scope.kinds.length > 0 && scope.kinds.length < connectionOptions.length"
                  @change="writeRoute({ relationships: ($event.target as HTMLInputElement).checked ? 'all' : 'none' })"
                />All</label
              >
              <label v-for="option in connectionOptions" :key="option.kind"
                ><input
                  type="checkbox"
                  :checked="scope.kinds.includes(option.kind)"
                  @change="setConnection(option.kind, ($event.target as HTMLInputElement).checked)"
                />{{ option.label }}</label
              >
              <p class="muted">Choose connections independently. These options only change the view, not task links.</p>
            </fieldset>
            <div class="dependency-range-options">
              <label class="dependency-depth-picker"
                >Connection range
                <UiSelect
                  aria-label="Connection range"
                  aria-describedby="dependency-range-help"
                  :model-value="String(scope.depth)"
                  @update:model-value="writeRoute({ depth: $event })"
                  ><option v-for="depth in 9" :key="depth - 1" :value="String(depth - 1)">
                    {{ depth - 1 }} {{ depth === 2 ? 'hop' : 'hops' }}
                  </option></UiSelect
                ></label
              >
              <UiButton
                :variant="scope.showIsolated ? 'primary' : undefined"
                :aria-pressed="scope.showIsolated"
                aria-label="Show unlinked tasks"
                title="Show tasks with no connections in the selected relationship types"
                @click="writeRoute({ isolated: scope.showIsolated ? undefined : '1' })"
                ><IconGlyph name="list" />Show unlinked tasks</UiButton
              >
            </div>
            <p id="dependency-range-help" class="dependency-range-help muted">
              Hops count links from the selected task, or from matching tasks without a selection. Stages arrange the
              returned tasks in prerequisite order; they are not hop counts.
            </p>
          </div>
        </template>
      </FilterBar>
    </div>
    <p class="muted dependency-help">
      Search text and filters narrow the results. Choose a task suggestion or enter its exact ID to explore that task
      and its connections.
    </p>
    <section
      v-if="check || preview || error || notice"
      class="dependency-check"
      aria-label="Relationship integrity"
      :aria-busy="busy"
    >
      <p v-if="error" role="alert">{{ error }}</p>
      <p v-if="notice" role="status">{{ notice }}</p>
      <p v-if="check" class="muted">
        Validation covers {{ scope.project ? `all tasks in project ${scope.project}` : 'all tasks in the workspace' }},
        independent of the task, depth, relationship choices, or search filters.
      </p>
      <p v-if="check && !check.complete" role="alert">
        Validation is partial. Unreadable or ambiguous tasks may hide additional problems.
      </p>
      <details v-if="check?.issues.length" class="integrity-details">
        <summary>Validation results ({{ check.issues.length }})</summary>
        <ul aria-label="Relationship check issues">
          <li v-for="(issue, index) in check.issues" :key="index">
            <strong>{{ issue.code }}</strong
            >: {{ issue.message }} ({{ issue.repairable ? 'safe repair available' : 'manual review required' }})
          </li>
        </ul>
      </details>
      <section
        v-if="preview"
        ref="previewElement"
        tabindex="-1"
        class="repair-preview"
        aria-label="Safe repair preview"
      >
        <h2>{{ repairIssue ? 'Review this problem fix' : 'Review safe repairs' }}</h2>
        <p v-if="repairIssue">
          Selected problem: <strong>{{ repairIssue.message }}</strong
          >. Only this problem is included; unrelated safe fixes will not be applied.
        </p>
        <p v-else>
          Scope: <strong>{{ scope.project ? `project ${scope.project}` : 'the entire workspace' }}</strong
          >, not just this graph neighborhood. Nothing changes until you confirm.
        </p>
        <p v-if="preview.changed_tasks.length">
          {{ preview.changed_tasks.length }} {{ preview.changed_tasks.length === 1 ? 'task' : 'tasks' }} will be
          updated:
        </p>
        <ul v-if="preview.changed_tasks.length" class="repair-tasks">
          <li v-for="id in preview.changed_tasks" :key="id">{{ id }}</li>
        </ul>
        <p v-else>No automatic repairs available. Remaining issues need manual review.</p>
        <p class="muted">
          Restoring a reciprocal link may undo an intentional manual deletion of one side. Statuses and agent jobs are
          not changed.
        </p>
        <details v-if="preview.issues.length" class="integrity-details">
          <summary>View {{ preview.issues.length }} findings</summary>
          <ul>
            <li v-for="(issue, index) in preview.issues" :key="index">{{ issue.code }}: {{ issue.message }}</li>
          </ul>
        </details>
        <UiButton :disabled="busy || !preview.changed_tasks.length" @click="applyRepair"
          >Confirm and apply safe repair</UiButton
        >
        <UiButton :disabled="busy" @click="preview = null">Cancel preview</UiButton>
      </section>
    </section>
    <DependencyGraph
      ref="graphRef"
      :root="scope.root"
      :project="scope.project"
      :depth="scope.depth"
      :show-isolated="scope.showIsolated"
      :relationship-kinds="scope.kinds"
      :filters="graphFilters"
      :sort-by="filter.sort_by || 'modified'"
      :order="filter.order || 'desc'"
      :fields="cardFieldMap"
      :fix-busy="busy"
      :show-layer-controls="false"
      @loaded="loadedGraph"
      @activate="activateTask"
      @fix="previewRepair"
    />
  </section>
</template>

<script setup lang="ts">
import PageHeader from '../components/PageHeader.vue'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter, type LocationQueryRaw } from 'vue-router'
import { api } from '../api/client'
import type {
  RelationshipCheckResponse,
  RelationshipGraphResponse,
  RelationshipIssue,
  RelationshipKind,
  RelationshipRepairResponse,
} from '../api/types'
import ColumnsMenu from '../components/ColumnsMenu.vue'
import DependencyGraph from '../components/DependencyGraph.vue'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import UiButton from '../components/UiButton.vue'
import UiSelect from '../components/UiSelect.vue'
import { useColumns, BUILTIN_COLUMNS } from '../composables/useColumns'
import { useConfig } from '../composables/useConfig'
import { useCustomFilterPresets, useProjectFilterSync } from '../composables/useFilterBuilder'
import { useFilterOptions } from '../composables/useFilterOptions'
import { useSprints, useSprintFilterOptions } from '../composables/useSprints'
import { useSse } from '../composables/useSse'

const route = useRoute(),
  router = useRouter()
const pendingQuery = ref<LocationQueryRaw | null>(null)
const connectionOptions: Array<{ kind: RelationshipKind; label: string }> = [
  { kind: 'dependency', label: 'Blocking dependencies' },
  { kind: 'hierarchy', label: 'Parent / child' },
  { kind: 'related', label: 'Related tasks' },
  { kind: 'fixes', label: 'Fixes' },
  { kind: 'duplicate', label: 'Duplicates' },
]
const layerTypes = ['dependency', 'related', 'hierarchy', 'fixes', 'duplicate', 'all']
function queryText(query: LocationQueryRaw, key: string) {
  const raw = query[key]
  return typeof raw === 'string' ? raw : ''
}
function legacyLayerType(query: LocationQueryRaw) {
  return (
    query.relationships === undefined && !queryText(query, 'layers') && layerTypes.includes(queryText(query, 'type'))
  )
}
function readKinds(query: LocationQueryRaw): RelationshipKind[] {
  const raw = queryText(query, 'relationships')
  if (raw === 'all') return connectionOptions.map((option) => option.kind)
  if (query.relationships !== undefined)
    return connectionOptions.filter((option) => raw.split(',').includes(option.kind)).map((option) => option.kind)
  const legacy = queryText(query, 'layers') || (legacyLayerType(query) ? queryText(query, 'type') : 'dependency')
  return connectionOptions
    .filter((option) => option.kind === 'dependency' || legacy === 'all' || option.kind === legacy)
    .map((option) => option.kind)
}
const viewQuery = computed(() => pendingQuery.value || route.query)
const value = (key: string) => queryText(viewQuery.value, key)
const scope = computed(() => ({
  root: value('root'),
  project: value('project'),
  kinds: readKinds(viewQuery.value),
  depth: value('depth') === '' ? 1 : Number(value('depth')),
  showIsolated: value('isolated') === '1',
}))
const scopeKey = computed(() => JSON.stringify(scope.value))
const project = ref(value('project'))
const graphQueryKeys = new Set(['root', 'project', 'relationships', 'layers', 'depth', 'isolated', 'taskPanel'])
function routeFilters() {
  return Object.fromEntries(
    Object.entries(route.query).filter(
      ([key, entry]) =>
        !graphQueryKeys.has(key) && !(key === 'type' && legacyLayerType(route.query)) && typeof entry === 'string',
    ),
  ) as Record<string, string>
}
const filter = ref<Record<string, string>>(routeFilters())
const filterPayload = computed(() => ({
  ...filter.value,
  ...(filter.value.q || scope.value.root ? { q: filter.value.q || scope.value.root } : {}),
  ...(project.value || Object.prototype.hasOwnProperty.call(viewQuery.value, 'project')
    ? { project: project.value }
    : {}),
}))
const graphFilters = computed<Record<string, string>>((previous) => {
  const next = {
    deletion: 'active',
    ...Object.fromEntries(
      Object.entries(filter.value).filter(
        ([key, entry]) => key !== 'sort_by' && key !== 'order' && !(key === 'q' && entry === scope.value.root) && entry,
      ),
    ),
  }
  return previous && JSON.stringify(previous) === JSON.stringify(next) ? previous : next
})
const filterBarRef = ref<{ clear?: () => void } | null>(null)
const {
  statuses,
  priorities,
  types,
  customFields,
  refresh: refreshConfig,
  scope: configScope,
  tags: configuredTags,
  members,
} = useConfig()
const { sprints, refresh: refreshSprints } = useSprints()
const sprintOptions = useSprintFilterOptions(sprints)
const graphNodes = ref<RelationshipGraphResponse['nodes']>([])
const filterOptions = useFilterOptions(
  () => graphNodes.value.flatMap((node) => (node.task ? [node.task] : [])),
  () => project.value,
  { scope: () => configScope.value, tags: () => configuredTags.value, members: () => members.value },
)
const customFilterPresets = useCustomFilterPresets(customFields)
// Retain the existing per-project field choices when moving details onto cards.
const cardFields = useColumns({
  storagePrefix: 'lotar.dependencyHoverFields',
  // Graph nodes carry no references, so there is nothing to show for attachments.
  builtinColumns: BUILTIN_COLUMNS.filter((key) => key !== 'attachments'),
  defaultVisible: [
    'id',
    'title',
    'status',
    'priority',
    'reporter',
    'assignee',
    'tags',
    'sprints',
    'due_date',
    'modified',
  ],
})
const fieldsMenuOpen = ref(false)
const cardFieldMap = computed(() =>
  Object.fromEntries(cardFields.fieldOptions.value.map((option) => [option.key, cardFields.isVisible(option.key)])),
)
watch(customFields, (keys) => cardFields.setCustomFieldKeys(keys), { immediate: true })
watch(
  project,
  (prefix) => {
    cardFields.setProjectKey(prefix)
    graphNodes.value = []
    void refreshConfig(prefix || undefined)
  },
  { immediate: true },
)
const { hasFilters, onFilterUpdate } = useProjectFilterSync(project, filter)
function loadedGraph(result: RelationshipGraphResponse) {
  graphNodes.value = result.nodes
}
const graphRef = ref<{ refresh: () => void; loading: boolean } | null>(null)
const graphLoading = computed(() => graphRef.value?.loading ?? false)
let routeRequest = 0
function writeRoute(next: Record<string, string | number | undefined>) {
  const base = pendingQuery.value || route.query
  const kinds = readKinds(base)
  const query = Object.fromEntries(
    Object.entries({
      ...base,
      ...(legacyLayerType(base) ? { type: undefined } : {}),
      layers: undefined,
      relationships: kinds.join(',') || 'none',
      ...next,
    }).filter(([, entry]) => entry !== undefined && entry !== ''),
  )
  pendingQuery.value = query
  const request = ++routeRequest
  void Promise.resolve(router.replace({ path: '/dependencies', query, hash: route.hash }))
    .catch(() => {
      if (alive && request === routeRequest) error.value = 'Could not update the dependency view.'
    })
    .finally(() => {
      if (request === routeRequest) {
        pendingQuery.value = null
        hydrateRoute()
      }
    })
}
function updateFilters(next: Record<string, string>) {
  const previousFilter = filter.value
  const previousProject = project.value
  const previousSearch = filter.value.q || scope.value.root
  const previousRoot = scope.value.root
  const previous = Object.keys(filter.value)
  onFilterUpdate(next)
  const changingProject = project.value !== previousProject || project.value !== scope.value.project
  if (changingProject) {
    invalidate()
    if (previousRoot && filter.value.q === previousRoot) {
      const rest = { ...filter.value }
      delete rest.q
      filter.value = rest
    }
  }
  const changedSearch = (filter.value.q || '') !== previousSearch
  // Controlled FilterBar hydration can echo while a drill-down push is pending.
  // Replacing that identical view would cancel the push and destroy its origin.
  const unchangedConditions =
    previousFilter === filter.value ||
    [...new Set([...Object.keys(previousFilter), ...Object.keys(filter.value)])].every(
      (key) =>
        key === 'q' ||
        (key === 'order'
          ? (previousFilter.order || 'desc') === (filter.value.order || 'desc')
          : (previousFilter[key] || '') === (filter.value[key] || '')),
    )
  if (!changingProject && !changedSearch && unchangedConditions) return
  writeRoute({
    ...Object.fromEntries(previous.map((key) => [key, undefined])),
    ...filter.value,
    project: project.value,
    ...(changingProject || changedSearch ? { root: undefined } : {}),
  })
}
function setSort(sort_by: string) {
  updateFilters({ ...filterPayload.value, sort_by })
}
function clearFilters() {
  filterBarRef.value?.clear?.()
  updateFilters({ project: project.value })
}
function suggestTasks(query: string, prefix?: string) {
  return api.suggestTasks(query, prefix, 12)
}
function selectTask(id: string) {
  onFilterUpdate({ ...filter.value, project: project.value, q: id })
  writeRoute({ ...filter.value, q: id, root: id, project: project.value || undefined })
}
async function activateTask(id: string) {
  const query = { ...(pendingQuery.value || route.query), root: id, q: id, taskPanel: id }
  const request = ++routeRequest
  pendingQuery.value = query
  filter.value = { ...filter.value, q: id }
  try {
    await router.push({ path: '/dependencies', query, hash: route.hash })
  } catch {
    if (alive && request === routeRequest) error.value = 'Could not open the dependency task.'
  } finally {
    if (request === routeRequest) {
      pendingQuery.value = null
      hydrateRoute()
    }
  }
}
function setConnection(kind: RelationshipKind, checked: boolean) {
  const kinds = new Set(scope.value.kinds)
  if (checked) kinds.add(kind)
  else kinds.delete(kind)
  writeRoute({
    relationships:
      connectionOptions
        .filter((option) => kinds.has(option.kind))
        .map((option) => option.kind)
        .join(',') || 'none',
  })
}
const check = ref<RelationshipCheckResponse | null>(null)
const preview = ref<RelationshipRepairResponse | null>(null)
const repairIssue = ref<RelationshipIssue | null>(null)
const previewElement = ref<HTMLElement | null>(null)
const busy = ref(false),
  error = ref(''),
  notice = ref('')
let generation = 0,
  alive = true
let applyingGeneration: number | null = null
let stream: ReturnType<typeof useSse> | undefined
function invalidate() {
  generation += 1
  preview.value = null
  repairIssue.value = null
  check.value = null
  busy.value = false
  error.value = ''
  notice.value = ''
}
watch(scopeKey, invalidate, { immediate: true })
watch(
  () => scope.value.project,
  (prefix) => {
    project.value = prefix
  },
)
function hydrateRoute() {
  project.value = queryText(route.query, 'project')
  filter.value = routeFilters()
}
watch(
  () => route.query,
  () => {
    if (!pendingQuery.value) hydrateRoute()
  },
  { deep: true },
)
async function run(action: () => Promise<void>) {
  if (busy.value) return
  busy.value = true
  error.value = ''
  notice.value = ''
  const request = generation
  try {
    await action()
  } catch (cause) {
    if (alive && request === generation) {
      preview.value = null
      repairIssue.value = null
      error.value = cause instanceof Error ? cause.message : 'Relationship request failed'
    }
  } finally {
    if (alive && request === generation) busy.value = false
  }
}
function checkIssues() {
  const request = generation,
    project = scope.value.project || undefined
  return run(async () => {
    const result = await api.relationshipCheck(project)
    if (alive && request === generation) {
      check.value = result
      const label = project ? `project ${project}` : 'the workspace'
      notice.value = result.issues.length
        ? `${result.issues.length} issues reported for ${label}.`
        : result.complete
          ? `No relationship problems found in ${label}.`
          : `No issues found in the readable portion of ${label}; validation is incomplete.`
    }
  })
}
function previewRepair(issue?: RelationshipIssue) {
  if (busy.value) return
  if (issue && (!issue.id || !issue.repairable)) {
    error.value = 'This problem needs manual review or a refreshed graph before it can be fixed.'
    return
  }
  const request = generation,
    project = scope.value.project || undefined
  preview.value = null
  repairIssue.value = null
  return run(async () => {
    const result = await api.relationshipRepair({
      project,
      apply: false,
      ...(issue?.id ? { issue_ids: [issue.id] } : {}),
    })
    if (alive && request === generation) {
      preview.value = result
      repairIssue.value = issue || null
      await nextTick()
      if (alive && request === generation) {
        previewElement.value?.focus({ preventScroll: true })
        previewElement.value?.scrollIntoView?.({ block: 'start', behavior: 'smooth' })
      }
    }
  })
}
function applyRepair() {
  const plan = preview.value
  if (!plan || busy.value) return
  const request = generation,
    project = scope.value.project || undefined
  const issueIds = repairIssue.value?.id ? [repairIssue.value.id] : undefined
  applyingGeneration = request
  return run(async () => {
    const result = await api.relationshipRepair({
      project,
      apply: true,
      revision: plan.revision,
      ...(issueIds ? { issue_ids: issueIds } : {}),
    })
    if (alive && request === generation) {
      preview.value = null
      repairIssue.value = null
      check.value = null
      graphRef.value?.refresh()
      notice.value = result.applied
        ? `Safe repair applied to ${result.changed_tasks.length} task${result.changed_tasks.length === 1 ? '' : 's'}.`
        : 'No repair applied.'
    }
  }).finally(() => {
    if (applyingGeneration === request) applyingGeneration = null
  })
}
onMounted(() => {
  void refreshSprints()
  if (typeof EventSource === 'undefined') return
  const changed = () => {
    if (applyingGeneration === generation) {
      preview.value = null
      repairIssue.value = null
      check.value = null
      return
    }
    const hadPreview = !!preview.value || busy.value
    generation += 1
    preview.value = null
    repairIssue.value = null
    check.value = null
    busy.value = false
    if (hadPreview) notice.value = 'Task data changed. Preview safe repair again before applying.'
  }
  stream = useSse('/api/events', {}, { onReconnect: changed })
  for (const event of ['task_created', 'task_updated', 'task_deleted', 'task_restored', 'task_error'])
    stream.on(event, changed)
})
onBeforeUnmount(() => {
  alive = false
  generation += 1
  routeRequest += 1
  pendingQuery.value = null
  stream?.close()
})
</script>

<style scoped>
.dependencies-page {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  max-width: 100%;
  gap: 16px;
}
.dependencies-page > * {
  min-width: 0;
}
.dependency-view-options {
  display: flex;
  gap: 20px;
  align-items: start;
  flex-wrap: wrap;
}
.dependency-relationships {
  display: flex;
  gap: 8px 16px;
  flex-wrap: wrap;
  flex: 1 1 25rem;
  min-width: 0;
  border: 0;
  padding: 0;
  margin: 0;
}
.dependency-relationships legend {
  font-weight: 600;
  padding: 0;
  margin-bottom: 8px;
}
.dependency-relationships label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-sm);
}
.dependency-relationships-all {
  font-weight: 600;
}
.dependency-relationships p {
  flex-basis: 100%;
  margin: 0;
  font-size: var(--text-xs);
}
/* A definite width (not only a flex-basis) so the row's intrinsic width
   includes it; otherwise a narrower label font wraps the button below. */
.dependency-depth-picker {
  display: grid;
  gap: 4px;
  flex: none;
  width: 7rem;
  min-width: 0;
  font-size: var(--text-sm);
}
.dependency-range-options > .btn {
  flex: none;
  white-space: nowrap;
}
.dependency-range-options {
  display: flex;
  align-items: end;
  flex-wrap: wrap;
  gap: 10px;
}
.dependency-range-help {
  flex-basis: 100%;
  margin: 0;
  font-size: var(--text-xs);
}
.dependency-depth-picker :deep(.input) {
  box-sizing: border-box;
  height: var(--control-height);
  min-height: var(--control-height);
}
.dependency-help {
  margin: 0;
  font-size: var(--text-sm);
}
.dependency-check > p {
  margin: 0 0 8px;
}
.repair-preview {
  border: 1px solid var(--border);
  border-left: 3px solid var(--accent);
  padding: 16px;
  border-radius: var(--radius-sm);
  scroll-margin-top: calc(var(--control-height) * 3);
}
.repair-preview h2 {
  margin: 0;
  font-size: 1em;
}
.repair-preview p {
  margin: 8px 0;
  font-size: 0.9em;
}
.repair-preview > button {
  margin: 8px 8px 0 0;
}
.repair-tasks {
  list-style: none;
  margin: 8px 0;
  padding: 0;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.repair-tasks li {
  padding: 3px 8px;
  background: var(--surface-contrast);
  border-radius: var(--radius-sm);
  font-size: 0.85em;
}
.integrity-details {
  margin: 8px 0;
  font-size: 0.9em;
}
.integrity-details summary {
  cursor: pointer;
  color: var(--muted);
}
.integrity-details ul {
  max-height: 220px;
  overflow: auto;
  padding-left: 20px;
}
@media (max-width: 640px) {
  .dependency-relationships {
    flex-basis: 100%;
  }
  .dependency-range-options {
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  .dependency-depth-picker {
    width: auto;
  }
}
</style>
