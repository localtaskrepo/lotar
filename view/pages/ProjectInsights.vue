<template>
  <section class="col insights" style="gap: 16px">
    <PageHeader
      clear-label="Clear filters"
      :can-clear="hasFilters"
      reload-label="Refresh insights"
      :reloading="loadingTasks"
      @clear="clearFilters"
      @reload="refreshTasks()"
    >
      <template #title>
        Insights
        <span v-if="project" class="muted app-page-header__project" :title="projectLabel(project)"
          >— {{ projectName(project) }}</span
        >
      </template>
      <template #subtitle>{{ subtitle }}</template>
    </PageHeader>

    <div class="filter-card">
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :sprint-options="sprintFilterOptions"
        :tag-options="filterOptions.tags"
        :assignee-options="filterOptions.assignees"
        :custom-field-values="filterOptions.customFieldValues"
        :custom-field-options="availableCustomFields"
        :value="filterPayload"
        :show-order="false"
        emit-project-key
        storage-key="lotar.insights.filter"
        @update:value="onFilterUpdate"
      >
        <template #actions>
          <label class="toolbar-field">
            <span class="toolbar-field__label">Period</span>
            <select
              class="input toolbar-select"
              data-testid="insights-period"
              :value="period"
              @change="setPeriod(($event.target as HTMLSelectElement).value)"
            >
              <option v-for="option in PERIOD_OPTIONS" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
              <option value="custom">Custom range…</option>
            </select>
          </label>
          <template v-if="period === 'custom'">
            <label class="toolbar-field">
              <span class="toolbar-field__label">From</span>
              <input
                type="date"
                class="input toolbar-date"
                data-testid="insights-from"
                :value="customFrom"
                :max="customTo || undefined"
                @change="setCustomDay('from', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="toolbar-field">
              <span class="toolbar-field__label">To</span>
              <input
                type="date"
                class="input toolbar-date"
                data-testid="insights-to"
                :value="customTo"
                :min="customFrom || undefined"
                @change="setCustomDay('to', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </template>
        </template>
      </FilterBar>
    </div>

    <nav class="insights-tabs" role="tablist" aria-label="Insight views">
      <button
        v-for="lens in LENSES"
        :id="`insights-tab-${lens.key}`"
        :key="lens.key"
        type="button"
        role="tab"
        class="insights-tab"
        :class="{ active: activeLens === lens.key }"
        :aria-selected="activeLens === lens.key"
        :aria-controls="activeLens === lens.key ? `insights-panel-${lens.key}` : undefined"
        :tabindex="activeLens === lens.key ? 0 : -1"
        :data-testid="`insights-tab-${lens.key}`"
        @click="setLens(lens.key)"
        @keydown.right.prevent="stepLens(1)"
        @keydown.left.prevent="stepLens(-1)"
        @keydown.home.prevent="focusLens(LENSES[0]!.key)"
        @keydown.end.prevent="focusLens(LENSES[LENSES.length - 1]!.key)"
      >
        {{ lens.label }}
      </button>
    </nav>

    <div v-if="loadError && !tasks.length" class="card refresh-error" role="alert">
      <span>Insights could not load: {{ loadError }}</span>
      <UiButton variant="ghost" type="button" @click="refreshTasks()">Retry</UiButton>
    </div>
    <div v-else-if="initialLoading" style="margin: 12px 0"><UiLoader>Loading insights…</UiLoader></div>

    <div
      v-else
      :id="`insights-panel-${activeLens}`"
      class="insights-panel"
      :class="{ 'insights-panel--refreshing': loadingTasks }"
      role="tabpanel"
      :aria-labelledby="`insights-tab-${activeLens}`"
    >
      <p class="insights-lens-note muted">{{ activeLensInfo.note }}</p>
      <p v-if="customInvalid" class="insights-callout" role="status">
        Pick a start and end date for the custom range. Showing the last 30 days until then.
      </p>

      <component :is="activeLensInfo.component" />
    </div>
  </section>
</template>

