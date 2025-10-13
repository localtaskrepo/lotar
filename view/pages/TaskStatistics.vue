<template>
  <section class="col" style="gap: 16px;">
    <div class="row" style="justify-content: space-between; align-items: center;">
      <h1>Statistics</h1>
      <div class="row" style="gap:8px;">
        <select class="input" v-model="selected">
          <option disabled value="">Select project</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ p.name }}</option>
        </select>
        <button class="btn" :disabled="!selected" @click="load">Load</button>
      </div>
    </div>

  <div class="grid" v-if="stats">
      <UiCard>
        <h3>Status distribution</h3>
        <PieChart :data="statusData" @select="onStatusSelect" />
      </UiCard>
      <UiCard>
        <h3>Activity (last changes per task)</h3>
        <BarChart :series="activitySeries" @select="onActivitySelect" />
        <div class="muted" style="margin-top:6px;">Click bars to view recent tasks.</div>
      </UiCard>
      <UiCard>
        <h3>Top tags</h3>
        <div class="row" style="gap:8px; flex-wrap: wrap;">
          <span v-for="t in stats.tags_top" :key="t" class="chip" @click="goToTag(t)">{{ t }}</span>
        </div>
      </UiCard>
      <UiCard>
        <h3>Top contributors (30d)</h3>
        <ul class="list">
          <li v-for="a in topAuthors" :key="a.email" class="row" style="justify-content: space-between;">
            <span>{{ a.author }}</span>
            <span class="muted">{{ a.commits }}</span>
          </li>
          <li v-if="!topAuthors.length" class="muted">No recent activity</li>
        </ul>
      </UiCard>
      <UiCard>
        <div class="row" style="justify-content: space-between; align-items: center;">
          <h3>Recently changed tasks</h3>
          <div class="row" style="gap:8px; align-items:center;">
            <select class="input" v-model="windowDays">
              <option v-for="d in [7,14,30,90]" :key="d" :value="d">Last {{ d }}d</option>
            </select>
            <select class="input" v-model="authorFilter">
              <option value="">All authors</option>
              <option v-for="a in topAuthors" :key="a.email" :value="a.author">{{ a.author }}</option>
            </select>
            <button class="btn" @click="loadChanged">Apply</button>
          </div>
        </div>
        <ul class="list">
          <li v-for="t in changedTasks" :key="t.id" class="row" style="justify-content: space-between;">
            <a @click.prevent="goToTask(t.id)" href="#">{{ t.id }}</a>
            <span class="muted">{{ new Date(t.last_date).toLocaleString() }}</span>
          </li>
          <li v-if="!changedTasks.length" class="muted">No changes in window</li>
        </ul>
      </UiCard>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { api } from '../api/client'
import BarChart from '../components/BarChart.vue'
import PieChart from '../components/PieChart.vue'
import UiCard from '../components/UiCard.vue'
import { useProjects } from '../composables/useProjects'
import { useTaskPanelController } from '../composables/useTaskPanelController'

const router = useRouter()
const { projects, stats, refresh, loadStats } = useProjects()
const { openTaskPanel } = useTaskPanelController()
const selected = ref('')
const route = useRoute()
onMounted(async () => {
  await refresh()
  const proj = String(route.query.project || '')
  if (proj) { selected.value = proj }
  else if (projects.value.length) { selected.value = projects.value[0].prefix }
  if (selected.value) await load()
})
async function load(){ if (selected.value) await loadStats(selected.value) }

// Build status donut from stats + a fast list query
const statusData = computed(() => {
  if (!stats.value) return []
  return [
    { label: 'Open', value: stats.value.open_count },
    { label: 'Done', value: stats.value.done_count },
  ]
})

// Use backend activity endpoints
const activitySeries = ref<Array<{ key: string; count: number }>>([])
const topAuthors = ref<Array<{ author: string; email: string; commits: number }>>([])
const changedTasks = ref<Array<{ id: string; project: string; last_date: string }>>([])
const windowDays = ref(30)
const authorFilter = ref('')
onMounted(async () => {
  if (!selected.value) return
  const series = await api.activitySeries('day', { project: selected.value })
  activitySeries.value = series.map(s => ({ key: s.key, count: s.count }))
  const authors = await api.activityAuthors({ project: selected.value })
  topAuthors.value = authors.slice(0, 8)
  await loadChanged()
})

function onStatusSelect(payload: unknown){
  const item = (payload as { label?: string }) || {}
  const q: Record<string,string> = { project: selected.value }
  if (item.label === 'Open') q.status = 'open,in-progress,blocked,backlog'
  if (item.label === 'Done') q.status = 'done'
  router.push({ path: '/', query: q })
}

function onActivitySelect(payload: { index: number }){
  const key = activitySeries.value[payload.index]?.key
  if (!key) return
  // Navigate to tasks filtered by date via search query (assuming backend ignores unknown keys and frontend sorts)
  router.push({ path: '/', query: { project: selected.value, q: key } })
}

function goToTag(tag: string){ router.push({ path: '/', query: { project: selected.value, tags: tag } }) }

async function loadChanged(){
  if (!selected.value) return
  const since = new Date(Date.now() - windowDays.value*24*60*60*1000).toISOString()
  const params: any = { project: selected.value, since }
  if (authorFilter.value) params.author = authorFilter.value
  const items = await api.activityChangedTasks(params)
  changedTasks.value = items.slice(0, 20)
}

function goToTask(id: string){
  openTaskPanel({ taskId: id })
}
</script>

<style scoped>
</style>
