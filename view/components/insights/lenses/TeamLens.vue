<template>
  <div class="stat-row">
    <StatTile label="People with open work" :value="fmt(assignees.length)" />
    <StatTile
      label="Unassigned"
      :value="fmt(unassignedOpen.length)"
      :tone="unassignedOpen.length ? 'alert' : 'neutral'"
      caption="Open tasks without an assignee"
      clickable
      @select="openTasks({ assignee: '__none__' }, true)"
    />
    <StatTile
      label="Active contributors"
      :value="fmt(contributors)"
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
            <th
              v-for="column in PEOPLE_COLUMNS"
              :key="column.key"
              scope="col"
              :class="{ numeric: column.numeric }"
              :aria-sort="
                peopleSort.key === column.key ? (peopleSort.dir === 'asc' ? 'ascending' : 'descending') : 'none'
              "
            >
              <button type="button" class="sort-button" @click="sortPeople(column.key)">
                {{ column.label }}
                <span v-if="peopleSort.key === column.key" aria-hidden="true">{{
                  peopleSort.dir === 'asc' ? '▲' : '▼'
                }}</span>
              </button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="person in people" :key="person.key">
            <td>
              <button type="button" class="link-button" @click="openTasks({ assignee: person.filter }, true)">
                {{ person.label }}
              </button>
            </td>
            <td class="numeric">{{ person.open }}</td>
            <td class="numeric">{{ person.inProgress }}</td>
            <td class="numeric" :class="{ alert: person.overdue > 0 }">{{ person.overdue }}</td>
            <td class="numeric">{{ person.completed }}</td>
            <td class="numeric">{{ formatDays(person.cycle) }}</td>
            <td
              class="numeric"
              :class="{ alert: cycleP85 !== null && person.oldest !== null && person.oldest > cycleP85 }"
            >
              {{ formatDays(person.oldest) }}
            </td>
            <td class="numeric">{{ person.effort }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </ChartCard>
  <div class="chart-grid">
    <ChartCard title="Completed per person" :subtitle="periodLabel" :empty="!completedByPerson.length">
      <BarList :rows="completedByPerson" label="Completed tasks per person" />
    </ChartCard>
    <ChartCard title="Activity per person" subtitle="Status changes, edits and comments" :empty="!activity.length">
      <BarList :rows="activityRows" label="Changes per person" />
    </ChartCard>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { TaskDTO } from '../../../api/types'
import {
  activityByActor,
  countBy,
  cycleDays,
  effortTotals,
  formatDays,
  percentile,
  type TaskTimeline,
} from '../../../utils/insightsMetrics'
import BarList from '../BarList.vue'
import ChartCard from '../ChartCard.vue'
import { effortText, fmt, rows, useInsights, valuesOf } from '../insightsContext'
import StatTile from '../StatTile.vue'

const { range, periodLabel, timelines, openTimelines, open, completed, overdue, cycleP85, startedDays, openTasks } =
  useInsights()

const unassignedOpen = computed(() => open.value.filter((task) => !task.assignee))

interface PersonRow {
  key: string
  label: string
  filter: string
  open: number
  inProgress: number
  overdue: number
  completed: number
  cycle: number | null
  oldest: number | null
  effortHours: number
  effort: string
}
type PersonSort = 'label' | 'open' | 'inProgress' | 'overdue' | 'completed' | 'cycle' | 'oldest' | 'effortHours'
const PEOPLE_COLUMNS: Array<{ key: PersonSort; label: string; numeric: boolean }> = [
  { key: 'label', label: 'Person', numeric: false },
  { key: 'open', label: 'Open', numeric: true },
  { key: 'inProgress', label: 'In progress', numeric: true },
  { key: 'overdue', label: 'Overdue', numeric: true },
  { key: 'completed', label: 'Completed', numeric: true },
  { key: 'cycle', label: 'Cycle time', numeric: true },
  { key: 'oldest', label: 'Oldest in progress', numeric: true },
  { key: 'effortHours', label: 'Open effort', numeric: true },
]
const peopleSort = ref<{ key: PersonSort; dir: 'asc' | 'desc' }>({ key: 'open', dir: 'desc' })
function sortPeople(key: PersonSort) {
  const current = peopleSort.value
  peopleSort.value =
    current.key === key
      ? { key, dir: current.dir === 'asc' ? 'desc' : 'asc' }
      : { key, dir: key === 'label' ? 'asc' : 'desc' }
}

const people = computed<PersonRow[]>(() => {
  // One pass over open and completed work.
  const groups = new Map<string, { open: TaskDTO[]; done: TaskTimeline[]; wip: TaskTimeline[] }>()
  const group = (key: string) => {
    let entry = groups.get(key)
    if (!entry) groups.set(key, (entry = { open: [], done: [], wip: [] }))
    return entry
  }
  for (const timeline of openTimelines.value) {
    const entry = group(timeline.task.assignee || '')
    entry.open.push(timeline.task)
    if (timeline.inProgress) entry.wip.push(timeline)
  }
  for (const timeline of completed.value) group(timeline.task.assignee || '').done.push(timeline)
  const overdueIds = new Set(overdue.value.map((task) => task.id))
  const list = [...groups.entries()].map(([key, entry]) => ({
    key: key || '(unassigned)',
    label: key || 'Unassigned',
    filter: key || '__none__',
    open: entry.open.length,
    inProgress: entry.wip.length,
    overdue: entry.open.filter((task) => overdueIds.has(task.id)).length,
    completed: entry.done.length,
    cycle: percentile(valuesOf(entry.done, cycleDays), 50),
    oldest: entry.wip.length ? Math.max(...entry.wip.map(startedDays)) : null,
    effortHours: effortTotals(entry.open).hours,
    effort: effortText(entry.open),
  }))
  const { key, dir } = peopleSort.value
  const sign = dir === 'asc' ? 1 : -1
  return list.sort((a, b) => {
    // Unassigned always last; missing values sort after real ones.
    const unassigned = Number(a.label === 'Unassigned') - Number(b.label === 'Unassigned')
    if (unassigned) return unassigned
    if (key === 'label') return sign * a.label.localeCompare(b.label)
    const left = a[key] ?? -Infinity
    const right = b[key] ?? -Infinity
    return sign * ((left as number) - (right as number)) || a.label.localeCompare(b.label)
  })
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
    // The Tasks list cannot filter by completion date.
    false,
  ),
)
const activity = computed(() => activityByActor(timelines.value, range.value))
const activityRows = computed(() => rows(activity.value, '(unknown)', false))
const contributors = computed(() => activity.value.filter((entry) => entry.key !== '(unknown)').length)
</script>

<style scoped>
.people-table {
  overflow-x: auto;
}

.people-table table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--text-sm, 0.875rem);
}

.people-table th,
.people-table td {
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-bottom: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
  text-align: left;
  white-space: nowrap;
}

.people-table thead th {
  font-weight: 500;
  color: var(--color-muted);
}

.people-table .numeric {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.people-table td.alert {
  color: var(--color-danger-strong);
  font-weight: 600;
}

.sort-button,
.link-button {
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.sort-button {
  font-weight: 500;
  white-space: nowrap;
}

.link-button {
  text-align: left;
}

.link-button:hover {
  text-decoration: underline;
}

.sort-button:focus-visible,
.link-button:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}
</style>
