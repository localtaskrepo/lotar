<template>
  <header class="topbar">
    <div class="topbar-inner container">
      <div class="brand-group">
        <a class="brand" href="/" aria-label="LoTaR home" @click.prevent="go('/')">
          <img class="brand__logo brand__logo--light" :src="logoLight" alt="" width="574" height="152" />
          <img class="brand__logo brand__logo--dark" :src="logoDark" alt="" width="574" height="152" />
          <span class="brand__name">LoTaR</span>
        </a>
        <UiButton
          variant="ghost"
          type="button"
          class="global-new-task"
          data-testid="global-new-task"
          aria-label="New task"
          title="New task"
          :disabled="taskPanelState.open"
          @click="openGlobalNewTask"
        >
          <IconGlyph name="plus" />
          <span class="global-new-task__label">Task</span>
        </UiButton>
      </div>
      <nav class="nav">
        <a
          v-for="item in visibleNavItems"
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
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import ActivityDrawer from './components/ActivityDrawer.vue'
import IconGlyph from './components/IconGlyph.vue'
import logoLight from './assets/branding/lotar-logo.svg'
import logoDark from './assets/branding/lotar-logo-dark.svg'
import TaskPanelHost from './components/TaskPanelHost.vue'
import ToastHost from './components/ToastHost.vue'
import UiButton from './components/UiButton.vue'
import { showToast } from './components/toast'
import { useTaskPanelController } from './composables/useTaskPanelController'
import { useNavTabs } from './composables/useNavTabs'
import { useTaskStore } from './composables/useTaskStore'
import { projectPrefixOfTaskId } from './utils/text'

const store = useTaskStore()
let unsubError: (() => void) | null = null
onMounted(() => {
  store.connectSse()
  unsubError = store.onTaskError(({ id, message }) => {
    showToast(`⚠ Task ${id}: ${message}`, 'File Error', 8000)
  })
})
onUnmounted(() => {
  store.disconnectSse()
  unsubError?.()
})
const version = (import.meta as any).env?.VITE_CARGO_VERSION || ''
const router = useRouter()
const route = useRoute()
const activityOpen = ref(false)
const { state: taskPanelState, openTaskPanel, closeTaskPanel } = useTaskPanelController()

// DEV98: known project context for global task creation. Only explicit signals
// count: a non-blank `?project=` query value (first entry when repeated) or, on
// a task detail route, the prefix of the route's task id. Never defaults to the
// first project.
const knownProject = computed<string | null>(() => {
  const rawProject = route.query.project
  const projectValue = Array.isArray(rawProject) ? rawProject[0] : rawProject
  if (typeof projectValue === 'string' && projectValue.trim() !== '') {
    return projectValue
  }
  const rawId = route.params.id
  const detailId = Array.isArray(rawId) ? rawId[0] : rawId
  if (typeof detailId === 'string' && detailId.trim() !== '') {
    return projectPrefixOfTaskId(detailId)
  }
  return null
})

function openGlobalNewTask() {
  if (taskPanelState.open) {
    return
  }
  openTaskPanel({
    taskId: 'new',
    initialProject: knownProject.value,
  })
}

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
  { label: 'Agents', path: '/agents' },
  { label: 'Automations', path: '/automations' },
  { label: 'Sync', path: '/sync' },
  { label: 'Scan', path: '/scan' },
  { label: 'Config', path: '/config' },
  { label: 'Preferences', path: '/preferences' },
]

const { isTabVisible } = useNavTabs()
const visibleNavItems = computed(() => navItems.filter((item) => isTabVisible(item.path)))

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

<style scoped>
/* Global new-task button: pinned next to the brand so it never scrolls away
   with the horizontally overflowing nav and stays reachable when nav tabs are
   hidden by preference. Mobile collapses it to an icon-only button. */
.brand-group {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
}

.brand {
  display: inline-flex;
  flex-shrink: 0;
  text-decoration: none;
}

.brand:focus-visible {
  border-radius: var(--radius-sm);
  outline: none;
  box-shadow: var(--focus-ring);
}

.brand__logo {
  display: block;
  width: auto;
  height: 28px;
}

.brand__logo--dark {
  display: none;
}

@media (prefers-color-scheme: dark) {
  .brand__logo--light { display: none; }
  .brand__logo--dark { display: block; }
}

:global(html[data-theme="light"] .brand__logo--light),
:global(html[data-theme="dark"] .brand__logo--dark) {
  display: block;
}

:global(html[data-theme="light"] .brand__logo--dark),
:global(html[data-theme="dark"] .brand__logo--light) {
  display: none;
}

.brand__name {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  clip-path: inset(50%);
  white-space: nowrap;
  border: 0;
}

.global-new-task {
  white-space: nowrap;
}

.global-new-task__label {
  white-space: nowrap;
}

@media (max-width: 900px) {
  .global-new-task {
    width: 2.25rem;
    height: 2.25rem;
    min-width: auto;
    padding: 0;
    gap: 0;
  }

  .global-new-task__label {
    display: none;
  }
}
</style>
