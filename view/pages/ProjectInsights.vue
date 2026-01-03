<template>
  <section class="col" style="gap:16px;">
    <div class="header row insights-header">
      <div class="col" style="gap:4px;">
        <h1>Insights</h1>
        <p class="muted" v-if="selectedProject">
          Health overview for <strong>{{ projectDisplayName }}</strong>
        </p>
      </div>
      <div class="controls row insights-controls">
        <div
          v-if="hasSingleProject"
          class="input insights-project-static"
          aria-label="Project filter"
          :title="singleProjectLabel"
        >
          <span class="insights-project-static__label">{{ singleProjectLabel }}</span>
        </div>
        <UiSelect v-else v-model="selectedProject" style="min-width:220px;">
          <option value="">All projects</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
        </UiSelect>
        <div class="tag-filter">
          <UiInput
            v-model="tagFilterInput"
            placeholder="Filter tags (comma separated)"
            @focus="onTagFilterFocus"
            @blur="onTagFilterBlur"
            @keydown="onTagFilterKeydown"
            @input="onTagFilterInput"
          />
          <ul
            v-if="tagSuggestionsVisible"
            class="tag-suggestions"
            role="listbox"
            aria-label="Tag suggestions"
          >
            <li
              v-for="(entry, index) in tagSuggestionEntries"
              :key="entry.value"
              class="tag-suggestions__item"
            >
              <button
                type="button"
                class="tag-suggestion"
                :class="{ active: tagActiveIndex === index }"
                role="option"
                :aria-selected="tagActiveIndex === index"
                @mousedown.prevent
                @click.prevent="selectTagSuggestion(entry.value)"
                @mouseenter="tagActiveIndex = index"
              >
                <span class="tag-suggestion__label">
                  <span
                    v-for="(part, partIndex) in entry.parts"
                    :key="partIndex"
                    :class="['tag-suggestion__part', { match: part.match }]"
                  >
                    {{ part.text }}
                  </span>
                </span>
              </button>
            </li>
          </ul>
        </div>
        <UiButton
          v-if="tagFilters.length"
          variant="ghost"
          type="button"
          @click="clearTagFilter"
        >
          Clear tags
        </UiButton>
        <ReloadButton
          :disabled="refreshing"
          :loading="refreshing"
          label="Refresh insights"
          title="Refresh insights"
          @click="refreshData"
        />
      </div>
    </div>

    <UiCard>
      <div class="summary-grid">
        <div
          v-for="tile in summaryTiles"
          :key="tile.label"
          :class="['summary-tile', { highlight: tile.highlight }]"
        >
          <span class="summary-label">{{ tile.label }}</span>
          <strong class="summary-value">{{ tile.display }}</strong>
        </div>
      </div>
    </UiCard>

    <div v-if="tasksLoading || refreshing" style="margin: 12px 0;">
      <UiLoader>Loading insights…</UiLoader>
    </div>

    <template v-else>
    <div v-if="!filteredTasks.length" class="muted" style="padding: 24px; text-align: center;">No tasks match the current filters.</div>
    <div v-else class="insights-grid">
        <UiCard>
          <div class="card-head">
            <h3>Status distribution</h3>
            <small class="muted">Click a segment to drill into Tasks</small>
          </div>
          <div class="row" style="gap:16px; flex-wrap: wrap; align-items: center;">
            <PieChart :data="statusPieData" :legend="false" :size="160" @select="onStatusSelect" />
            <table class="distribution">
              <tbody>
                <tr v-for="item in statusBreakdown" :key="item.label" @click="openTaskList({ status: item.raw })">
                  <th>{{ item.label }}</th>
                  <td>{{ item.count }}</td>
                  <td class="muted">{{ item.percent }}%</td>
                </tr>
              </tbody>
            </table>
          </div>
        </UiCard>

        <UiCard>
          <div class="card-head">
            <h3>Priority mix</h3>
            <small class="muted">Breakdown across all tasks in scope</small>
          </div>
          <table class="distribution">
            <tbody>
              <tr v-for="item in priorityBreakdown" :key="item.label" @click="openTaskList({ priority: item.raw })">
                <th>{{ item.label }}</th>
                <td>{{ item.count }}</td>
                <td class="muted">{{ item.percent }}%</td>
              </tr>
              <tr v-if="!priorityBreakdown.length"><td colspan="3" class="muted">No priority data</td></tr>
            </tbody>
          </table>
        </UiCard>

        <UiCard>
          <div class="card-head">
            <h3>Work types</h3>
            <small class="muted">Categorised by task type</small>
          </div>
          <table class="distribution">
            <tbody>
              <tr v-for="item in typeBreakdown" :key="item.label" @click="openTaskList({ type: item.raw })">
                <th>{{ item.label }}</th>
                <td>{{ item.count }}</td>
                <td class="muted">{{ item.percent }}%</td>
              </tr>
              <tr v-if="!typeBreakdown.length"><td colspan="3" class="muted">No type data</td></tr>
            </tbody>
          </table>
        </UiCard>

        <UiCard>
          <div class="card-head">
            <h3>Assignee load</h3>
            <small class="muted">Who owns the current work</small>
          </div>
          <table class="distribution">
            <tbody>
              <tr v-for="item in assigneeBreakdown" :key="item.label" @click="openTaskList({ assignee: item.raw })">
                <th>{{ item.label }}</th>
                <td>{{ item.count }}</td>
                <td class="muted">{{ item.percent }}%</td>
              </tr>
              <tr v-if="!assigneeBreakdown.length"><td colspan="3" class="muted">No assignee data</td></tr>
            </tbody>
          </table>
        </UiCard>

        <UiCard class="activity-card">
          <div class="card-head" style="align-items: flex-start;">
            <div class="col" style="gap:4px;">
              <h3>Activity trend</h3>
              <small class="muted">Tasks updated per day (last {{ windowDays }} days)</small>
            </div>
            <div class="chart-controls" role="group" aria-label="Select activity window">
              <UiButton
                v-for="days in timeWindows"
                :key="days"
                variant="ghost"
                :class="{ active: windowDays === days }"
                type="button"
                @click="windowDays = days"
              >
                {{ days }}d
              </UiButton>
            </div>
          </div>
          <div class="chart-scroll" ref="activityChartHost">
            <UiLoader v-if="activityFeedLoading" style="margin: 12px 0;">Loading activity…</UiLoader>
            <div v-else-if="activityFeedError" class="muted" style="padding: 12px 0;">{{ activityFeedError }}</div>
            <BarChart
              v-else
              :series="activitySeries"
              :width="activityChartRenderWidth"
              :height="180"
              color="#0ea5e9"
              :key="activityChartKey"
            />
          </div>
          <div class="muted" style="margin-top:8px;">Average {{ formatDecimal(averageUpdatesPerDay) }} updates / day</div>
        </UiCard>

        <UiCard>
          <div class="card-head">
            <h3>Due date outlook</h3>
            <small class="muted">Stay ahead of upcoming deadlines</small>
          </div>
          <table class="distribution">
            <tbody>
              <tr
                v-for="item in dueDateBreakdown"
                :key="item.label"
                :class="{ clickable: item.raw }"
                @click="item.raw ? openDueFilter(item.raw) : null"
              >
                <th>{{ item.label }}</th>
                <td>{{ item.count }}</td>
                <td class="muted">{{ item.percent }}%</td>
              </tr>
              <tr v-if="!dueDateBreakdown.length"><td colspan="3" class="muted">No due date information</td></tr>
            </tbody>
          </table>
        </UiCard>

        <UiCard v-if="!selectedProject">
          <div class="card-head">
            <h3>Projects overview</h3>
            <small class="muted">Quick snapshot across teams</small>
          </div>
          <table class="distribution">
            <thead>
              <tr>
                <th>Project</th>
                <th>Active</th>
                <th>Last update</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="summary in projectSummaries" :key="summary.prefix">
                <th style="font-weight:600;">
                  <button class="link-btn" type="button" @click="selectProject(summary.prefix)">
                    {{ summary.name }}
                  </button>
                  <div class="muted">{{ summary.prefix }}</div>
                </th>
                <td>{{ summary.open_count }}</td>
                <td>{{ summary.last_update }}</td>
              </tr>
              <tr v-if="!projectSummaries.length"><td colspan="3" class="muted">No projects available</td></tr>
            </tbody>
          </table>
        </UiCard>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch, watchEffect } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import type { ProjectDTO, ProjectStatsDTO, TaskDTO } from '../api/types'
