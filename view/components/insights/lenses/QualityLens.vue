<template>
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
      @select="openTasks({ type: bugTypeFilter }, true)"
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
    <StatTile label="Bug share" :value="bugShare === null ? '—' : pct(bugShare)" caption="Of new work in the period" />
    <StatTile
      label="Reopened"
      :value="fmt(reopened.length)"
      hint="Tasks of any type moved from done back to an open status in the period"
      :delta="delta(reopened.length, reopenedPrev.length, 'down')"
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
        :partial="bucketPartial"
        :summary="`Bugs reported and fixed per ${bucketUnit}`"
      />
      <template #table>
        <SeriesTable :labels="bucketLabels" :series="bugSeries" />
      </template>
    </ChartCard>
    <ChartCard class="span-2" title="Open bugs over time" :empty="!bugTypesPresent">
      <TrendChart :labels="bucketLabels" :series="openBugTrend" summary="Open bugs over time" />
      <template #table>
        <SeriesTable :labels="bucketLabels" :series="openBugTrend" />
      </template>
    </ChartCard>
    <ChartCard title="Open bugs by priority" :empty="!openBugs.length">
      <BarList
        :rows="priorityRows(openBugs)"
        label="Open bugs by priority"
        clickable
        @select="(key) => openTasks({ type: bugTypeFilter, priority: key }, true)"
      />
    </ChartCard>
    <ChartCard title="Oldest open bugs" :empty="!openBugs.length" empty-text="No open bugs.">
      <TaskTable :rows="oldestBugRows" :columns="OLDEST_BUG_COLUMNS" @open="openTask" />
    </ChartCard>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TaskDTO } from '../../../api/types'
import {
  countPerBucket,
  DAY_MS,
  formatDays,
  inRange,
  isBugType,
  leadDays,
  openAtBucketEnds,
  percentile,
  type Range,
} from '../../../utils/insightsMetrics'
import BarList from '../BarList.vue'
import ChartCard from '../ChartCard.vue'
import ColumnChart from '../ColumnChart.vue'
import { fmt, pct, useInsights, valuesOf } from '../insightsContext'
import SeriesTable from '../SeriesTable.vue'
import StatTile from '../StatTile.vue'
import TaskTable, { type TaskTableRow } from '../TaskTable.vue'
import TrendChart from '../TrendChart.vue'

const {
  tasks,
  now,
  range,
  prevRange,
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
  priorityRows,
  delta,
  openTasks,
  openTask,
} = useInsights()

const isBug = (task: TaskDTO) => isBugType(task.task_type)
const bugTypes = computed(() => [...new Set(tasks.value.filter(isBug).map((task) => task.task_type))])
const bugTypesPresent = computed(() => bugTypes.value.length > 0)
const bugTypeFilter = computed(() => bugTypes.value.join(','))
const openBugs = computed(() => open.value.filter(isBug))
const bugsReported = computed(() => created.value.filter((timeline) => isBug(timeline.task)))
const bugsReportedPrev = computed(() => createdPrev.value.filter((timeline) => isBug(timeline.task)))
const bugsFixed = computed(() => completed.value.filter((timeline) => isBug(timeline.task)))
const bugsFixedPrev = computed(() => completedPrev.value.filter((timeline) => isBug(timeline.task)))
const bugFixP50 = computed(() => percentile(valuesOf(bugsFixed.value, leadDays), 50))
const bugShare = computed(() => (created.value.length ? bugsReported.value.length / created.value.length : null))
const reopenedIn = (target: Range) =>
  timelines.value.filter((timeline) => timeline.reopenedAt.some((at) => inRange(at, target)))
const reopened = computed(() => reopenedIn(range.value))
const reopenedPrev = computed(() => reopenedIn(prevRange.value))

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
const openBugTrend = computed(() => [
  {
    label: 'Open bugs',
    values: openAtBucketEnds(
      timelines.value.filter((timeline) => isBug(timeline.task)),
      buckets.value,
      now.value,
    ),
  },
])

const OLDEST_BUG_COLUMNS = [
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
</script>
