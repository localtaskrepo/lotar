<template>
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
        @select="openTasks({ assignee: '@me' }, true)"
      />
      <StatTile label="In progress" :value="fmt(myWip.length)" />
      <StatTile
        label="Overdue"
        :value="fmt(myOverdue.length)"
        :tone="myOverdue.length ? 'alert' : 'neutral'"
        clickable
        @select="openTasks({ assignee: '@me', due: 'overdue' })"
      />
      <StatTile label="Due in the next 7 days" :value="fmt(myDueSoon.length)" caption="From tomorrow" />
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
        <TaskTable :rows="myOpenRows" :columns="MY_OPEN_COLUMNS" :limit="15" @open="openTask" />
      </ChartCard>
      <ChartCard
        class="span-2"
        title="Recently completed"
        :subtitle="periodLabel"
        :empty="!myCompleted.length"
        empty-text="Nothing completed in this period."
      >
        <TaskTable :rows="myCompletedRows" :columns="MY_COMPLETED_COLUMNS" @open="openTask" />
      </ChartCard>
    </div>
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { TaskDTO } from '../../../api/types'
import { cycleDays, formatDays, parseDue } from '../../../utils/insightsMetrics'
import ChartCard from '../ChartCard.vue'
import { dayFormat, fmt, formatDue, useInsights } from '../insightsContext'
import StatTile from '../StatTile.vue'
import TaskTable, { type TaskTableRow } from '../TaskTable.vue'

const { me, priorities, periodLabel, completion, open, completed, overdue, dueSoon, wip, openTasks, openTask } =
  useInsights()

const isMine = (task: TaskDTO) => !!me.value && task.assignee === me.value
const myOpen = computed(() => open.value.filter(isMine))
const myWip = computed(() => wip.value.filter((timeline) => isMine(timeline.task)))
const myOverdue = computed(() => overdue.value.filter(isMine))
const myDueSoon = computed(() => dueSoon.value.filter(isMine))
const myCompleted = computed(() =>
  completed.value.filter((timeline) => isMine(timeline.task)).sort((a, b) => (b.doneAt ?? 0) - (a.doneAt ?? 0)),
)

const priorityRank = computed(() => new Map(priorities.value.map((label, index) => [label.toLowerCase(), index])))
const MY_OPEN_COLUMNS = [
  { key: 'status', label: 'Status' },
  { key: 'priority', label: 'Priority' },
  { key: 'due', label: 'Due' },
]
const myOpenRows = computed<TaskTableRow[]>(() =>
  [...myOpen.value]
    .sort(
      (a, b) =>
        (parseDue(a.due_date) ?? Infinity) - (parseDue(b.due_date) ?? Infinity) ||
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
const MY_COMPLETED_COLUMNS = [
  { key: 'done', label: 'Completed' },
  { key: 'cycle', label: 'Cycle time', numeric: true },
]
const myCompletedRows = computed<TaskTableRow[]>(() =>
  myCompleted.value.map((timeline) => ({
    id: timeline.task.id,
    title: timeline.task.title,
    cells: {
      done: dayFormat.format(timeline.doneAt!),
      cycle: formatDays(cycleDays(timeline)),
    },
  })),
)
</script>
