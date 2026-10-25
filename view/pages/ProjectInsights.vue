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
        <span v-if="project" class="muted">— {{ project }}</span>
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
        :custom-presets="customFilterPresets"
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
            </select>
          </label>
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
        :aria-controls="`insights-panel-${lens.key}`"
        :tabindex="activeLens === lens.key ? 0 : -1"
        :data-testid="`insights-tab-${lens.key}`"
        @click="setLens(lens.key)"
        @keydown.right.prevent="stepLens(1)"
        @keydown.left.prevent="stepLens(-1)"
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

      <!-- Overview: leadership summary -->
      <template v-if="activeLens === 'overview'">
        <div class="stat-row">
          <StatTile label="Open" :value="fmt(open.length)" caption="Right now" clickable @select="openTasks({})" />
          <StatTile
            label="Completed"
            :value="fmt(completed.length)"
            :delta="delta(completed.length, completedPrev.length, 'up')"
          />
          <StatTile
            label="Created"
            :value="fmt(created.length)"
            :delta="delta(created.length, createdPrev.length, null)"
          />
          <StatTile
            label="Median lead time"
            :value="formatDays(leadP50)"
            hint="Creation to done, for work completed in the period"
            :delta="durationDelta(leadP50, leadP50Prev)"
          />
          <StatTile
            label="Overdue"
            :value="fmt(overdue.length)"
            :tone="overdue.length ? 'alert' : 'neutral'"
            caption="Right now"
            clickable
            @select="openTasks({ due: 'overdue' })"
          />
          <StatTile
            label="On time"
            :value="onTimeRate === null ? '—' : pct(onTimeRate)"
            :caption="onTimeCaption"
            hint="Completed tasks with a due date that finished by the end of their due day"
          />
        </div>
        <div class="chart-grid">
          <ChartCard
            class="span-2"
            title="Created vs completed"
            :subtitle="`Per ${bucketUnit}, ${periodLabel.toLowerCase()}`"
            :empty="!created.length && !completed.length"
          >
            <ColumnChart
              :labels="bucketLabels"
              :series="flowSeries"
              :summary="`Created and completed tasks per ${bucketUnit}`"
            />
            <template #table>
              <SeriesTable :labels="bucketLabels" :series="flowSeries" />
            </template>
          </ChartCard>
          <ChartCard
            title="Needs attention"
            subtitle="Open work that may be stuck or at risk"
            :empty="!attention.length"
            empty-text="Nothing needs attention."
          >
            <BarList :rows="attention" label="Needs attention" clickable @select="openAttention" />
          </ChartCard>
          <ChartCard title="Open by status" :empty="!open.length">
            <BarList
              :rows="openByStatus"
              label="Open tasks by status"
              clickable
              @select="(key) => openTasks({ status: key })"
            />
          </ChartCard>
          <ChartCard title="Open by priority" :empty="!open.length">
            <BarList
              :rows="openByPriority"
              label="Open tasks by priority"
              clickable
              @select="(key) => openTasks({ priority: key })"
            />
          </ChartCard>
          <ChartCard title="Open by type" :empty="!open.length">
            <BarList
              :rows="openByType"
              label="Open tasks by type"
              clickable
              @select="(key) => openTasks({ type: key })"
            />
          </ChartCard>
        </div>
      </template>

      <!-- Delivery: flow and speed -->
      <template v-else-if="activeLens === 'delivery'">
        <div class="stat-row">
          <StatTile
            :label="`Throughput per ${bucketUnit}`"
            :value="fmt(throughputPerBucket, 1)"
            :delta="delta(completed.length, completedPrev.length, 'up')"
          />
          <StatTile label="In progress" :value="fmt(wip.length)" caption="Started and not done" />
          <StatTile
            label="Cycle time (median)"
            :value="formatDays(cycleP50)"
            hint="First status change to done"
            :delta="durationDelta(cycleP50, cycleP50Prev)"
          />
          <StatTile
            label="Cycle time (85th pct.)"
            :value="formatDays(cycleP85)"
            hint="85% of work finished within this time"
          />
          <StatTile label="Lead time (median)" :value="formatDays(leadP50)" hint="Creation to done" />
        </div>
        <div class="chart-grid">
          <ChartCard
            class="span-2"
            title="Throughput"
            :subtitle="`Tasks completed per ${bucketUnit}`"
            :empty="!completed.length"
          >
            <ColumnChart
              :labels="bucketLabels"
              :series="throughputSeries"
              :summary="`Completed tasks per ${bucketUnit}`"
            />
            <template #table>
              <SeriesTable :labels="bucketLabels" :series="throughputSeries" />
            </template>
          </ChartCard>
          <ChartCard
            title="Cycle time distribution"
            subtitle="Work completed in the period"
            :empty="!cycleValues.length"
            empty-text="No completed work with a recorded start."
          >
            <ColumnChart
              :labels="cycleBinLabels"
              :series="cycleBinSeries"
              summary="Number of completed tasks per cycle time range"
            />
            <template #table>
              <SeriesTable :labels="cycleBinLabels" :series="cycleBinSeries" />
            </template>
          </ChartCard>
          <ChartCard
            title="Time in status"
            subtitle="Average per task completed in the period"
            :empty="!statusTimes.length"
          >
            <BarList :rows="statusTimes" label="Average time in status" />
          </ChartCard>
          <ChartCard
            class="span-2"
            title="Aging work in progress"
            subtitle="Longest in their current status; flagged when older than the 85th percentile cycle time"
            :empty="!wip.length"
            empty-text="Nothing is in progress."
          >
            <TaskTable :rows="agingRows" :columns="agingColumns" @open="openTask" />
          </ChartCard>
        </div>
        <p class="insights-footnote muted">
          Sprint burndown, velocity and health live on the
          <RouterLink :to="{ path: '/sprints', query: project ? { project } : {} }">Sprints</RouterLink> page.
        </p>
      </template>

      <!-- Team: workload and contribution -->
      <template v-else-if="activeLens === 'team'">
        <div class="stat-row">
          <StatTile label="People with open work" :value="fmt(assignees.length)" />
          <StatTile
            label="Unassigned"
            :value="fmt(unassignedOpen.length)"
            :tone="unassignedOpen.length ? 'alert' : 'neutral'"
            caption="Open tasks without an assignee"
            clickable
            @select="openTasks({ assignee: '__none__' })"
          />
          <StatTile
            label="Active contributors"
            :value="fmt(activity.length)"
            :caption="`Made changes, ${periodLabel.toLowerCase()}`"
          />
          <StatTile label="Open per person" :value="fmt(openPerPerson, 1)" caption="Average across assignees" />
        </div>
        <ChartCard
          title="Workload"
          subtitle="Open work per person and what they finished in the period"
          :empty="!people.length"
        >
          <div class="people-table">
            <table>
              <thead>
                <tr>
                  <th scope="col">Person</th>
                  <th scope="col" class="numeric">Open</th>
                  <th scope="col" class="numeric">In progress</th>
                  <th scope="col" class="numeric">Overdue</th>
                  <th scope="col" class="numeric">Completed</th>
                  <th scope="col" class="numeric">Cycle time</th>
                  <th scope="col" class="numeric">Open effort</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="person in people" :key="person.key">
                  <td>
                    <button type="button" class="link-button" @click="openTasks({ assignee: person.filter })">
                      {{ person.label }}
                    </button>
                  </td>
                  <td class="numeric">{{ person.open }}</td>
                  <td class="numeric">{{ person.inProgress }}</td>
                  <td class="numeric" :class="{ alert: person.overdue > 0 }">{{ person.overdue }}</td>
                  <td class="numeric">{{ person.completed }}</td>
                  <td class="numeric">{{ formatDays(person.cycle) }}</td>
                  <td class="numeric">{{ person.effort }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </ChartCard>
        <div class="chart-grid">
          <ChartCard title="Completed per person" :subtitle="periodLabel" :empty="!completedByPerson.length">
            <BarList
              :rows="completedByPerson"
              label="Completed tasks per person"
              clickable
              @select="(key) => openTasks({ assignee: key === 'Unassigned' ? '__none__' : key })"
            />
          </ChartCard>
          <ChartCard
            title="Activity per person"
            subtitle="Status changes, edits and comments"
            :empty="!activity.length"
          >
            <BarList :rows="activityRows" label="Changes per person" />
          </ChartCard>
        </div>
      </template>

      <!-- Planning: deadlines and scope -->
      <template v-else-if="activeLens === 'planning'">
        <div class="stat-row">
          <StatTile
            label="Overdue"
            :value="fmt(overdue.length)"
            :tone="overdue.length ? 'alert' : 'neutral'"
            clickable
            @select="openTasks({ due: 'overdue' })"
          />
          <StatTile
            label="Due within 7 days"
            :value="fmt(dueSoon.length)"
            clickable
            @select="openTasks({ due: 'soon' })"
          />
          <StatTile
            label="No due date"
            :value="fmt(noDueDate.length)"
            caption="Open tasks"
            clickable
            @select="openTasks({ needs: 'due' })"
          />
          <StatTile label="Open effort" :value="openEffortText" :caption="openEffortCaption" />
          <StatTile
            label="Unestimated"
            :value="fmt(open.length - openEffort.estimated)"
            caption="Open tasks without effort"
            clickable
            @select="openTasks({ needs: 'effort' })"
          />
        </div>
        <div class="chart-grid">
          <ChartCard title="Due date outlook" subtitle="Open work by deadline" :empty="!open.length">
            <BarList :rows="dueOutlook" label="Open work by deadline" clickable @select="openDueBucket" />
          </ChartCard>
          <ChartCard
            title="Scope by tag"
            subtitle="Open tasks per tag"
            :empty="!openByTag.length"
            empty-text="No tagged open work."
          >
            <BarList
              :rows="openByTag"
              label="Open tasks per tag"
              clickable
              @select="(key) => key !== 'No tag' && openTasks({ tags: key })"
            />
          </ChartCard>
          <ChartCard
            class="span-2"
            title="Upcoming deadlines"
            subtitle="Open work due in the next 14 days, and anything overdue"
            :empty="!upcoming.length"
            empty-text="No deadlines in the next 14 days."
          >
            <TaskTable :rows="upcomingRows" :columns="upcomingColumns" :limit="12" @open="openTask" />
          </ChartCard>
          <ChartCard class="span-2" title="Completed by type" :subtitle="periodLabel" :empty="!completed.length">
            <BarList :rows="completedByType" label="Completed tasks by type" />
          </ChartCard>
        </div>
      </template>

      <!-- Quality: bugs -->
      <template v-else-if="activeLens === 'quality'">
        <p v-if="!bugTypesPresent" class="insights-callout">
          No bug-type tasks match this view. Quality tracks tasks whose type is named like Bug, Defect, Incident or
          Regression.
        </p>
        <div class="stat-row">
          <StatTile
            label="Open bugs"
            :value="fmt(openBugs.length)"
            :tone="openBugs.length ? 'alert' : 'neutral'"
            :clickable="bugTypesPresent"
            @select="openTasks({ type: bugTypeFilter })"
          />
          <StatTile
            label="Reported"
            :value="fmt(bugsReported.length)"
            :delta="delta(bugsReported.length, bugsReportedPrev.length, 'down')"
          />
          <StatTile
            label="Fixed"
            :value="fmt(bugsFixed.length)"
            :delta="delta(bugsFixed.length, bugsFixedPrev.length, 'up')"
          />
          <StatTile label="Time to fix (median)" :value="formatDays(bugFixP50)" hint="Reported to done" />
          <StatTile
            label="Bug share"
            :value="bugShare === null ? '—' : pct(bugShare)"
            caption="Of new work in the period"
          />
        </div>
        <div class="chart-grid">
          <ChartCard
            class="span-2"
            title="Reported vs fixed"
            :subtitle="`Bugs per ${bucketUnit}`"
            :empty="!bugsReported.length && !bugsFixed.length"
          >
            <ColumnChart
              :labels="bucketLabels"
              :series="bugSeries"
              :summary="`Bugs reported and fixed per ${bucketUnit}`"
            />
            <template #table>
              <SeriesTable :labels="bucketLabels" :series="bugSeries" />
            </template>
          </ChartCard>
          <ChartCard title="Open bugs by priority" :empty="!openBugs.length">
            <BarList
              :rows="openBugsByPriority"
              label="Open bugs by priority"
              clickable
              @select="(key) => openTasks({ type: bugTypeFilter, priority: key })"
            />
          </ChartCard>
          <ChartCard title="Oldest open bugs" :empty="!openBugs.length" empty-text="No open bugs.">
            <TaskTable :rows="oldestBugRows" :columns="oldestBugColumns" @open="openTask" />
          </ChartCard>
        </div>
      </template>

      <!-- My work: the signed-in person -->
      <template v-else>
        <p v-if="!me" class="insights-callout">
          Your identity could not be resolved. Set a default reporter or a git user name to see your own work here.
        </p>
        <template v-else>
          <div class="stat-row">
            <StatTile
              label="Assigned to me"
              :value="fmt(myOpen.length)"
              caption="Open"
              clickable
              @select="openTasks({ assignee: '@me' })"
            />
            <StatTile label="In progress" :value="fmt(myWip.length)" />
            <StatTile
              label="Overdue"
              :value="fmt(myOverdue.length)"
              :tone="myOverdue.length ? 'alert' : 'neutral'"
              clickable
              @select="openTasks({ assignee: '@me', due: 'overdue' })"
            />
            <StatTile label="Due within 7 days" :value="fmt(myDueSoon.length)" />
            <StatTile label="Completed" :value="fmt(myCompleted.length)" :caption="periodLabel" />
          </div>
          <div class="chart-grid">
            <ChartCard
              class="span-2"
              title="My open work"
              subtitle="Soonest deadline first, then priority"
              :empty="!myOpen.length"
              empty-text="Nothing assigned to you is open."
            >
              <TaskTable :rows="myOpenRows" :columns="myOpenColumns" :limit="15" @open="openTask" />
            </ChartCard>
            <ChartCard
              class="span-2"
              title="Recently completed"
              :subtitle="periodLabel"
              :empty="!myCompleted.length"
              empty-text="Nothing completed in this period."
            >
              <TaskTable :rows="myCompletedRows" :columns="myCompletedColumns" @open="openTask" />
            </ChartCard>
          </div>
        </template>
      </template>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, defineComponent, h, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { TaskDTO } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import BarList, { type BarRow } from '../components/insights/BarList.vue'
import ChartCard from '../components/insights/ChartCard.vue'
import ColumnChart from '../components/insights/ColumnChart.vue'
import StatTile, { type StatDelta } from '../components/insights/StatTile.vue'
import TaskTable, { type TaskTableRow } from '../components/insights/TaskTable.vue'
import PageHeader from '../components/PageHeader.vue'
import UiButton from '../components/UiButton.vue'
import UiLoader from '../components/UiLoader.vue'
import { useCompletionPolicy } from '../composables/useCompletionPolicy'
import { useConfig } from '../composables/useConfig'
import { buildServerFilter, useCustomFilterPresets, useProjectFilterSync } from '../composables/useFilterBuilder'
import { useFilterOptions } from '../composables/useFilterOptions'
import { useSprintFilterOptions, useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { useTaskStore, type TaskQueryHandle } from '../composables/useTaskStore'
import {
  activityByActor,
  bucketsFor,
  buildTimeline,
  changeRatio,
  countBy,
  countPerBucket,
  cycleDays,
  DAY_MS,
  daysInCurrentStatus,
  durationHistogram,
  effortTotals,
  formatDays,
  inRange,
  isBugType,
  isPeriodKey,
  leadDays,
  metDueDate,
  orderByConfigured,
  percentile,
  PERIOD_OPTIONS,
  periodRange,
  previousRange,
  timeInStatus,
  type CompletionLookup,
  type CountEntry,
  type PeriodKey,
  type TaskTimeline,
} from '../utils/insightsMetrics'
import { storageGet, storageSet } from '../utils/storage'

defineOptions({ name: 'ProjectInsights' })

type LensKey = 'overview' | 'delivery' | 'team' | 'planning' | 'quality' | 'mine'

const LENSES: Array<{ key: LensKey; label: string; note: string }> = [
  {
    key: 'overview',
    label: 'Overview',
    note: 'The headline numbers for leadership: how much is open, what got done and what needs attention.',
  },
  {
    key: 'delivery',
    label: 'Delivery',
    note: 'For engineering managers and scrum masters: throughput, cycle time and work that is getting old.',
  },
  {
    key: 'team',
    label: 'Team',
    note: 'For team leads: who is carrying how much work, what each person finished and where help is needed.',
  },
  {
    key: 'planning',
    label: 'Planning',
    note: 'For product and project managers: deadlines, estimates and where the open scope sits.',
  },
  {
    key: 'quality',
    label: 'Quality',
    note: 'For QA and support: bugs reported against bugs fixed, and how long fixes take.',
  },
  { key: 'mine', label: 'My work', note: 'Your own open work, deadlines and what you completed.' },
]

const route = useRoute()
const router = useRouter()
const store = useTaskStore()
const { openTaskPanel } = useTaskPanelController()
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
})
const customFilterPresets = useCustomFilterPresets(availableCustomFields)

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
function readLens(): LensKey {
  const candidate = route.query.view ?? storageGet('lotar.insights.view')
  return LENSES.some((lens) => lens.key === candidate) ? (candidate as LensKey) : 'overview'
}
function readPeriod(): PeriodKey {
  const candidate = route.query.period ?? storageGet('lotar.insights.period')
  return isPeriodKey(candidate) ? candidate : '30d'
}
const activeLens = ref<LensKey>(readLens())
const period = ref<PeriodKey>(readPeriod())
const activeLensInfo = computed(() => LENSES.find((lens) => lens.key === activeLens.value)!)

