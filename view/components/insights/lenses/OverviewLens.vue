<template>
  <div class="stat-row">
    <StatTile label="Open" :value="fmt(open.length)" caption="Right now" clickable @select="openTasks({}, true)" />
    <StatTile
      label="Completed"
      :value="fmt(completed.length)"
      :delta="delta(completed.length, completedPrev.length, 'up')"
    />
    <StatTile label="Created" :value="fmt(created.length)" :delta="delta(created.length, createdPrev.length, null)" />
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
        :partial="bucketPartial"
        :summary="`Created and completed tasks per ${bucketUnit}`"
      />
      <template #table>
        <SeriesTable :labels="bucketLabels" :series="flowSeries" />
      </template>
    </ChartCard>
    <ChartCard
      title="Open work over time"
      subtitle="Open tasks at the end of each period: is the backlog growing?"
      :empty="!tasks.length"
    >
      <TrendChart :labels="bucketLabels" :series="openTrendSeries" summary="Open tasks over time" />
      <template #table>
        <SeriesTable :labels="bucketLabels" :series="openTrendSeries" />
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
    <ChartCard class="span-2" title="Open work by" :subtitle="`${open.length} open tasks`" :empty="!open.length">
      <template #actions>
        <label class="toolbar-field">
          <span class="toolbar-field__label">Group</span>
          <select v-model="openDimension" class="input toolbar-select" data-testid="insights-open-dimension">
            <option v-for="option in OPEN_DIMENSIONS" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </label>
      </template>
      <BarList
        :rows="openByDimension"
        :label="`Open tasks by ${openDimension}`"
        :limit="10"
        clickable
        @select="openDimensionRow"
      />
    </ChartCard>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  countBy,
  countPerBucket,
  DAY_MS,
  formatDays,
  leadDays,
  metDueDate,
  openAtBucketEnds,
  orderByConfigured,
  percentile,
} from '../../../utils/insightsMetrics'
import BarList, { type BarRow } from '../BarList.vue'
import ChartCard from '../ChartCard.vue'
import ColumnChart from '../ColumnChart.vue'
import { fmt, pct, rows, useInsights, valuesOf } from '../insightsContext'
import SeriesTable from '../SeriesTable.vue'
import StatTile from '../StatTile.vue'
import TrendChart from '../TrendChart.vue'

const {
  tasks,
  types,
  priorities,
  now,
  periodLabel,
  buckets,
  bucketLabels,
  bucketPartial,
  bucketUnit,
  timelines,
  open,
  completed,
  completedPrev,
  created,
  createdPrev,
  overdue,
  completedSeries,
  leadP50,
  openByStatus,
  openByTag,
  priorityRows,
  delta,
  durationDelta,
  openTasks,
} = useInsights()

const leadP50Prev = computed(() => percentile(valuesOf(completedPrev.value, leadDays), 50))

const onTimeResults = computed(() =>
  completed.value.map(metDueDate).filter((value): value is boolean => value !== null),
)
const onTimeRate = computed(() =>
  onTimeResults.value.length ? onTimeResults.value.filter(Boolean).length / onTimeResults.value.length : null,
)
const onTimeCaption = computed(() =>
  onTimeResults.value.length ? `${onTimeResults.value.length} with a due date` : 'No completed work had a due date',
)

const flowSeries = computed(() => [
  {
    label: 'Created',
    values: countPerBucket(
      created.value.map((timeline) => timeline.created),
      buckets.value,
    ),
  },
  { label: 'Completed', values: completedSeries.value },
])
const openTrendSeries = computed(() => [
  { label: 'Open', values: openAtBucketEnds(timelines.value, buckets.value, now.value) },
])

// -- Needs attention -------------------------------------------------------------
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
      // The Tasks list has no "not updated since" filter to drill into.
      clickable: false,
    },
    { key: 'unassigned', label: 'Unassigned', value: unassignedOpen.value.length },
  ].filter((row) => row.value > 0),
)
function openAttention(key: string) {
  if (key === 'overdue') openTasks({ due: 'overdue' })
  else if (key === 'urgent-unassigned') openTasks({ assignee: '__none__', priority: topPriority.value }, true)
  else if (key === 'unassigned') openTasks({ assignee: '__none__' }, true)
}

// -- Open work by a chosen dimension -----------------------------------------------
type OpenDimension = 'status' | 'priority' | 'type' | 'assignee' | 'tag'
const OPEN_DIMENSIONS: Array<{ value: OpenDimension; label: string }> = [
  { value: 'status', label: 'Status' },
  { value: 'priority', label: 'Priority' },
  { value: 'type', label: 'Type' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'tag', label: 'Tag' },
]
const DIMENSION_FILTER: Record<OpenDimension, string> = {
  status: 'status',
  priority: 'priority',
  type: 'type',
  assignee: 'assignee',
  tag: 'tags',
}
const openDimension = ref<OpenDimension>('status')
const openByDimension = computed<BarRow[]>(() => {
  switch (openDimension.value) {
    case 'priority':
      return priorityRows(open.value)
    case 'type':
      return rows(
        orderByConfigured(
          countBy(open.value, (task) => task.task_type),
          types.value,
        ),
      )
    case 'assignee':
      return rows(
        countBy(open.value, (task) => task.assignee, 'Unassigned'),
        'Unassigned',
      )
    case 'tag':
      return openByTag.value
    default:
      return openByStatus.value
  }
})
function openDimensionRow(key: string) {
  // A status drill-down is already open-only; the others add the open statuses.
  openTasks({ [DIMENSION_FILTER[openDimension.value]]: key }, openDimension.value !== 'status')
}
</script>
