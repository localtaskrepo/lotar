<template>
  <div class="stat-row">
    <StatTile
      :label="`Throughput per ${bucketUnit}`"
      :value="fmt(throughputPerUnit, 1)"
      :delta="delta(throughputPerUnit, throughputPerUnitPrev, 'up', 1)"
    />
    <StatTile label="In progress" :value="fmt(wip.length)" caption="Started and not done" />
    <StatTile
      label="Cycle time (median)"
      :value="formatDays(cycleP50)"
      hint="From the first move into an active status to done"
      :delta="durationDelta(cycleP50, cycleP50Prev)"
    />
    <StatTile
      label="Cycle time (85th pct.)"
      :value="formatDays(cycleP85)"
      :caption="cycleP85 === null ? 'Needs 5 completed tasks' : ''"
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
      <template #actions>
        <label class="toolbar-field">
          <span class="toolbar-field__label">Split by</span>
          <select v-model="throughputSplit" class="input toolbar-select" data-testid="insights-throughput-split">
            <option v-for="option in splitOptions" :key="option.value" :value="option.value">
              {{ option.label }}
            </option>
          </select>
        </label>
      </template>
      <ColumnChart
        :labels="bucketLabels"
        :series="throughputSeries"
        :partial="bucketPartial"
        :stacked="throughputSplit !== 'none'"
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
      subtitle="Average days per task completed in the period"
      :empty="!statusTimes.length"
    >
      <BarList :rows="statusTimes" label="Average time in status" />
    </ChartCard>
    <ChartCard
      class="span-2"
      title="Aging work in progress"
      subtitle="Longest since work started; flagged when older than the 85th percentile cycle time"
      :empty="!wip.length"
      empty-text="Nothing is in progress."
    >
      <TaskTable :rows="agingRows" :columns="AGING_COLUMNS" @open="openTask" />
    </ChartCard>
  </div>
  <p class="insights-footnote muted">
    Sprint burndown, velocity and health live on the
    <RouterLink :to="{ path: '/sprints', query: project ? { project } : {} }">Sprints</RouterLink> page.
  </p>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { TaskDTO } from '../../../api/types'
import {
  countBy,
  countPerBucket,
  cycleDays,
  durationHistogram,
  formatDays,
  percentile,
  projectOf,
  timeInStatus,
  unitsIn,
} from '../../../utils/insightsMetrics'
import BarList, { type BarRow } from '../BarList.vue'
import ChartCard from '../ChartCard.vue'
import ColumnChart, { type ColumnSeries } from '../ColumnChart.vue'
import { fmt, pct, useInsights, valuesOf } from '../insightsContext'
import SeriesTable from '../SeriesTable.vue'
import StatTile from '../StatTile.vue'
import TaskTable, { type TaskTableRow } from '../TaskTable.vue'

const {
  project,
  range,
  prevRange,
  buckets,
  bucketLabels,
  bucketPartial,
  bucketUnit,
  lookup,
  timelines,
  completed,
  completedPrev,
  wip,
  completedSeries,
  leadP50,
  cycleP85,
  startedDays,
  delta,
  durationDelta,
  openTask,
} = useInsights()

// -- Speed -----------------------------------------------------------------------
const cycleValues = computed(() => valuesOf(completed.value, cycleDays))
const cycleP50 = computed(() => percentile(cycleValues.value, 50))
const cycleP50Prev = computed(() => percentile(valuesOf(completedPrev.value, cycleDays), 50))
const cycleHistogram = computed(() => durationHistogram(cycleValues.value))
const cycleBinLabels = computed(() => cycleHistogram.value.map((bin) => bin.label))
const cycleBinSeries = computed(() => [{ label: 'Tasks', values: cycleHistogram.value.map((bin) => bin.count) }])
/** Per whole day/week/month in the range (partial buckets don't dilute it). */
const throughputPerUnit = computed(() => completed.value.length / unitsIn(range.value, bucketUnit.value))
const throughputPerUnitPrev = computed(() => completedPrev.value.length / unitsIn(prevRange.value, bucketUnit.value))

// -- Throughput split ("what did we spend capacity on?") ----------------------------
type SplitKey = 'none' | 'type' | 'priority' | 'assignee' | 'project'
const SPLIT_OPTIONS: Array<{ value: SplitKey; label: string }> = [
  { value: 'none', label: 'Nothing' },
  { value: 'type', label: 'Type' },
  { value: 'priority', label: 'Priority' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'project', label: 'Project' },
]
const throughputSplit = ref<SplitKey>('none')
const splitOptions = computed(() => SPLIT_OPTIONS.filter((option) => option.value !== 'project' || !project.value))
function splitValue(task: TaskDTO, key: SplitKey): string {
  if (key === 'type') return task.task_type || '(none)'
  if (key === 'priority') return task.priority || '(none)'
  if (key === 'assignee') return task.assignee || 'Unassigned'
  return projectOf(task.id)
}
const throughputSeries = computed<ColumnSeries[]>(() => {
  const key = throughputSplit.value
  if (key === 'none') return [{ label: 'Completed', values: completedSeries.value }]
  // The four largest groups keep their own colour; the rest fold into Other.
  const totals = countBy(
    completed.value.map((timeline) => timeline.task),
    (task) => splitValue(task, key),
  )
  const top = totals.slice(0, totals.length > 4 ? 3 : 4).map((entry) => entry.key)
  const doneAtFor = (keep: (group: string) => boolean) =>
    countPerBucket(
      completed.value.filter((timeline) => keep(splitValue(timeline.task, key))).map((timeline) => timeline.doneAt),
      buckets.value,
    )
  const series: ColumnSeries[] = top.map((group) => ({ label: group, values: doneAtFor((value) => value === group) }))
  if (totals.length > top.length) {
    series.push({ label: 'Other', muted: true, values: doneAtFor((value) => !top.includes(value)) })
  }
  return series
})

// -- Where time goes -----------------------------------------------------------------
const statusTimes = computed<BarRow[]>(() =>
  timeInStatus(timelines.value, range.value, lookup).map((entry) => ({
    key: entry.status,
    label: entry.status,
    value: entry.days,
    display: formatDays(entry.days),
    hint: `${pct(entry.share)} of completed tasks passed through ${entry.status}`,
    clickable: false,
  })),
)

const AGING_COLUMNS = [
  { key: 'status', label: 'Status' },
  { key: 'assignee', label: 'Assignee' },
  { key: 'age', label: 'In progress for', numeric: true },
]
const agingRows = computed<TaskTableRow[]>(() =>
  wip.value
    .map((timeline) => ({ task: timeline.task, age: startedDays(timeline) }))
    .sort((a, b) => b.age - a.age)
    .map(({ task, age }) => ({
      id: task.id,
      title: task.title,
      cells: { status: task.status, assignee: task.assignee || '—', age: formatDays(age) },
      alert: cycleP85.value !== null && age > cycleP85.value ? ['age'] : [],
    })),
)
</script>