/** Only ever rewrite the URL while this page is the one being shown (DEV-187). */
function writeViewQuery() {
  if (disposed || route.path !== '/insights') return
  if (route.query.view === activeLens.value && route.query.period === period.value) return
  void router.replace({ path: '/insights', query: { ...route.query, view: activeLens.value, period: period.value } })
}
function setLens(key: LensKey) {
  activeLens.value = key
  storageSet('lotar.insights.view', key)
  writeViewQuery()
}
function stepLens(direction: number) {
  const index = LENSES.findIndex((lens) => lens.key === activeLens.value)
  const next = LENSES[(index + direction + LENSES.length) % LENSES.length]!
  setLens(next.key)
  document.getElementById(`insights-tab-${next.key}`)?.focus()
}
function setPeriod(value: string) {
  if (!isPeriodKey(value)) return
  period.value = value
  storageSet('lotar.insights.period', value)
  writeViewQuery()
}
watch(
  () => [route.query.view, route.query.period],
  () => {
    if (disposed || route.path !== '/insights') return
    activeLens.value = readLens()
    period.value = readPeriod()
  },
)

// -- Identity for "My work" --------------------------------------------------
const me = ref('')

// -- Time ---------------------------------------------------------------------
const now = ref(Date.now())
let clock: ReturnType<typeof setInterval> | null = null
watch(tasks, () => (now.value = Date.now()))