<script setup lang="ts">
import { useProjectNames } from '../composables/useProjectNames'
import { computed, onMounted, onUnmounted, ref, shallowRef, watch, type Component } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import { fmt, provideInsights, toDay, type PeriodChoice } from '../components/insights/insightsContext'
import DeliveryLens from '../components/insights/lenses/DeliveryLens.vue'
import MyWorkLens from '../components/insights/lenses/MyWorkLens.vue'
import OverviewLens from '../components/insights/lenses/OverviewLens.vue'
import PlanningLens from '../components/insights/lenses/PlanningLens.vue'
import QualityLens from '../components/insights/lenses/QualityLens.vue'
import TeamLens from '../components/insights/lenses/TeamLens.vue'
import PageHeader from '../components/PageHeader.vue'
import UiButton from '../components/UiButton.vue'
import UiLoader from '../components/UiLoader.vue'
import { useConfig } from '../composables/useConfig'
import { buildServerFilter, useProjectFilterSync } from '../composables/useFilterBuilder'
import { useFilterOptions } from '../composables/useFilterOptions'
import { useSprintFilterOptions, useSprints } from '../composables/useSprints'
import { useTaskStore, type TaskQueryHandle } from '../composables/useTaskStore'
import { isPeriodKey, PERIOD_OPTIONS } from '../utils/insightsMetrics'
import { storageGet, storageSet } from '../utils/storage'

defineOptions({ name: 'ProjectInsights' })

type LensKey = 'overview' | 'delivery' | 'team' | 'planning' | 'quality' | 'mine'

const { projectName, projectLabel } = useProjectNames()
const LENSES: Array<{ key: LensKey; label: string; note: string; component: Component }> = [
  {
    key: 'overview',
    label: 'Overview',
    note: 'The headline numbers for leadership: how much is open, what got done and what needs attention.',
    component: OverviewLens,
  },
  {
    key: 'delivery',
    label: 'Delivery',
    note: 'For engineering managers and scrum masters: throughput, cycle time and work that is getting old.',
    component: DeliveryLens,
  },
  {
    key: 'team',
    label: 'Team',
    note: 'For team leads: who is carrying how much work, what each person finished and where help is needed.',
    component: TeamLens,
  },
  {
    key: 'planning',
    label: 'Planning',
    note: 'For product and project managers: deadlines, estimates and where the open scope sits.',
    component: PlanningLens,
  },
  {
    key: 'quality',
    label: 'Quality',
    note: 'For QA and support: bugs reported against bugs fixed, and how long fixes take.',
    component: QualityLens,
  },
  {
    key: 'mine',
    label: 'My work',
    note: 'Your own open work, deadlines and what you completed.',
    component: MyWorkLens,
  },
]

/** URL keys holding this page's view state; they survive project changes. */
const VIEW_QUERY_KEYS = ['view', 'period', 'from', 'to']

const route = useRoute()
const router = useRouter()
const store = useTaskStore()
const {
  statuses,
  priorities,
  types,
  customFields: availableCustomFields,
  refresh: refreshConfig,
  scope: configScope,
  tags: configuredTags,
  members,
} = useConfig()
const { sprints, refresh: refreshSprints } = useSprints()
const sprintFilterOptions = useSprintFilterOptions(sprints)

// -- Filters and data --------------------------------------------------------
const project = ref<string>(typeof route.query.project === 'string' ? route.query.project : '')
const filter = ref<Record<string, string>>({})
const filterBarRef = ref<{ clear?: () => void } | null>(null)
const filterPayload = computed(() => ({
  ...filter.value,
  ...(project.value || Object.prototype.hasOwnProperty.call(route.query, 'project') ? { project: project.value } : {}),
}))
const {
  hasFilters,
  onFilterUpdate,
  clearFilters: clearFiltersAction,
} = useProjectFilterSync(project, filter, {
  routePath: '/insights',
  keepQuery: VIEW_QUERY_KEYS,
})