import BarChart from '../components/BarChart.vue'
import PieChart from '../components/PieChart.vue'
import ReloadButton from '../components/ReloadButton.vue'
import { showToast } from '../components/toast'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import { useActivity } from '../composables/useActivity'
import { useProjects } from '../composables/useProjects'
import { useTasks } from '../composables/useTasks'
import { parseTaskDate, parseTaskDateToMillis, startOfLocalDay } from '../utils/date'
import { formatProjectLabel } from '../utils/projectLabels'

defineOptions({ name: 'ProjectInsights' })

const router = useRouter()
const route = useRoute()

const { projects, refresh: refreshProjects } = useProjects()
const { items: tasks, refresh: refreshTasks, loading: tasksLoading } = useTasks()

const singleProject = computed(() => (projects.value.length === 1 ? projects.value[0] : null))
const hasSingleProject = computed(() => !!singleProject.value)
const singleProjectLabel = computed(() => {
  const p = singleProject.value
  return p ? formatProjectLabel(p) : ''
})

const selectedProject = ref<string>('')
const tagFilterInput = ref<string>('')
const windowDays = ref<number>(30)
const timeWindows = [14, 30, 60, 90]

const projectStats = ref<ProjectStatsDTO | null>(null)
const projectStatsLoading = ref(false)
const projectStatsMap = reactive<Record<string, ProjectStatsDTO>>({})