const range = computed(() => periodRange(period.value, now.value, tasks.value))
const prevRange = computed(() => previousRange(range.value))
const comparable = computed(() => period.value !== 'all')
const periodLabel = computed(() => PERIOD_OPTIONS.find((option) => option.value === period.value)!.label)
const buckets = computed(() => bucketsFor(range.value))
const bucketLabels = computed(() => buckets.value.map((bucket) => bucket.label))
const bucketUnit = computed(() => {
  const span = (range.value.end - range.value.start) / DAY_MS
  return span <= 14 ? 'day' : span <= 120 ? 'week' : 'month'
})

const subtitle = computed(() => {
  const scope = project.value ? `project ${project.value}` : 'all projects'
  return `${fmt(tasks.value.length)} tasks in ${scope} · ${periodLabel.value.toLowerCase()}`
})

// -- Timelines and core sets ------------------------------------------------
const completion = useCompletionPolicy({ tasks: () => tasks.value })
const lookup: CompletionLookup = {
  isTaskDone: (task) => completion.isTaskDone(task),
  isDoneStatus: (prefix, status) => completion.isDoneStatus(prefix, status),
}
const timelines = computed<TaskTimeline[]>(() => tasks.value.map((task) => buildTimeline(task, lookup)))
const openTimelines = computed(() => timelines.value.filter((timeline) => !timeline.isDone))
const open = computed(() => openTimelines.value.map((timeline) => timeline.task))
const completed = computed(() => timelines.value.filter((timeline) => inRange(timeline.doneAt, range.value)))
const completedPrev = computed(() => timelines.value.filter((timeline) => inRange(timeline.doneAt, prevRange.value)))
const created = computed(() => timelines.value.filter((timeline) => inRange(timeline.created, range.value)))
const createdPrev = computed(() => timelines.value.filter((timeline) => inRange(timeline.created, prevRange.value)))
const overdue = computed(() => open.value.filter((task) => completion.isTaskOverdue(task)))
const dueSoon = computed(() =>
  open.value.filter((task) => ['today', 'soon'].includes(completion.dueBucketFor(task) ?? '')),
)
const noDueDate = computed(() => open.value.filter((task) => !task.due_date))