// One live task query (shared store, all pages, kept current over SSE).
const query = shallowRef<TaskQueryHandle | null>(null)
function adoptQuery(handle: TaskQueryHandle) {
  if (query.value?.key === handle.key) return
  query.value?.release()
  handle.retain()
  query.value = handle
}
const tasks = computed<TaskDTO[]>(() => (query.value?.tasks.value ?? []).filter((task) => !task.deleted_at))
const loadingTasks = computed(() => query.value?.status.value === 'loading')
const loadError = computed(() => query.value?.error.value ?? null)
const initialLoading = computed(() => !query.value || (loadingTasks.value && !query.value.hasSnapshot.value))

const filterOptions = useFilterOptions(
  () => tasks.value,
  () => project.value,
  { scope: () => configScope.value, tags: () => configuredTags.value, members: () => members.value },
)

let disposed = false
async function refreshTasks() {
  if (disposed) return
  const { serverFilter } = buildServerFilter(filter.value, project.value)
  adoptQuery(store.getQuery(serverFilter))
  await query.value!.refresh()
}

let filterDebounce: ReturnType<typeof setTimeout> | null = null
watch(filter, () => {
  if (filterDebounce) clearTimeout(filterDebounce)
  filterDebounce = setTimeout(() => void refreshTasks(), 150)
})
watch(project, async (value) => {
  await refreshConfig(value)
  await refreshTasks()
})

function clearFilters() {
  clearFiltersAction(filterBarRef)
}

// -- View state: lens and period (URL first, then this browser) --------------
const isPeriodChoice = (value: unknown): value is PeriodChoice => value === 'custom' || isPeriodKey(value)
const isDay = (value: unknown): value is string => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value)

function readLens(): LensKey {
  const candidate = route.query.view ?? storageGet('lotar.insights.view')
  return LENSES.some((lens) => lens.key === candidate) ? (candidate as LensKey) : 'overview'
}
function readPeriod(): PeriodChoice {
  const candidate = route.query.period ?? storageGet('lotar.insights.period')
  return isPeriodChoice(candidate) ? candidate : '30d'
}
function readDay(key: 'from' | 'to'): string {
  const candidate = route.query[key] ?? storageGet(`lotar.insights.${key}`)
  return isDay(candidate) ? candidate : ''
}
const activeLens = ref<LensKey>(readLens())
const period = ref<PeriodChoice>(readPeriod())
const customFrom = ref(readDay('from'))
const customTo = ref(readDay('to'))
const activeLensInfo = computed(() => LENSES.find((lens) => lens.key === activeLens.value)!)

/** Only ever rewrite the URL while this page is the one being shown (DEV-187). */
function writeViewQuery() {
  if (disposed || route.path !== '/insights') return
  const next: Record<string, string> = {}
  for (const [key, value] of Object.entries(route.query)) {
    if (typeof value === 'string' && !VIEW_QUERY_KEYS.includes(key)) next[key] = value
  }
  next.view = activeLens.value
  next.period = period.value
  if (period.value === 'custom') {
    if (customFrom.value) next.from = customFrom.value
    if (customTo.value) next.to = customTo.value
  }
  const same = VIEW_QUERY_KEYS.every((key) => (route.query[key] ?? undefined) === next[key])
  if (!same) void router.replace({ path: '/insights', query: next })
}
function setLens(key: LensKey) {
  activeLens.value = key
  storageSet('lotar.insights.view', key)
  writeViewQuery()
}
function focusLens(key: LensKey) {
  setLens(key)
  document.getElementById(`insights-tab-${key}`)?.focus()
}
function stepLens(direction: number) {
  const index = LENSES.findIndex((lens) => lens.key === activeLens.value)
  focusLens(LENSES[(index + direction + LENSES.length) % LENSES.length]!.key)
}
function setPeriod(value: string) {
  if (!isPeriodChoice(value)) return
  if (value === 'custom' && !insights.customRange.value) {
    // Start from the range that was showing, so the switch changes nothing yet.
    const showing = insights.range.value
    customFrom.value = toDay(showing.start)
    customTo.value = toDay(Math.min(showing.end, Date.now()) - 1)
    storageSet('lotar.insights.from', customFrom.value)
    storageSet('lotar.insights.to', customTo.value)
  }
  period.value = value
  storageSet('lotar.insights.period', value)
  writeViewQuery()
}
function setCustomDay(key: 'from' | 'to', value: string) {
  if (value && !isDay(value)) return
  if (key === 'from') customFrom.value = value
  else customTo.value = value
  storageSet(`lotar.insights.${key}`, value)
  writeViewQuery()
}
watch(
  () => [route.query.view, route.query.period, route.query.from, route.query.to],
  () => {
    if (disposed || route.path !== '/insights') return
    activeLens.value = readLens()
    period.value = readPeriod()
    customFrom.value = readDay('from')
    customTo.value = readDay('to')
  },
)

