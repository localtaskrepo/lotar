<template>
  <div class="stat-row">
    <StatTile
      label="Overdue"
      :value="fmt(overdue.length)"
      :tone="overdue.length ? 'alert' : 'neutral'"
      clickable
      @select="openTasks({ due: 'overdue' })"
    />
    <StatTile
      label="Due in the next 7 days"
      :value="fmt(dueSoon.length)"
      caption="From tomorrow, open work"
      clickable
      @select="openTasks({ due: 'soon' }, true)"
    />
    <StatTile
      label="No due date"
      :value="fmt(noDueDate.length)"
      caption="Open tasks"
      clickable
      @select="openTasks({ needs: 'due' }, true)"
    />
    <StatTile label="Open effort" :value="effortText(open)" :caption="openEffortCaption" />
    <StatTile
      label="Unestimated"
      :value="fmt(unestimated.length)"
      caption="Open tasks without effort"
      clickable
      @select="openTasks({ needs: 'effort' }, true)"
    />
    <StatTile
      label="Time to clear open work"
      :value="forecastText"
      :caption="forecastCaption"
      hint="Open tasks divided by the completion rate in the period; ignores new work coming in"
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
        @select="(key) => key !== 'No tag' && openTasks({ tags: key }, true)"
      />
    </ChartCard>
    <ChartCard
      class="span-2"
      title="Upcoming deadlines"
      subtitle="Open work due in the next 14 days, and anything overdue"
      :empty="!upcoming.length"
      empty-text="No deadlines in the next 14 days."
    >
      <TaskTable :rows="upcomingRows" :columns="UPCOMING_COLUMNS" :limit="12" @open="openTask" />
    </ChartCard>
    <ChartCard
      class="span-2"
      title="Investment mix"
      :subtitle="`Completed work by type, ${periodLabel.toLowerCase()}`"
      :empty="!completed.length"
    >
      <BarList :rows="completedByType" label="Completed tasks by type" />
    </ChartCard>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { countBy, DAY_MS, effortTotals, parseDue, unitsIn } from '../../../utils/insightsMetrics'
import BarList, { type BarRow } from '../BarList.vue'
import ChartCard from '../ChartCard.vue'
import { effortText, fmt, formatDue, rows, useInsights } from '../insightsContext'
import StatTile from '../StatTile.vue'
import TaskTable, { type TaskTableRow } from '../TaskTable.vue'

const {
  now,
  range,
  periodLabel,
  completion,
  open,
  completed,
  overdue,
  dueSoon,
  noDueDate,
  openByTag,
  openTasks,
  openTask,
} = useInsights()

const openEffortCaption = computed(
  () => `${effortTotals(open.value).estimated} of ${open.value.length} open tasks estimated`,
)
/** Matches the server's `needs=effort`: no effort value at all. */
const unestimated = computed(() => open.value.filter((task) => !(task.effort ?? '').trim()))

const weeklyThroughput = computed(() => completed.value.length / unitsIn(range.value, 'week'))
const weeksToClear = computed(() => (weeklyThroughput.value > 0 ? open.value.length / weeklyThroughput.value : null))
const forecastText = computed(() => {
  const weeks = weeksToClear.value
  if (weeks === null) return '—'
  if (weeks < 1) return '< 1 week'
  return weeks < 20 ? `~${Math.round(weeks)} weeks` : `~${Math.round(weeks / 4.35)} months`
})
const forecastCaption = computed(() =>
  weeksToClear.value === null
    ? 'Nothing completed in the period'
    : `At ${fmt(weeklyThroughput.value, 1)} completed per week, with no new work`,
)

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
  if (key === 'none') openTasks({ needs: 'due' }, true)
  else openTasks({ due: key }, key !== 'overdue')
}

const upcoming = computed(() =>
  open.value
    .filter((task) => {
      const bucket = completion.dueBucketFor(task)
      if (bucket === 'overdue' || bucket === 'today' || bucket === 'soon') return true
      const due = parseDue(task.due_date)
      return due !== null && due <= now.value + 14 * DAY_MS
    })
    .sort((a, b) => (parseDue(a.due_date) ?? 0) - (parseDue(b.due_date) ?? 0)),
)
const UPCOMING_COLUMNS = [
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

const completedByType = computed(() =>
  rows(
    countBy(
      completed.value.map((timeline) => timeline.task),
      (task) => task.task_type,
    ),
    '(none)',
    false,
  ),
)
</script>