/** Started (left its initial status and not moved back) and not done. */
function isInProgress(timeline: TaskTimeline) {
  if (timeline.isDone || timeline.startedAt === null) return false
  const first = (timeline.task.history ?? [])
    .flatMap((entry) => entry.changes ?? [])
    .find((change) => change.field === 'status')
  return !first?.old || first.old !== timeline.task.status
}
const wip = computed(() => openTimelines.value.filter(isInProgress))

// -- Durations -----------------------------------------------------------------
const values = (list: TaskTimeline[], pick: (timeline: TaskTimeline) => number | null) =>
  list.map(pick).filter((value): value is number => value !== null)
const cycleValues = computed(() => values(completed.value, cycleDays))
const leadValues = computed(() => values(completed.value, leadDays))
const cycleP50 = computed(() => percentile(cycleValues.value, 50))
const cycleP85 = computed(() => percentile(cycleValues.value, 85))
const cycleP50Prev = computed(() => percentile(values(completedPrev.value, cycleDays), 50))
const leadP50 = computed(() => percentile(leadValues.value, 50))
const leadP50Prev = computed(() => percentile(values(completedPrev.value, leadDays), 50))
const cycleHistogram = computed(() => durationHistogram(cycleValues.value))
const cycleBinLabels = computed(() => cycleHistogram.value.map((bin) => bin.label))
const cycleBinSeries = computed(() => [{ label: 'Tasks', values: cycleHistogram.value.map((bin) => bin.count) }])
const throughputPerBucket = computed(() => completed.value.length / Math.max(1, buckets.value.length))

