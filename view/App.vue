<template>
  <header class="topbar">
    <div class="topbar-inner container">
      <div class="brand-group">
        <a class="brand" href="/" aria-label="LoTaR home" @click.prevent="go('/')">
          <img class="brand__logo brand__logo--light" :src="logoLight" alt="" width="574" height="152" />
          <img class="brand__logo brand__logo--dark" :src="logoDark" alt="" width="574" height="152" />
          <span class="brand__name">LoTaR</span>
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
import TaskPanelHost from './components/TaskPanelHost.vue'
import ToastHost from './components/ToastHost.vue'
import UiButton from './components/UiButton.vue'
import { showToast } from './components/toast'
import { useGlobalShortcuts } from './composables/useGlobalShortcuts'
import { useTaskPanelController } from './composables/useTaskPanelController'
import { NAV_TABS, useNavTabs } from './composables/useNavTabs'
import { GO_SHORTCUTS, paletteShortcutLabel } from './utils/keyboardShortcuts'
import { onPreferencesChanged, readSingleKeyShortcutsPreference, SINGLE_KEY_SHORTCUTS_KEY } from './utils/preferences'
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
let stopPreferences: (() => void) | null = null
onMounted(() => {
  stopPreferences = onPreferencesChanged((key) => {
    if (key === SINGLE_KEY_SHORTCUTS_KEY) singleKeyShortcuts.value = readSingleKeyShortcutsPreference()
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

const paletteCommands = computed<PaletteCommand[]>(() => [
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

.brand__logo--dark {
  display: none;
}

@media (prefers-color-scheme: dark) {
  .brand__logo--light {
    display: none;
  }
  .brand__logo--dark {
    display: block;
  }
}

:global(html[data-theme='light'] .brand__logo--light),
:global(html[data-theme='dark'] .brand__logo--dark) {
  display: block;
}

:global(html[data-theme='light'] .brand__logo--dark),
:global(html[data-theme='dark'] .brand__logo--light) {
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