const refreshing = ref(false)
const lastUpdated = ref<Date | null>(null)
const mounted = ref(false)

const tasksBase = computed<TaskDTO[]>(() => tasks.value ?? [])
const tagFilters = computed<string[]>(() => parseTagInput(tagFilterInput.value))
const tagFiltersNormalized = computed<string[]>(() => tagFilters.value.map(tag => normaliseTag(tag)))

const tagInputFocused = ref(false)
const tagActiveIndex = ref(-1)
let tagBlurTimer: ReturnType<typeof setTimeout> | null = null
const TAG_SUGGESTION_LIMIT = 8

const tagSuggestionCandidates = computed(() => {
  const map = new Map<string, string>()
  tasksBase.value.forEach(task => {
    (task.tags || []).forEach(tag => {
      const trimmed = (tag || '').trim()
      if (!trimmed) return
      const key = trimmed.toLowerCase()
      if (!map.has(key)) map.set(key, trimmed)
    })
  })
  if (tasksBase.value.some(task => !task.tags || !(task.tags.length))) {
    map.set('untagged', 'Untagged')
  }
  ;(projectStats.value?.tags_top || []).forEach(tag => {
    const trimmed = (tag || '').trim()
    if (!trimmed) return
    const key = trimmed.toLowerCase()
    if (!map.has(key)) map.set(key, trimmed)
  })
  return Array.from(map.values()).sort((a, b) => a.localeCompare(b))
})

const availableTagSuggestions = computed(() => {
  if (!tagSuggestionCandidates.value.length) return [] as string[]
  const selected = new Set(tagFiltersNormalized.value)
  return tagSuggestionCandidates.value.filter(tag => !selected.has(normaliseTag(tag)))
})

const activeTagQuery = computed(() => {
  const raw = tagFilterInput.value
  const parts = raw.split(',')
  if (!parts.length) return ''
  const trailingEmpty = raw.trim().endsWith(',')
  const current = trailingEmpty ? '' : parts[parts.length - 1]
  return (current ?? '').trim()
})

const tagSuggestionList = computed(() => {
  const base = availableTagSuggestions.value
  if (!tagInputFocused.value || !base.length) return [] as string[]
  const query = activeTagQuery.value.toLowerCase()
  if (!query) return base.slice(0, TAG_SUGGESTION_LIMIT)
  return base.filter(tag => tag.toLowerCase().includes(query)).slice(0, TAG_SUGGESTION_LIMIT)
})

const tagSuggestionsVisible = computed(() => tagInputFocused.value && tagSuggestionList.value.length > 0)

const tagSuggestionEntries = computed(() => tagSuggestionList.value.map(tag => ({
  value: tag,
  parts: highlightTagSuggestion(tag, activeTagQuery.value),
})))

const activityChartHost = ref<HTMLElement | null>(null)
const activityChartWidth = ref(640)
let activityResizeObserver: ResizeObserver | null = null
const {
  feed: sharedActivityFeed,
  feedLoading: sharedActivityFeedLoading,
  feedError: sharedActivityFeedError,
  refreshFeed: refreshSharedActivityFeed,
} = useActivity()

const activityFeed = sharedActivityFeed
const activityFeedLoading = sharedActivityFeedLoading
const activityFeedError = sharedActivityFeedError

function updateActivityChartWidth(widthOverride?: number) {
  const host = activityChartHost.value
  if (!host) return
  const measured = typeof widthOverride === 'number' ? widthOverride : host.getBoundingClientRect().width
  if (!measured || Number.isNaN(measured)) return
  activityChartWidth.value = Math.max(1, Math.floor(measured))
}

watch(activityChartHost, el => {
  if (el) {
    updateActivityChartWidth()
  }
})
watch(tagSuggestionList, list => {
  tagActiveIndex.value = list.length ? 0 : -1
})

const projectDisplayName = computed(() => {
  if (!selectedProject.value) return 'All projects'
  const project = projects.value.find(p => p.prefix === selectedProject.value)
  return project ? project.name : selectedProject.value
})

watchEffect(() => {
  const p = singleProject.value
  if (!p) return
  const prefix = (p.prefix ?? '').trim()
  if (!prefix) return
  if (selectedProject.value !== prefix) {
    selectedProject.value = prefix
  }
})

const filteredTasks = computed(() => {
  if (!tagFiltersNormalized.value.length) return tasksBase.value
  const includeUntagged = tagFiltersNormalized.value.includes('untagged')
  const requiredTags = tagFiltersNormalized.value.filter(tag => tag !== 'untagged')
  return tasksBase.value.filter(task => {
    const tags = (task.tags || []).map(tag => normaliseTag(tag))
    if (!tags.length) {
      return includeUntagged && requiredTags.length === 0
    }
    if (!requiredTags.length) return true
    return requiredTags.every((filterTag: string) => tags.includes(filterTag))
  })
})