const createdSeries = computed(() =>
  countPerBucket(
    created.value.map((timeline) => timeline.created),
    buckets.value,
  ),
)
const completedSeries = computed(() =>
  countPerBucket(
    completed.value.map((timeline) => timeline.doneAt),
    buckets.value,
  ),
)
const flowSeries = computed(() => [
  { label: 'Created', values: createdSeries.value },
  { label: 'Completed', values: completedSeries.value },
])
const throughputSeries = computed(() => [{ label: 'Completed', values: completedSeries.value }])

const onTimeResults = computed(() =>
  completed.value.map(metDueDate).filter((value): value is boolean => value !== null),
)
const onTimeRate = computed(() =>
  onTimeResults.value.length ? onTimeResults.value.filter(Boolean).length / onTimeResults.value.length : null,
)
const onTimeCaption = computed(() =>
  onTimeResults.value.length ? `${onTimeResults.value.length} with a due date` : 'No completed work had a due date',
)

// -- Breakdowns ----------------------------------------------------------------
function rows(entries: CountEntry[], mutedKey = '(none)'): BarRow[] {
  return entries.map((entry) => ({
    key: entry.key,
    label: entry.key,
    value: entry.count,
    muted: entry.key === mutedKey,
  }))
}
const openByStatus = computed(() =>
  rows(
    orderByConfigured(
      countBy(open.value, (task) => task.status),
      statuses.value,
    ),
  ),
)
const openByPriority = computed(() =>
  rows(
    orderByConfigured(
      countBy(open.value, (task) => task.priority),
      [...priorities.value].reverse(),
    ),
  ),
)
const openByType = computed(() =>
  rows(
    orderByConfigured(
      countBy(open.value, (task) => task.task_type),
      types.value,
    ),
  ),
)
const openByTag = computed(() =>
  rows(
    countBy(open.value, (task) => (task.tags?.length ? task.tags : null), 'No tag'),
    'No tag',
  ),
)
const completedByType = computed(() =>
  rows(
    countBy(
      completed.value.map((timeline) => timeline.task),
      (task) => task.task_type,
    ),
  ),
)

