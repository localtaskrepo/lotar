<template>
  <header class="topbar">
    <div class="topbar-inner container">
      <div class="brand">LoTaR</div>
      <nav class="nav">
        <a href="/" @click.prevent="go('/')">Tasks</a>
        <a href="/sprints" @click.prevent="go('/sprints')">Sprints</a>
        <a href="/boards" @click.prevent="go('/boards')">Boards</a>
        <a href="/calendar" @click.prevent="go('/calendar')">Calendar</a>
        <a href="/insights" @click.prevent="go('/insights')">Insights</a>
        <a href="/config" @click.prevent="go('/config')">Config</a>
        <a href="/preferences" @click.prevent="go('/preferences')">Preferences</a>
        <button class="btn ghost" type="button" @click="activityOpen = true">Activity</button>
      </nav>
    </div>
  </header>
  <main class="container">
    <div class="surface">
      <router-view />
    </div>
  </main>
  <footer class="container muted" style="padding: 16px;">
    <small>Local Task Repo · v{{ version }}</small>
  </footer>
  <TaskPanelHost />
  <ToastHost />
  <ActivityDrawer :open="activityOpen" @close="activityOpen = false" />
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import ActivityDrawer from './components/ActivityDrawer.vue'
import TaskPanelHost from './components/TaskPanelHost.vue'
import ToastHost from './components/ToastHost.vue'
const version = (import.meta as any).env?.VITE_CARGO_VERSION || ''
const router = useRouter()
const activityOpen = ref(false)
function go(path: string) { router.push(path) }
</script>

<style>
/* No component-scoped styles; rely on global utilities in styles.css */
</style>