const totalTasks = computed(() => filteredTasks.value.length)
const uniqueStatusCount = computed(() => {
  const set = new Set<string>()
  filteredTasks.value.forEach(task => set.add((task.status || 'Unspecified').trim().toLowerCase()))
  return set.size
})
const assignedTasks = computed(() => filteredTasks.value.filter(t => !!t.assignee).length)
const unassignedTasks = computed(() => filteredTasks.value.filter(t => !t.assignee).length)
const taggedTasks = computed(() => filteredTasks.value.filter(t => (t.tags || []).length > 0).length)
const overdueTasks = computed(() => {
  const nowMs = Date.now()
  return filteredTasks.value.filter((t) => {
    const dueMs = parseTaskDateToMillis(t.due_date)
    return dueMs !== null && dueMs < nowMs
  }).length
})

const noDueDateTasks = computed(() => filteredTasks.value.filter(t => !t.due_date).length)

const statusBreakdown = computed(() => buildBreakdown(filteredTasks.value, t => normaliseLabel(t.status), t => t.status || ''))
const priorityBreakdown = computed(() => buildBreakdown(filteredTasks.value, t => normaliseLabel(t.priority), t => t.priority || ''))
const typeBreakdown = computed(() => buildBreakdown(filteredTasks.value, t => normaliseLabel(t.task_type), t => t.task_type || ''))
const assigneeBreakdown = computed(() => buildBreakdown(filteredTasks.value, t => (t.assignee ? formatAssignee(t.assignee) : 'Unassigned'), t => t.assignee || ''))

const statusPieData = computed(() => statusBreakdown.value.map(item => ({ label: item.label, value: item.count, raw: item.raw })))

const activitySeries = computed(() => {
  const now = new Date()
  const start = new Date(now)
  start.setDate(now.getDate() - (windowDays.value - 1))
  const startIso = isoDay(start)
  const endIso = isoDay(now)
  const visibleTasks = new Set(filteredTasks.value.map(task => task.id))
  const dayBuckets = new Map<string, Record<string, number>>()

  activityFeed.value.forEach(item => {
    if (visibleTasks.size && !visibleTasks.has(item.task_id)) return
    item.history.forEach(entry => {
      const entryDate = new Date(entry.at)
      if (Number.isNaN(entryDate.getTime())) return
      const iso = isoDay(entryDate)
      if (iso < startIso || iso > endIso) return
      const bucket = dayBuckets.get(iso) ?? {}
      entry.changes.forEach(change => {
        const kind = (change.kind || 'other').toLowerCase()
        bucket[kind] = (bucket[kind] ?? 0) + 1
      })
      dayBuckets.set(iso, bucket)
    })
  })

  const series: Array<{ key: string; total: number; count: number; breakdown: Record<string, number> }> = []
  for (let i = 0; i < windowDays.value; i += 1) {
    const date = new Date(start)
    date.setDate(start.getDate() + i)
    const iso = isoDay(date)
    const label = date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
    const breakdown = dayBuckets.get(iso) ?? {}
    const total = Object.values(breakdown).reduce((sum, value) => sum + value, 0)
  series.push({ key: label, total, count: total, breakdown: { ...breakdown } })
  }
  return series
})

const totalActivityInWindow = computed(() => activitySeries.value.reduce((acc, item) => acc + item.total, 0))
const averageUpdatesPerDay = computed(() => (windowDays.value ? totalActivityInWindow.value / windowDays.value : 0))
const activityChartKey = computed(() => [
  selectedProject.value || 'all',
  windowDays.value,
  tagFiltersNormalized.value.join('|') || 'none',
  filteredTasks.value.length,
  activityFeed.value.length,
  totalActivityInWindow.value,
].join(':'))
const activityChartRenderWidth = computed(() => {
  const width = Math.round(activityChartWidth.value)
  return width > 0 ? width : 320
})

const projectSummaries = computed(() => projects.value.map((project: ProjectDTO) => {
  const stats = projectStatsMap[project.prefix]
  return {
    prefix: project.prefix,
    name: project.name,
    open_count: stats?.open_count ?? 0,
    last_update: formatRelativeTimestamp(stats?.recent_modified ?? null),
  }
}))

const lastUpdatedText = computed(() => {
  if (!lastUpdated.value) return '—'
  return lastUpdated.value.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
})

const summaryTiles = computed(() => [
  { label: 'Total tasks', display: formatNumber(totalTasks.value) },
  { label: 'Statuses', display: formatNumber(uniqueStatusCount.value) },
  { label: 'Tagged', display: formatNumber(taggedTasks.value) },
  { label: 'Assigned', display: formatNumber(assignedTasks.value) },
  { label: 'Unassigned', display: formatNumber(unassignedTasks.value) },
  { label: 'No due date', display: formatNumber(noDueDateTasks.value) },
  { label: 'Overdue', display: formatNumber(overdueTasks.value), highlight: overdueTasks.value > 0 },
  { label: 'Activity (avg/day)', display: formatDecimal(averageUpdatesPerDay.value) },
  { label: 'Updated', display: lastUpdatedText.value },
])

