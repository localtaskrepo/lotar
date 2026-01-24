<template>
  <header class="topbar">
    <div class="topbar-inner container">
      <div class="brand">LoTaR</div>
      <nav class="nav">
        <a
          v-for="item in navItems"
          :key="item.path"
          class="nav__link"
          :class="{ active: isActive(item) }"
          :href="item.path"
          @click.prevent="go(item.path)"
        >
          {{ item.label }}
        </a>
        <UiButton variant="ghost" type="button" @click="activityOpen = true">Activity</UiButton>
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
import { ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import ActivityDrawer from './components/ActivityDrawer.vue'
import TaskPanelHost from './components/TaskPanelHost.vue'
import ToastHost from './components/ToastHost.vue'
import UiButton from './components/UiButton.vue'
import { useTaskPanelController } from './composables/useTaskPanelController'
const version = (import.meta as any).env?.VITE_CARGO_VERSION || ''
const router = useRouter()
const route = useRoute()
const activityOpen = ref(false)
const { state: taskPanelState, closeTaskPanel } = useTaskPanelController()

type NavItem = {
  label: string
  path: string
  matches?: (currentPath: string) => boolean
}

const navItems: NavItem[] = [
  { label: 'Tasks', path: '/', matches: (current) => current === '/' || current.startsWith('/task/') },
  { label: 'Sprints', path: '/sprints' },
  { label: 'Boards', path: '/boards' },
  { label: 'Calendar', path: '/calendar' },
  { label: 'Insights', path: '/insights' },
  { label: 'Sync', path: '/sync' },
  { label: 'Scan', path: '/scan' },
  { label: 'Config', path: '/config' },
  { label: 'Preferences', path: '/preferences' },
]

function isActive(item: NavItem) {
  const currentPath = route.path
  return item.matches ? item.matches(currentPath) : currentPath === item.path
}

function go(path: string) {
  router.push(path)
}

watch(activityOpen, (open) => {
  if (open) {
    closeTaskPanel()
  }
})

watch(
  () => taskPanelState.open,
  (open) => {
    if (open) {
      activityOpen.value = false
    }
  },
)
</script>

<style>
/* No component-scoped styles; rely on global utilities in styles.css */
</style>
