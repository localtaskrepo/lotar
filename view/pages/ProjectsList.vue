<template>
  <section class="col" style="gap:16px;">
    <div class="row" style="justify-content: space-between;">
      <h1>Projects</h1>
      <UiButton @click="refresh">Refresh</UiButton>
    </div>
    <UiCard>
      <ul class="list">
        <li v-for="p in projects" :key="p.prefix" class="row" style="justify-content: space-between; align-items: center;">
          <div class="col">
            <strong>{{ p.name }}</strong>
            <span class="muted">{{ p.prefix }}</span>
          </div>
          <div class="row" style="gap: 12px; align-items: center;">
            <div class="row" v-if="statsMap[p.prefix]" style="gap:8px;">
              <span class="muted">Open: <strong>{{ statsMap[p.prefix].open_count }}</strong></span>
              <span class="muted">Done: <strong>{{ statsMap[p.prefix].done_count }}</strong></span>
            </div>
            <UiButton @click="viewStats(p.prefix)">View Stats</UiButton>
            <UiButton @click="editConfig(p.prefix)">Edit Config</UiButton>
          </div>
        </li>
        <li v-if="!projects.length" class="muted">No projects</li>
      </ul>
    </UiCard>
  </section>
</template>
<script setup lang="ts">
import { onMounted, reactive } from 'vue'
import { useRouter } from 'vue-router'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import { useProjects } from '../composables/useProjects'

const router = useRouter()
const { projects, refresh, loadStats } = useProjects()
const statsMap = reactive<Record<string, { open_count: number; done_count: number }>>({})
onMounted(refresh)
function viewStats(prefix: string){ router.push('/insights?project=' + encodeURIComponent(prefix)) }
function editConfig(prefix: string){ router.push('/config?project=' + encodeURIComponent(prefix)) }

onMounted(async () => {
  await refresh()
  // Prefetch lightweight stats per project
  for (const p of projects.value) {
    try {
      const s = await loadStats(p.prefix)
      statsMap[p.prefix] = { open_count: s.open_count, done_count: s.done_count }
    } catch {}
  }
})
</script>
<style scoped></style>