const staleCutoff = computed(() => now.value - 14 * DAY_MS)
const stale = computed(() => open.value.filter((task) => Date.parse(task.modified) < staleCutoff.value))
const unassignedOpen = computed(() => open.value.filter((task) => !task.assignee))
const topPriority = computed(() => priorities.value[priorities.value.length - 1] ?? '')
const urgentUnassigned = computed(() =>
  unassignedOpen.value.filter((task) => topPriority.value && task.priority === topPriority.value),
)
const attention = computed<BarRow[]>(() =>
  [
    { key: 'overdue', label: 'Overdue', value: overdue.value.length, alert: true },
    {
      key: 'urgent-unassigned',
      label: `${topPriority.value || 'Top priority'}, unassigned`,
      value: urgentUnassigned.value.length,
      alert: true,
    },
    {
      key: 'stale',
      label: 'No update in 14 days',
      value: stale.value.length,
      hint: 'Open tasks not modified for two weeks',
    },
    { key: 'unassigned', label: 'Unassigned', value: unassignedOpen.value.length },
  ].filter((row) => row.value > 0),
)
function openAttention(key: string) {
  if (key === 'overdue') openTasks({ due: 'overdue' })
  else if (key === 'urgent-unassigned') openTasks({ assignee: '__none__', priority: topPriority.value })
  else if (key === 'unassigned') openTasks({ assignee: '__none__' })
  else openTasks({})
}

const statusTimes = computed<BarRow[]>(() =>
  timeInStatus(timelines.value, range.value, lookup).map((entry) => ({
    key: entry.status,
    label: entry.status,
    value: entry.days,
    display: formatDays(entry.days),
    hint: `${entry.tasks} tasks`,
  })),
)

const agingColumns = [
  { key: 'status', label: 'Status' },
  { key: 'assignee', label: 'Assignee' },
  { key: 'age', label: 'In status', numeric: true },
]
const agingRows = computed<TaskTableRow[]>(() =>
  wip.value
    .map((timeline) => ({ task: timeline.task, age: daysInCurrentStatus(timeline.task, now.value) ?? 0 }))
    .sort((a, b) => b.age - a.age)
    .map(({ task, age }) => ({
      id: task.id,
      title: task.title,
      cells: { status: task.status, assignee: task.assignee || '—', age: formatDays(age) },
      alert: cycleP85.value !== null && age > cycleP85.value ? ['age'] : [],
    })),
)

// -- Team -----------------------------------------------------------------------
interface PersonRow {
  key: string
  label: string
  filter: string
  open: number
  inProgress: number
  overdue: number
  completed: number
  cycle: number | null
  effort: string
}
function effortText(list: TaskDTO[]): string {
  const totals = effortTotals(list)
  const parts = []
  if (totals.hours) parts.push(`${Math.round(totals.hours)}h`)
  if (totals.points) parts.push(`${Math.round(totals.points * 10) / 10}pt`)
  return parts.join(' + ') || '—'
}
const people = computed<PersonRow[]>(() => {
  const keys = new Set<string>()
  for (const task of open.value) keys.add(task.assignee || '')
  for (const timeline of completed.value) keys.add(timeline.task.assignee || '')
  const wipIds = new Set(wip.value.map((timeline) => timeline.task.id))
  const overdueIds = new Set(overdue.value.map((task) => task.id))
  return [...keys]
    .map((key) => {
      const mine = open.value.filter((task) => (task.assignee || '') === key)
      const done = completed.value.filter((timeline) => (timeline.task.assignee || '') === key)
      return {
        key: key || '(unassigned)',
        label: key || 'Unassigned',
        filter: key || '__none__',
        open: mine.length,
        inProgress: mine.filter((task) => wipIds.has(task.id)).length,
        overdue: mine.filter((task) => overdueIds.has(task.id)).length,
        completed: done.length,
        cycle: percentile(values(done, cycleDays), 50),
        effort: effortText(mine),
      }
    })
    .sort(
      (a, b) =>
        Number(a.label === 'Unassigned') - Number(b.label === 'Unassigned') ||
        b.open - a.open ||
        a.label.localeCompare(b.label),
    )
})
const assignees = computed(() => people.value.filter((person) => person.label !== 'Unassigned' && person.open > 0))
const openPerPerson = computed(() =>
  assignees.value.length ? assignees.value.reduce((sum, person) => sum + person.open, 0) / assignees.value.length : 0,
)
const completedByPerson = computed(() =>
  rows(
    countBy(
      completed.value.map((timeline) => timeline.task),
      (task) => task.assignee,
      'Unassigned',
    ),
    'Unassigned',
  ),
)
const activity = computed(() => activityByActor(tasks.value, range.value))
const activityRows = computed(() => rows(activity.value, '(unknown)'))