const dueDateBreakdown = computed(() => {
  if (!filteredTasks.value.length) return [] as Array<{ label: string; count: number; percent: number; raw: string }>
  const todayStart = startOfLocalDay(new Date())
  const tomorrowStart = addDays(todayStart, 1)
  const soonCutoff = addDays(todayStart, 7)
  let overdue = 0
  let dueToday = 0
  let dueSoon = 0
  let dueLater = 0
  let missing = 0
  filteredTasks.value.forEach(task => {
    if (!task.due_date) {
      missing += 1
      return
    }
    const due = parseTaskDate(task.due_date)
    if (!due) {
      missing += 1
      return
    }
    const dueStart = startOfLocalDay(due)
    if (dueStart < todayStart) {
      overdue += 1
    } else if (dueStart < tomorrowStart) {
      dueToday += 1
    } else if (dueStart <= soonCutoff) {
      dueSoon += 1
    } else {
      dueLater += 1
    }
  })
  const total = filteredTasks.value.length
  const rows = [
    { label: 'Overdue', count: overdue, raw: 'overdue' },
    { label: 'Due today', count: dueToday, raw: 'today' },
    { label: 'Due in next 7 days', count: dueSoon, raw: 'soon' },
    { label: 'Due later', count: dueLater, raw: 'later' },
    { label: 'No due date', count: missing, raw: 'missing' },
  ]
  return rows
    .filter(row => row.count > 0)
    .map(row => ({ ...row, percent: total ? Math.round((row.count / total) * 100) : 0 }))
})

function onTagFilterFocus() {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
  tagInputFocused.value = true
  tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
}

function onTagFilterBlur() {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
  tagBlurTimer = setTimeout(() => {
    tagInputFocused.value = false
    tagActiveIndex.value = -1
    tagBlurTimer = null
  }, 120)
}

function onTagFilterInput() {
  tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
}

function onTagFilterKeydown(event: KeyboardEvent) {
  const suggestions = tagSuggestionList.value
  if (event.key === 'ArrowDown') {
    if (!suggestions.length) return
    event.preventDefault()
    tagActiveIndex.value = (tagActiveIndex.value + 1 + suggestions.length) % suggestions.length
  } else if (event.key === 'ArrowUp') {
    if (!suggestions.length) return
    event.preventDefault()
    tagActiveIndex.value = (tagActiveIndex.value - 1 + suggestions.length) % suggestions.length
  } else if (event.key === 'Enter') {
    if (tagActiveIndex.value >= 0 && suggestions[tagActiveIndex.value]) {
      event.preventDefault()
      selectTagSuggestion(suggestions[tagActiveIndex.value])
      return
    }
    event.preventDefault()
    applyTagFilters(parseTagInput(tagFilterInput.value))
  } else if (event.key === 'Tab') {
    if (tagActiveIndex.value >= 0 && suggestions[tagActiveIndex.value]) {
      selectTagSuggestion(suggestions[tagActiveIndex.value])
      event.preventDefault()
    }
  } else if (event.key === 'Escape') {
    tagActiveIndex.value = -1
    tagInputFocused.value = false
  }
}

function currentTagTokens() {
  const raw = tagFilterInput.value
  if (!raw) return [] as string[]
  const parts = raw.split(',')
  if (!parts.length) return [] as string[]
  const trailingEmpty = raw.trim().endsWith(',')
  const tokens = trailingEmpty ? parts : parts.slice(0, -1)
  return tokens.map(part => part.trim()).filter(part => part.length > 0)
}

function selectTagSuggestion(tag: string) {
  const base = currentTagTokens()
  applyTagFilters([...base, tag])
  tagActiveIndex.value = tagSuggestionList.value.length ? 0 : -1
}

function highlightTagSuggestion(tag: string, query: string) {
  if (!query) {
    return [{ text: tag, match: false }] as Array<{ text: string; match: boolean }>
  }
  const lowerTag = tag.toLowerCase()
  const lowerQuery = query.toLowerCase()
  const segments: Array<{ text: string; match: boolean }> = []
  let searchStart = 0
  let matchIndex = lowerTag.indexOf(lowerQuery)
  if (matchIndex === -1) {
    return [{ text: tag, match: false }]
  }
  while (matchIndex !== -1) {
    if (matchIndex > searchStart) {
      segments.push({ text: tag.slice(searchStart, matchIndex), match: false })
    }
    const matchEnd = matchIndex + lowerQuery.length
    segments.push({ text: tag.slice(matchIndex, matchEnd), match: true })
    searchStart = matchEnd
    matchIndex = lowerTag.indexOf(lowerQuery, searchStart)
  }
  if (searchStart < tag.length) {
    segments.push({ text: tag.slice(searchStart), match: false })
  }
  return segments
}