// -- Identity for "My work" and the clock -----------------------------------------
const me = ref('')
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null
watch(tasks, () => (now.value = Date.now()))

// -- Shared metrics for the lens components -----------------------------------------
const insights = provideInsights({
  tasks,
  project,
  filter,
  statuses,
  priorities,
  types,
  period,
  customFrom,
  customTo,
  me,
  now,
})
const customInvalid = computed(() => period.value === 'custom' && !insights.customRange.value)
const subtitle = computed(() => {
  const scope = project.value ? `project ${projectName(project.value)}` : 'all projects'
  const label = insights.periodLabel.value
  return `${fmt(tasks.value.length)} tasks in ${scope} · ${period.value === 'custom' ? label : label.toLowerCase()}`
})

// -- Lifecycle --------------------------------------------------------------
onMounted(async () => {
  clock = setInterval(() => (now.value = Date.now()), 60_000)
  void api
    .whoami()
    .then((value) => {
      if (!disposed) me.value = (value || '').trim()
    })
    .catch(() => undefined)
  void Promise.resolve(refreshSprints()).catch(() => undefined)
  await refreshConfig(project.value)
  await refreshTasks()
  writeViewQuery()
})

onUnmounted(() => {
  disposed = true
  if (filterDebounce) clearTimeout(filterDebounce)
  if (clock) clearInterval(clock)
  query.value?.release()
  query.value = null
})
</script>

<style scoped>
.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.insights-tabs {
  display: flex;
  gap: var(--space-1, 0.25rem);
  border-bottom: 1px solid var(--color-border);
  overflow-x: auto;
}

.insights-tab {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--color-muted);
  font: inherit;
  font-size: var(--text-sm, 0.875rem);
  white-space: nowrap;
  cursor: pointer;
}

.insights-tab:hover {
  color: var(--color-fg);
}

.insights-tab.active {
  border-bottom-color: var(--color-accent);
  color: var(--color-fg);
  font-weight: 600;
}

.insights-tab:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.insights-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
  transition: opacity 150ms ease;
}

.insights-panel--refreshing {
  opacity: 0.6;
}

.insights-lens-note {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}

.toolbar-date {
  box-sizing: border-box;
  height: var(--control-height);
  min-height: var(--control-height);
  padding: 0 var(--space-2, 0.5rem);
  font-size: var(--text-sm, 0.875rem);
}

.refresh-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-left: 3px solid var(--color-danger);
}
</style>

<!-- Layout shared by the lens components, scoped to this page by its root class. -->
<style>
.insights .stat-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: var(--space-3, 0.75rem);
}

.insights .chart-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-3, 0.75rem);
}

.insights .chart-grid > .span-2 {
  grid-column: span 2;
}

@media (max-width: 900px) {
  .insights .chart-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .insights .chart-grid > .span-2 {
    grid-column: auto;
  }
}

.insights .insights-callout {
  margin: 0;
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border-left: 3px solid var(--color-warning);
  background: color-mix(in oklab, var(--color-warning) 10%, transparent);
  font-size: var(--text-sm, 0.875rem);
}

.insights .insights-footnote {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}
</style>