// -- Planning -------------------------------------------------------------------
const openEffort = computed(() => effortTotals(open.value))
const openEffortText = computed(() => effortText(open.value))
const openEffortCaption = computed(() => `${openEffort.value.estimated} of ${open.value.length} open tasks estimated`)
const dueOutlook = computed<BarRow[]>(() => {
  const counts = { overdue: 0, today: 0, soon: 0, later: 0 }
  for (const task of open.value) {
    const bucket = completion.dueBucketFor(task)
    if (bucket) counts[bucket] += 1
  }
  return [
    { key: 'overdue', label: 'Overdue', value: counts.overdue, alert: counts.overdue > 0 },
    { key: 'today', label: 'Due today', value: counts.today },
    { key: 'soon', label: 'Next 7 days', value: counts.soon },
    { key: 'later', label: 'Later', value: counts.later },
    { key: 'none', label: 'No due date', value: noDueDate.value.length, muted: true },
  ]
})
function openDueBucket(key: string) {
  if (key === 'none') openTasks({ needs: 'due' })
  else openTasks({ due: key })
}
const upcoming = computed(() =>
  open.value
    .filter((task) => {
      const bucket = completion.dueBucketFor(task)
      if (bucket === 'overdue' || bucket === 'today' || bucket === 'soon') return true
      const due = task.due_date ? Date.parse(task.due_date) : NaN
      return !Number.isNaN(due) && due <= now.value + 14 * DAY_MS
    })
    .sort((a, b) => (a.due_date ?? '').localeCompare(b.due_date ?? '')),
)
const upcomingColumns = [
  { key: 'due', label: 'Due' },
  { key: 'status', label: 'Status' },
  { key: 'priority', label: 'Priority' },
  { key: 'assignee', label: 'Assignee' },
]
const upcomingRows = computed<TaskTableRow[]>(() =>
  upcoming.value.map((task) => ({
    id: task.id,
    title: task.title,
    cells: {
      due: formatDue(task.due_date),
      status: task.status,
      priority: task.priority,
      assignee: task.assignee || '—',
    },
    alert: completion.isTaskOverdue(task) ? ['due'] : [],
  })),
)

// -- Quality --------------------------------------------------------------------
const isBug = (task: TaskDTO) => isBugType(task.task_type)
const bugTypes = computed(() => [...new Set(tasks.value.filter(isBug).map((task) => task.task_type))])
const bugTypesPresent = computed(() => bugTypes.value.length > 0)
const bugTypeFilter = computed(() => bugTypes.value.join(','))
const openBugs = computed(() => open.value.filter(isBug))
const bugsReported = computed(() => created.value.filter((timeline) => isBug(timeline.task)))
const bugsReportedPrev = computed(() => createdPrev.value.filter((timeline) => isBug(timeline.task)))
const bugsFixed = computed(() => completed.value.filter((timeline) => isBug(timeline.task)))
const bugsFixedPrev = computed(() => completedPrev.value.filter((timeline) => isBug(timeline.task)))
const bugFixP50 = computed(() => percentile(values(bugsFixed.value, leadDays), 50))
const bugShare = computed(() => (created.value.length ? bugsReported.value.length / created.value.length : null))
const bugSeries = computed(() => [
  {
    label: 'Reported',
    values: countPerBucket(
      bugsReported.value.map((timeline) => timeline.created),
      buckets.value,
    ),
  },
  {
    label: 'Fixed',
    values: countPerBucket(
      bugsFixed.value.map((timeline) => timeline.doneAt),
      buckets.value,
    ),
  },
])
const openBugsByPriority = computed(() =>
  rows(
    orderByConfigured(
      countBy(openBugs.value, (task) => task.priority),
      [...priorities.value].reverse(),
    ),
  ),
)
const oldestBugColumns = [
  { key: 'priority', label: 'Priority' },
  { key: 'assignee', label: 'Assignee' },
  { key: 'age', label: 'Age', numeric: true },
]
const oldestBugRows = computed<TaskTableRow[]>(() =>
  openBugs.value
    .map((task) => ({ task, age: (now.value - Date.parse(task.created)) / DAY_MS }))
    .sort((a, b) => b.age - a.age)
    .map(({ task, age }) => ({
      id: task.id,
      title: task.title,
      cells: { priority: task.priority, assignee: task.assignee || '—', age: formatDays(age) },
    })),
)

// -- My work --------------------------------------------------------------------
const isMine = (task: TaskDTO) => !!me.value && task.assignee === me.value
const myOpen = computed(() => open.value.filter(isMine))
const myWip = computed(() => wip.value.filter((timeline) => isMine(timeline.task)))
const myOverdue = computed(() => overdue.value.filter(isMine))
const myDueSoon = computed(() => dueSoon.value.filter(isMine))
const myCompleted = computed(() =>
  completed.value.filter((timeline) => isMine(timeline.task)).sort((a, b) => (b.doneAt ?? 0) - (a.doneAt ?? 0)),
)
const priorityRank = computed(() => new Map(priorities.value.map((label, index) => [label.toLowerCase(), index])))
const myOpenColumns = [
  { key: 'status', label: 'Status' },
  { key: 'priority', label: 'Priority' },
  { key: 'due', label: 'Due' },
]
const myOpenRows = computed<TaskTableRow[]>(() =>
  [...myOpen.value]
    .sort(
      (a, b) =>
        (a.due_date || '9999').localeCompare(b.due_date || '9999') ||
        (priorityRank.value.get(b.priority.toLowerCase()) ?? -1) -
          (priorityRank.value.get(a.priority.toLowerCase()) ?? -1),
    )
    .map((task) => ({
      id: task.id,
      title: task.title,
      cells: { status: task.status, priority: task.priority, due: formatDue(task.due_date) },
      alert: completion.isTaskOverdue(task) ? ['due'] : [],
    })),
)
const myCompletedColumns = [
  { key: 'done', label: 'Completed' },
  { key: 'cycle', label: 'Cycle time', numeric: true },
]
const myCompletedRows = computed<TaskTableRow[]>(() =>
  myCompleted.value.map((timeline) => ({
    id: timeline.task.id,
    title: timeline.task.title,
    cells: {
      done: formatDue(new Date(timeline.doneAt!).toISOString()),
      cycle: formatDays(cycleDays(timeline)),
    },
  })),
)

