<template>
  <header class="topbar">
    <div class="topbar-inner container">
      <div class="brand-group">
        <a
          class="brand"
          :class="{ 'brand--custom': brandIsCustom }"
          href="/"
          :aria-label="`${appName} home`"
          @click.prevent="go('/')"
        >
          <template v-if="branding.logo">
            <img class="brand__logo brand__logo--custom brand__logo--light" :src="branding.logo" alt="" />
            <img
              class="brand__logo brand__logo--custom brand__logo--dark"
              :src="branding.logoDark ?? branding.logo"
              alt=""
            />
          </template>
          <!-- A custom name without a logo pairs it with the LoTaR app icon. -->
          <img v-else-if="branding.name" class="brand__logo brand__logo--icon" :src="appIcon" alt="" />
          <template v-else>
            <img class="brand__logo brand__logo--light" :src="logoLight" alt="" width="574" height="152" />
            <img class="brand__logo brand__logo--dark" :src="logoDark" alt="" width="574" height="152" />
          </template>
          <span class="brand__name" :class="{ 'brand__name--visible': branding.name }">{{ appName }}</span>
        </a>
        <div class="topbar-actions">
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
          <UiButton
            v-if="showPaletteButton"
            variant="ghost"
            type="button"
            class="palette-trigger"
            data-testid="command-palette-trigger"
            :aria-label="`Command palette (${paletteLabel})`"
            :title="`Jump to a page, run a command or find a task (${paletteLabel})`"
            @click="paletteOpen = true"
          >
            <IconGlyph name="search" />
            <kbd class="palette-trigger__keys">{{ paletteLabel }}</kbd>
          </UiButton>
        </div>
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
  <footer class="container muted" style="padding: 16px">
    <small>Local Task Repo · v{{ version }}</small>
  </footer>
  <TaskPanelHost />
  <ToastHost />
  <ActivityDrawer :open="activityOpen" @close="activityOpen = false" />
  <CommandPalette
    :open="paletteOpen"
    :commands="paletteCommands"
    @close="paletteOpen = false"
    @open-task="(id) => openTaskPanel({ taskId: id })"
  />
  <KeyboardShortcutsDialog
    :open="shortcutsOpen"
    :single-key-enabled="singleKeyShortcuts"
    @close="shortcutsOpen = false"
    @navigate="go"
  />
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import ActivityDrawer from './components/ActivityDrawer.vue'
import CommandPalette, { type PaletteCommand } from './components/CommandPalette.vue'
import KeyboardShortcutsDialog from './components/KeyboardShortcutsDialog.vue'
import IconGlyph from './components/IconGlyph.vue'
import logoLight from './assets/branding/lotar-logo.svg'
import logoDark from './assets/branding/lotar-logo-dark.svg'
import appIcon from './assets/branding/lotar-app-icon.svg'
import { DEFAULT_APP_NAME, useBranding } from './composables/useBranding'
import TaskPanelHost from './components/TaskPanelHost.vue'
import ToastHost from './components/ToastHost.vue'
import UiButton from './components/UiButton.vue'
import { showToast } from './components/toast'
import { useActiveFilterBar } from './composables/useFilterCommands'
import { useGlobalShortcuts } from './composables/useGlobalShortcuts'
import { useTaskPanelController } from './composables/useTaskPanelController'
import { NAV_TABS, useNavTabs } from './composables/useNavTabs'
import { GO_SHORTCUTS, paletteShortcutLabel } from './utils/keyboardShortcuts'
import {
  onPreferencesChanged,
  readSingleKeyShortcutsPreference,
  SINGLE_KEY_SHORTCUTS_KEY,
  PALETTE_BUTTON_KEY,
  readPaletteButtonPreference,
} from './utils/preferences'
import { readRecentTasks, type RecentTask } from './utils/recentTasks'
import { applyThemePreference, storeThemePreference, type ThemePreference } from './utils/theme'
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
// DEV-218: repository logo, name and theme default.
const branding = useBranding()
const appName = computed(() => branding.name ?? DEFAULT_APP_NAME)
const brandIsCustom = computed(() => Boolean(branding.logo || branding.name))
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

const { visibleTabs } = useNavTabs()
const visibleNavItems = computed(() => [...visibleTabs.value, { label: 'Preferences', path: '/preferences' }])

function isActive(item: { path: string }) {
  const currentPath = route.path
  return currentPath === item.path || (item.path === '/' && currentPath.startsWith('/task/'))
}

function go(path: string) {
  router.push(path)
}

watch(activityOpen, (open) => {
  if (open) {
    closeTaskPanel()
  }
})