function openDueFilter(key: string) {
  if (key === 'missing') {
    openTaskList({ needs: 'due' })
    return
  }
  if (key === 'overdue' || key === 'today' || key === 'soon' || key === 'later') {
    openTaskList({ due: key })
  }
}

function parseTagInput(input: string): string[] {
  return input
    .split(/[\s,]+/)
    .map(segment => segment.trim())
    .filter(segment => segment.length > 0)
}

function normaliseTag(tag: string): string {
  return tag.trim().toLowerCase()
}

function normaliseLabel(value?: string | null) {
  if (!value) return 'Unspecified'
  return value.replace(/[_-]/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}

function formatAssignee(value: string) {
  const trimmed = value.startsWith('@') ? value : `@${value}`
  return trimmed
}

function isoDay(date: Date) {
  return date.toISOString().slice(0, 10)
}

function startOfDay(date: Date) {
  return startOfLocalDay(date)
}

function addDays(date: Date, days: number) {
  const next = new Date(date)
  next.setDate(next.getDate() + days)
  return next
}

function toBreakdownArray(map: Record<string, number>, denominator: number) {
  return Object.entries(map)
    .map(([label, count]) => ({ label, count, percent: denominator ? Math.round((count / denominator) * 100) : 0 }))
    .sort((a, b) => b.count - a.count)
}

function buildBreakdown(tasksArray: TaskDTO[], labelFor: (task: TaskDTO) => string, rawFor: (task: TaskDTO) => string, limit = 12) {
  const counts: Record<string, { count: number; raw: string }> = {}
  tasksArray.forEach(task => {
    const label = labelFor(task)
    const raw = rawFor(task)
    if (!counts[label]) counts[label] = { count: 0, raw }
    counts[label].count += 1
  })
  const denominator = tasksArray.length || 0
  return Object.entries(counts)
    .map(([label, entry]) => ({
      label,
      count: entry.count,
      raw: entry.raw,
      percent: denominator ? Math.round((entry.count / denominator) * 100) : 0,
    }))
    .sort((a, b) => b.count - a.count)
    .slice(0, limit)
}

function formatDecimal(value: number) {
  return value ? value.toFixed(1) : '0.0'
}

function formatNumber(value: number) {
  return value.toLocaleString()
}

function formatRelativeTimestamp(value: string | null | undefined) {
  if (!value) return '—'
  const parsed = new Date(value)
  if (Number.isNaN(parsed.getTime())) return '—'
  const now = new Date()
  const diffMs = now.getTime() - parsed.getTime()
  const diffMinutes = Math.round(diffMs / (60 * 1000))
  if (diffMinutes <= 0) return 'just now'
  if (diffMinutes < 60) return `${diffMinutes} min ago`
  const diffHours = Math.round(diffMinutes / 60)
  if (diffHours < 24) return `${diffHours} hr${diffHours === 1 ? '' : 's'} ago`
  const diffDays = Math.round(diffHours / 24)
  if (diffDays < 30) return `${diffDays} day${diffDays === 1 ? '' : 's'} ago`
  const diffMonths = Math.round(diffDays / 30)
  if (diffMonths < 12) return `${diffMonths} mo${diffMonths === 1 ? '' : 's'} ago`
  const diffYears = Math.round(diffMonths / 12)
  return `${diffYears} yr${diffYears === 1 ? '' : 's'} ago`
}

async function refreshData() {
  if (refreshing.value) return
  refreshing.value = true
  const filter: Record<string, string> = {}
  if (selectedProject.value) filter.project = selectedProject.value
  try {
    await refreshTasks(filter)
    await loadSelectedProjectStats()
    if (!selectedProject.value) await ensureProjectStatsMap()
    await refreshActivityFeedWindow()
    lastUpdated.value = new Date()
  } catch (error: any) {
    showToast(error?.message || 'Failed to refresh insights')
  } finally {
    refreshing.value = false
  }
}

async function refreshActivityFeedWindow() {
  const now = new Date()
  const sinceDate = startOfLocalDay(addDays(now, -(windowDays.value - 1)))
  const params: Record<string, any> = {
    since: sinceDate.toISOString(),
    until: now.toISOString(),
    limit: 400,
  }
  if (selectedProject.value) {
    params.project = selectedProject.value
  }
  await refreshSharedActivityFeed(params)
}

function applyTagFilters(next: string[]) {
  const deduped: string[] = []
  const seen = new Set<string>()
  next.forEach(tag => {
    const trimmed = tag.trim()
    if (!trimmed) return
    const normalised = normaliseTag(trimmed)
    if (seen.has(normalised)) return
    seen.add(normalised)
    deduped.push(trimmed)
  })
  tagFilterInput.value = deduped.join(', ')
}

function clearTagFilter() {
  applyTagFilters([])
}

async function loadSelectedProjectStats() {
  if (!selectedProject.value) {
    projectStats.value = null
    return
  }
  projectStatsLoading.value = true
  try {
    const stats = await api.projectStats(selectedProject.value)
    projectStats.value = stats
    projectStatsMap[selectedProject.value] = stats
  } catch (error: any) {
    showToast(error?.message || 'Failed to load project stats')
  } finally {
    projectStatsLoading.value = false
  }
}

async function ensureProjectStatsMap() {
  const pending = projects.value.filter(project => !projectStatsMap[project.prefix])
  if (!pending.length) return
  await Promise.all(pending.map(async project => {
    try {
      const stats = await api.projectStats(project.prefix)
      projectStatsMap[project.prefix] = stats
    } catch (error: any) {
      showToast(error?.message || `Failed to load stats for ${project.prefix}`)
    }
  }))
}

function openTaskList(filters: Record<string, string | string[]>) {
  const query: Record<string, string> = {}
  if (selectedProject.value) query.project = selectedProject.value
  if (tagFilters.value.length) query.tags = tagFilters.value.join(',')
  Object.entries(filters).forEach(([key, value]) => {
    if (Array.isArray(value)) {
      if (value.length) query[key] = value.join(',')
    } else if (value) {
      query[key] = value
    }
  })
  router.push({ path: '/', query })
}

function onStatusSelect(payload: unknown) {
  if (!payload || typeof payload !== 'object') return
  const candidate = payload as { raw?: unknown }
  if (typeof candidate.raw !== 'string' || !candidate.raw.trim()) return
  openTaskList({ status: candidate.raw })
}

function selectProject(prefix: string) {
  selectedProject.value = prefix
}

const routeProject = computed(() => {
  const value = route.query.project
  return typeof value === 'string' ? value : ''
})

const routeTags = computed(() => {
  const value = route.query.tags
  const values = Array.isArray(value) ? value : (typeof value === 'string' ? value.split(',') : [])
  return values
    .map(entry => decodeURIComponent(String(entry)))
    .map(tag => tag.trim())
    .filter(tag => tag.length > 0)
})

function haveSameTags(a: string[], b: string[]) {
  if (a.length !== b.length) return false
  const left = [...a].map(tag => normaliseTag(tag)).sort()
  const right = [...b].map(tag => normaliseTag(tag)).sort()
  return left.every((value, index) => value === right[index])
}

function buildRouteQuery() {
  const query: Record<string, string> = {}
  Object.entries(route.query).forEach(([key, value]) => {
    if (key === 'project' || key === 'tags') return
    if (Array.isArray(value)) return
    if (value != null) query[key] = String(value)
  })
  if (selectedProject.value) query.project = selectedProject.value
  if (tagFilters.value.length) query.tags = tagFilters.value.join(',')
  return query
}

async function syncRoute() {
  const query = buildRouteQuery()
  await router.replace({ path: '/insights', query })
}

watch(routeProject, value => {
  if (!mounted.value) return
  if (value !== selectedProject.value) selectedProject.value = value
})

watch(routeTags, value => {
  if (!mounted.value) return
  if (haveSameTags(value, tagFilters.value)) return
  applyTagFilters(value)
})

watch(selectedProject, async value => {
  if (!mounted.value) return
  await syncRoute()
  await refreshData()
})

watch(tagFilters, async value => {
  if (!mounted.value) return
  if (haveSameTags(value, routeTags.value)) return
  await syncRoute()
})

watch(projects, () => {
  if (!mounted.value) return
  ensureProjectStatsMap()
})

watch(windowDays, async () => {
  if (!mounted.value) return
  await refreshActivityFeedWindow()
})

onBeforeUnmount(() => {
  if (tagBlurTimer) {
    clearTimeout(tagBlurTimer)
    tagBlurTimer = null
  }
  if (activityResizeObserver) {
    activityResizeObserver.disconnect()
    activityResizeObserver = null
  }
})

onMounted(async () => {
  await refreshProjects()
  if (routeProject.value) {
    selectedProject.value = routeProject.value
  }
  if (routeTags.value.length) {
    applyTagFilters(routeTags.value)
  }
  await ensureProjectStatsMap()
  await refreshData()
  mounted.value = true
  if (route.path !== '/insights') {
    await syncRoute()
  }
  await nextTick()
  updateActivityChartWidth()
  if (typeof window !== 'undefined' && typeof ResizeObserver !== 'undefined') {
    activityResizeObserver = new ResizeObserver(entries => {
      entries.forEach(entry => {
        if (entry.target === activityChartHost.value) {
          updateActivityChartWidth(entry.contentRect?.width)
        }
      })
    })
    if (activityChartHost.value) {
      activityResizeObserver.observe(activityChartHost.value)
    }
  }
})
</script>
<style scoped>
.header h1 { margin: 0; }
.controls button { white-space: nowrap; }
.summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 12px;
}
.summary-tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px;
  border-radius: 8px;
  background: color-mix(in oklab, var(--bg, #f8fafc) 92%, transparent);
}
.summary-tile.highlight {
  border: 1px solid var(--color-danger, #ef4444);
}
.summary-label {
  font-size: 0.8rem;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.summary-value {
  font-size: 1.4rem;
  font-weight: 700;
}
.insights-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
}
.card-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 12px;
}
.card-head h3 {
  margin: 0;
}
.insights-header {
  flex-direction: row;
  align-items: flex-start;
  flex-wrap: wrap;
  gap: 12px;
}
.insights-header > .col {
  flex: 1 1 280px;
  min-width: 220px;
}
.insights-controls {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  margin-left: auto;
  justify-content: flex-end;
  min-width: 320px;
}
.insights-controls > .ui-select {
  flex: 0 0 220px;
  min-width: 220px;
}
.insights-controls > .insights-project-static {
  flex: 0 0 220px;
  min-width: 220px;
  max-width: 220px;
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  border-color: transparent;
  background: transparent;
  color: var(--color-muted);
  box-shadow: none;
  cursor: default;
}

.insights-project-static__label {
  display: block;
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
}
.insights-controls :is(.ui-select, .input, .btn) {
  min-height: 2.25rem;
  height: 2.25rem;
  padding-top: 0;
  padding-bottom: 0;
}
.insights-controls .ui-select {
  padding-right: calc(var(--space-3, 0.75rem) + 16px);
}
.tag-filter {
  position: relative;
  flex: 0 0 220px;
  width: 220px;
  min-width: 220px;
  max-width: 220px;
  display: flex;
}
.tag-filter .input {
  flex: 1 1 auto;
  width: 100%;
  min-height: 2.25rem;
  height: 2.25rem;
  padding-top: 0;
  padding-bottom: 0;
}
.activity-card {
  grid-column: span 2;
}
.activity-card .chart-scroll {
  width: 100%;
}
@media (max-width: 960px) {
  .activity-card {
    grid-column: span 1;
  }
}
.chart-scroll {
  width: 100%;
  overflow-x: hidden;
  padding: 8px 0 12px;
}
.chart-scroll canvas {
  display: block;
  width: 100% !important;
}
.chart-controls {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.chart-controls .ghost {
  border: 1px solid color-mix(in oklab, var(--border, #e2e8f0) 80%, transparent);
}
.chart-controls .ghost.active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 20%, transparent);
  border-color: color-mix(in oklab, var(--color-accent, #0ea5e9) 60%, transparent);
}
.distribution {
  width: 100%;
  border-collapse: collapse;
}
.distribution th,
.distribution td {
  padding: 6px 0;
  text-align: left;
  border-bottom: 1px solid color-mix(in oklab, var(--border, #e2e8f0) 80%, transparent);
}
.distribution tr { cursor: pointer; }
.distribution tr:hover { background: color-mix(in oklab, var(--bg, #f8fafc) 75%, transparent); }
.tag-cloud {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.tag-chip {
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 6px 10px;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--border, #e2e8f0) 80%, transparent);
  background: transparent;
  cursor: pointer;
}
.tag-chip:hover {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 12%, transparent);
}
.tag-chip.active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  border-color: color-mix(in oklab, var(--color-accent, #0ea5e9) 40%, transparent);
}
@media (max-width: 960px) {
  .insights-header {
    flex-direction: column;
  }
  .insights-controls {
    width: 100%;
    margin-left: 0;
    justify-content: flex-start;
  }
}
@media (max-width: 960px) {
  .insights-controls {
    justify-content: flex-start;
  }
}
.tag-suggestions {
  position: absolute;
  z-index: 5;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  background: var(--surface, #ffffff);
  border-radius: 8px;
  border: 1px solid color-mix(in oklab, var(--border, #e2e8f0) 80%, transparent);
  box-shadow: 0 10px 30px rgba(15, 23, 42, 0.12);
  margin: 0;
  padding: 4px;
  list-style: none;
  max-height: 220px;
  overflow-y: auto;
}
.tag-suggestions__item + .tag-suggestions__item { margin-top: 2px; }
.tag-suggestion {
  width: 100%;
  text-align: left;
  padding: 6px 10px;
  border-radius: 6px;
  border: none;
  background: transparent;
  cursor: pointer;
  font: inherit;
  display: flex;
  gap: 4px;
}
.tag-suggestion:hover,
.tag-suggestion.active {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 16%, transparent);
  color: var(--color-strong, #0f172a);
}
.tag-suggestion__part.match {
  font-weight: 600;
  color: var(--color-accent-strong, #0284c7);
}
.tag-suggestion__label {
  display: flex;
  gap: 2px;
  flex-wrap: wrap;
}
.link-btn {
  background: none;
  border: none;
  padding: 0;
  font: inherit;
  color: var(--color-accent, #0ea5e9);
  cursor: pointer;
}
.link-btn:hover { text-decoration: underline; }
</style>