// -- Formatting and navigation ----------------------------------------------
const numberFormat = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 })
function fmt(value: number, digits = 0) {
  const factor = 10 ** digits
  return numberFormat.format(Math.round(value * factor) / factor)
}
function pct(ratio: number) {
  return `${Math.round(ratio * 100)}%`
}
const dayFormat = new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' })
function formatDue(value: string | null | undefined) {
  if (!value) return '—'
  const at = /^\d{4}-\d{2}-\d{2}$/.test(value) ? new Date(`${value}T00:00:00`) : new Date(value)
  return Number.isNaN(at.getTime()) ? value : dayFormat.format(at)
}

/** Change vs the previous period; `goodWhen` says which direction is good news. */
function delta(current: number, previous: number, goodWhen: 'up' | 'down' | null): StatDelta | null {
  if (!comparable.value) return null
  const ratio = changeRatio(current, previous)
  const direction = current > previous ? 'up' : current < previous ? 'down' : 'flat'
  const tone = direction === 'flat' || !goodWhen ? 'neutral' : direction === goodWhen ? 'good' : 'bad'
  const vs = `vs previous ${periodLabel.value.replace(/^Last /, '').toLowerCase()}`
  // Percentages of tiny baselines mislead ("1300%"); show the difference instead.
  if (ratio === null || previous < 10) {
    const diff = current - previous
    return { text: `${diff === 0 ? 'No change' : `${diff > 0 ? '+' : '−'}${Math.abs(diff)}`} ${vs}`, direction, tone }
  }
  return { text: `${direction === 'flat' ? 'No change' : pct(Math.abs(ratio))} ${vs}`, direction, tone }
}
function durationDelta(current: number | null, previous: number | null): StatDelta | null {
  if (!comparable.value || current === null || previous === null) return null
  const direction = current > previous ? 'up' : current < previous ? 'down' : 'flat'
  return {
    text: direction === 'flat' ? 'No change' : `was ${formatDays(previous)}`,
    direction,
    tone: direction === 'flat' ? 'neutral' : direction === 'down' ? 'good' : 'bad',
  }
}

function openTasks(extra: Record<string, string>) {
  const target: Record<string, string> = { ...filter.value }
  if (project.value) target.project = project.value
  for (const [key, value] of Object.entries(extra)) if (value) target[key] = value
  void router.push({ path: '/', query: target })
}
function openTask(id: string) {
  openTaskPanel({ taskId: id })
}

// Plain table view for column charts (every value readable without hover).
const SeriesTable = defineComponent({
  props: {
    labels: { type: Array as () => string[], required: true },
    series: { type: Array as () => Array<{ label: string; values: number[] }>, required: true },
  },
  setup(props) {
    return () =>
      h('table', { class: 'series-table' }, [
        h(
          'thead',
          h('tr', [
            h('th', { scope: 'col' }, ''),
            ...props.series.map((entry) => h('th', { scope: 'col', class: 'numeric' }, entry.label)),
          ]),
        ),
        h(
          'tbody',
          props.labels.map((label, index) =>
            h('tr', [
              h('th', { scope: 'row' }, label),
              ...props.series.map((entry) => h('td', { class: 'numeric' }, String(entry.values[index] ?? 0))),
            ]),
          ),
        ),
      ])
  },
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

.stat-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: var(--space-3, 0.75rem);
}

.chart-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-3, 0.75rem);
}

.chart-grid > .span-2 {
  grid-column: span 2;
}

@media (max-width: 900px) {
  .chart-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .chart-grid > .span-2 {
    grid-column: auto;
  }
}

.insights-callout {
  margin: 0;
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border-left: 3px solid var(--color-warning);
  background: color-mix(in oklab, var(--color-warning) 10%, transparent);
  font-size: var(--text-sm, 0.875rem);
}

.insights-footnote {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
}

.people-table {
  overflow-x: auto;
}

.people-table table,
:deep(.series-table) {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm, 0.875rem);
}

.people-table th,
.people-table td,
:deep(.series-table) th,
:deep(.series-table) td {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-bottom: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
  text-align: left;
  white-space: nowrap;
}

.people-table thead th,
:deep(.series-table) thead th {
  font-weight: 500;
  color: var(--color-muted);
}

.people-table .numeric,
:deep(.series-table) .numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.people-table td.alert {
  color: var(--color-danger-strong);
  font-weight: 600;
}

.link-button {
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.link-button:hover {
  text-decoration: underline;
}

.link-button:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.refresh-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border-left: 3px solid var(--color-danger);
}
</style>