// -- Keyboard shortcuts and command palette (DEV-158) ---------------------------------
const paletteOpen = ref(false)
const shortcutsOpen = ref(false)
const paletteLabel = paletteShortcutLabel()
const singleKeyShortcuts = ref(readSingleKeyShortcutsPreference())
// DEV-219: the palette stays on its shortcut when the button is hidden.
const showPaletteButton = ref(readPaletteButtonPreference())
let stopPreferences: (() => void) | null = null
onMounted(() => {
  stopPreferences = onPreferencesChanged((key) => {
    if (key === SINGLE_KEY_SHORTCUTS_KEY) singleKeyShortcuts.value = readSingleKeyShortcutsPreference()
    if (key === PALETTE_BUTTON_KEY) showPaletteButton.value = readPaletteButtonPreference()
  })
})
onUnmounted(() => stopPreferences?.())

function goKeys(path: string): string | undefined {
  const entry = GO_SHORTCUTS.find((shortcut) => shortcut.path === path)
  return entry ? `G ${entry.key.toUpperCase()}` : undefined
}
function setTheme(theme: ThemePreference) {
  applyThemePreference(theme)
  storeThemePreference(theme)
}

// DEV-198: the open task's actions, recent tasks and the page's quick filters.
const { panelCommands } = useTaskPanelController()
const activeFilterBar = useActiveFilterBar()
const recentTasks = ref<RecentTask[]>([])
watch(paletteOpen, (open) => {
  if (open) recentTasks.value = readRecentTasks()
})

async function copyText(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    showToast(`Copied ${what}`)
  } catch {
    showToast(`Could not copy ${what}`)
  }
}

const taskCommands = computed<PaletteCommand[]>(() => {
  const open = panelCommands.value
  if (!open) return []
  const commands: PaletteCommand[] = [
    ...open.statuses
      .filter((status) => status !== open.status)
      .map((status) => ({
        id: `task-status-${status}`,
        group: 'This task' as const,
        label: `Set status to ${status}`,
        keywords: `${open.taskId} move state`,
        run: () => void open.setStatus(status),
      })),
    ...open.priorities
      .filter((priority) => priority !== open.priority)
      .map((priority) => ({
        id: `task-priority-${priority}`,
        group: 'This task' as const,
        label: `Set priority to ${priority}`,
        keywords: open.taskId,
        run: () => void open.setPriority(priority),
      })),
    {
      id: 'task-assign-me',
      group: 'This task',
      label: 'Assign to me',
      keywords: `${open.taskId} assignee take`,
      run: () => void open.assignToMe(),
    },
    {
      id: 'task-toggle-view',
      group: 'This task',
      label: open.isLargeView() ? 'Switch to side panel' : 'Switch to large view',
      keywords: `${open.taskId} expand bigger wide full screen layout`,
      run: () => open.toggleView(),
    },
    {
      id: 'task-copy-id',
      group: 'This task',
      label: `Copy task ID ${open.taskId}`,
      keywords: 'clipboard',
      run: () => void copyText(open.taskId, open.taskId),
    },
    {
      id: 'task-copy-link',
      group: 'This task',
      label: 'Copy link to this task',
      keywords: `${open.taskId} url clipboard share`,
      run: () => void copyText(`${window.location.origin}/task/${encodeURIComponent(open.taskId)}`, 'link'),
    },
  ]
  return commands
})

const recentCommands = computed<PaletteCommand[]>(() =>
  recentTasks.value
    .filter((recent) => recent.id !== panelCommands.value?.taskId)
    .map((recent) => ({
      id: `recent-${recent.id}`,
      group: 'Recent tasks' as const,
      label: recent.title || recent.id,
      detail: recent.id,
      emptyOnly: true,
      run: () => openTaskPanel({ taskId: recent.id }),
    })),
)

const QUICK_FILTERS: Array<{ id: string; label: string; keywords: string; filters: Record<string, string> }> = [
  { id: 'mine', label: 'Show my tasks', keywords: 'assigned me mine', filters: { assignee: '@me' } },
  { id: 'overdue', label: 'Show overdue tasks', keywords: 'late due', filters: { due: 'overdue' } },
  {
    id: 'due-soon',
    label: 'Show tasks due in the next 7 days',
    keywords: 'soon deadline week',
    filters: { due: 'soon' },
  },
  { id: 'unassigned', label: 'Show unassigned tasks', keywords: 'nobody owner', filters: { assignee: '__none__' } },
  {
    id: 'recent',
    label: 'Show tasks changed in the last 7 days',
    keywords: 'recent modified updated week',
    filters: { recent: '7d' },
  },
]

const filterCommands = computed<PaletteCommand[]>(() => {
  const bar = activeFilterBar.value
  if (!bar) return []
  const commands: PaletteCommand[] = QUICK_FILTERS.map((quick) => ({
    id: `filter-${quick.id}`,
    group: 'Filters' as const,
    label: quick.label,
    keywords: `filter ${quick.keywords}`,
    run: () => bar.apply(quick.filters),
  }))
  commands.push({
    id: 'filter-clear',
    group: 'Filters',
    label: 'Clear filters',
    keywords: 'reset remove',
    run: bar.clear,
  })
  const projects = bar.projects()
  if (projects.length > 1) {
    const current = bar.currentProject()
    for (const project of projects) {
      if (project.prefix === current) continue
      commands.push({
        id: `filter-project-${project.prefix}`,
        group: 'Filters',
        label: `Switch to project ${project.name || project.prefix}`,
        detail: project.prefix,
        keywords: 'project change scope',
        run: () => bar.setProject(project.prefix),
      })
    }
    if (current) {
      commands.push({
        id: 'filter-project-all',
        group: 'Filters',
        label: 'Show all projects',
        keywords: 'project scope every',
        run: () => bar.setProject(''),
      })
    }
  }
  return commands
})

const paletteCommands = computed<PaletteCommand[]>(() => [
  ...taskCommands.value,
  ...recentCommands.value,
  ...filterCommands.value,
  ...[...NAV_TABS, { label: 'Preferences', path: '/preferences' }].map((tab) => ({
    id: `go-${tab.path.replace(/\W+/g, '') || 'tasks'}`,
    group: 'Go to' as const,
    label: tab.label,
    keywords: 'go open page view',
    shortcut: goKeys(tab.path),
    run: () => go(tab.path),
  })),
  {
    id: 'new-task',
    group: 'Actions',
    label: 'New task',
    keywords: 'create add issue',
    shortcut: 'C',
    run: openGlobalNewTask,
  },
  {
    id: 'activity',
    group: 'Actions',
    label: 'Show activity',
    keywords: 'recent changes feed',
    run: () => (activityOpen.value = true),
  },
  {
    id: 'shortcuts',
    group: 'Actions',
    label: 'Keyboard shortcuts',
    keywords: 'help keys',
    shortcut: '?',
    run: () => (shortcutsOpen.value = true),
  },
  {
    id: 'theme-light',
    group: 'Actions',
    label: 'Use light theme',
    keywords: 'appearance mode',
    run: () => setTheme('light'),
  },
  {
    id: 'theme-dark',
    group: 'Actions',
    label: 'Use dark theme',
    keywords: 'appearance mode night',
    run: () => setTheme('dark'),
  },
  {
    id: 'theme-system',
    group: 'Actions',
    label: 'Use system theme',
    keywords: 'appearance mode auto',
    run: () => setTheme('system'),
  },
])

useGlobalShortcuts({
  openPalette: () => (paletteOpen.value = true),
  openHelp: () => (shortcutsOpen.value = true),
  newTask: openGlobalNewTask,
  navigate: go,
  singleKeyEnabled: () => singleKeyShortcuts.value,
  // An open panel, drawer or dialog owns the keyboard.
  // The palette opens over the task panel (its "This task" commands), but
  // not over other dialogs.
  paletteBlocked: () =>
    paletteOpen.value ||
    shortcutsOpen.value ||
    [...document.querySelectorAll('[aria-modal="true"]')].some((dialog) => !dialog.classList.contains('task-panel')),
  busy: () =>
    paletteOpen.value ||
    shortcutsOpen.value ||
    activityOpen.value ||
    taskPanelState.open ||
    !!document.querySelector('[aria-modal="true"]'),
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
  flex-shrink: 0;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
}

.nav {
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: thin;
}

.nav > * {
  flex-shrink: 0;
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

/* A theme whose top bar stays dark (or light) in both modes sets
   --topbar-logo-light / --topbar-logo-dark to pick the logo itself. */
.brand__logo--light {
  display: var(--topbar-logo-light, block);
}

.brand__logo--dark {
  display: var(--topbar-logo-dark, none);
}

/* The whole selector sits in :global(); Vue drops anything after it. */
:global(html[data-mode='dark'] .brand__logo--light) {
  display: var(--topbar-logo-light, none);
}

:global(html[data-mode='dark'] .brand__logo--dark) {
  display: var(--topbar-logo-dark, block);
}

/* Repository logos can have any shape; keep them within the header. */
.brand__logo--custom {
  max-width: 180px;
  object-fit: contain;
}

.brand--custom {
  align-items: center;
  gap: 8px;
}

.brand__name.brand__name--visible {
  position: static;
  width: auto;
  height: auto;
  margin: 0;
  overflow: hidden;
  clip: auto;
  clip-path: none;
  max-width: 220px;
  text-overflow: ellipsis;
  font-family: var(--font-heading);
  font-weight: var(--heading-weight);
  text-transform: var(--heading-transform);
  letter-spacing: var(--heading-tracking);
  text-shadow: var(--text-glow);
  font-size: 1.05rem;
  color: var(--topbar-fg);
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

  .palette-trigger {
    width: 2.25rem;
    height: 2.25rem;
    min-width: auto;
    padding: 0;
  }

  .palette-trigger__keys {
    display: none;
  }
}

/* The two top-bar buttons sit closer together than the group gap: their own
   padding already separates them, so the visual gap matches logo-to-button. */
.topbar-actions {
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.palette-trigger {
  gap: var(--space-2);
  padding-inline: var(--space-2);
  white-space: nowrap;
}
</style>
