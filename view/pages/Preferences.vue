<template>
  <section class="preferences">
    <PageHeader><template #title>Preferences</template></PageHeader>

    <UiCard class="preferences-appearance">
      <h3>Appearance</h3>
      <div class="appearance">
        <fieldset class="appearance__group">
          <legend>Mode</legend>
          <div class="row" style="gap: 16px; flex-wrap: wrap">
            <label class="row" style="gap: 8px; align-items: center">
              <input type="radio" name="theme" value="system" v-model="theme" /> System (follow OS)
            </label>
            <label class="row" style="gap: 8px; align-items: center">
              <input type="radio" name="theme" value="light" v-model="theme" /> Light
            </label>
            <label class="row" style="gap: 8px; align-items: center">
              <input type="radio" name="theme" value="dark" v-model="theme" /> Dark
            </label>
          </div>
        </fieldset>

        <fieldset class="appearance__group">
          <legend>Theme</legend>
          <ThemeSelect
            v-model="themeChoice"
            :configured="configuredTheme"
            :configured-source="configuredSource"
            :custom-name="branding.themeFile ? 'Repository skin' : null"
            aria-label="Theme"
          />
          <p class="muted appearance__note">{{ configuredNote }}</p>
        </fieldset>

        <fieldset class="appearance__group">
          <legend>Accent color</legend>
          <div class="col" style="gap: 12px">
            <label class="row" style="gap: 8px; align-items: center">
              <input type="checkbox" v-model="accentEnabled" /> Use custom accent color
            </label>
            <div
              v-if="accentEnabled"
              class="accent-controls row"
              style="gap: 12px; align-items: center; flex-wrap: wrap"
            >
              <input class="accent-color" type="color" v-model="accent" aria-label="Accent color" />
              <UiInput
                class="accent-hex"
                v-model="accent"
                maxlength="7"
                placeholder="#rrggbb"
                aria-label="Accent color hex"
              />
              <span class="chip preview" :style="{ background: accentPreview, color: accentPreviewContrast }"
                >Preview</span
              >
              <UiButton variant="ghost" @click="resetAccent">Reset</UiButton>
            </div>
            <p class="muted" style="margin: 0">
              Replaces the theme's accent for highlights, focus rings and selection states, in every theme.
            </p>
          </div>
        </fieldset>
      </div>
    </UiCard>

    <div class="preferences-layout">
      <div class="preference-column">
        <UiCard>
          <h3>Startup destination</h3>
          <div class="col" style="gap: 8px">
            <UiSelect v-model="startupDestination" class="preferences__startup">
              <option v-for="option in startupDestinationOptions" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </UiSelect>
            <p class="muted" style="margin: 0">
              Choose where LoTaR lands after loading. “Remember where I left off” reopens the last main section you
              visited.
            </p>
          </div>
        </UiCard>

        <UiCard>
          <h3>Navigation</h3>
          <div class="col" style="gap: 8px">
            <p class="muted" style="margin: 0">
              Hide tabs you don't use to reduce clutter. The pages stay reachable by URL.
            </p>
            <div class="preferences__tabs row" style="gap: 8px; flex-wrap: wrap">
              <label v-for="tab in NAV_TABS" :key="tab.path" class="row" style="gap: 6px; align-items: center">
                <input
                  type="checkbox"
                  :checked="isTabVisible(tab.path)"
                  @change="setTabHidden(tab.path, !($event.target as HTMLInputElement).checked)"
                />
                <span>{{ tab.label }}</span>
              </label>
            </div>
          </div>
        </UiCard>

        <UiCard>
          <h3>Keyboard</h3>
          <div class="col" style="gap: 10px">
            <label class="row" style="gap: 8px; align-items: center">
              <input v-model="singleKeyShortcuts" type="checkbox" data-testid="preferences-single-key-shortcuts" />
              Single-key shortcuts
            </label>
            <label class="row" style="gap: 8px; align-items: center">
              <input v-model="showPaletteButton" type="checkbox" data-testid="preferences-palette-button" />
              Show the command palette button in the header
            </label>
            <p class="muted" style="margin: 0">
              <kbd>c</kbd> new task, <kbd>/</kbd> search, <kbd>g</kbd> then a letter to change pages, and
              <kbd>?</kbd> for the full list. Turn them off if they get in the way of a screen reader or voice control;
              {{ paletteLabel }} opens the command palette either way.
            </p>
          </div>
        </UiCard>

        <UiCard>
          <h3>Tables</h3>
          <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap">
            <UiButton @click="resetTables">Reset saved columns and sorting</UiButton>
            <span class="muted"
              >Clears saved table columns, sorting, and board/calendar field visibility for all projects.</span
            >
          </div>
        </UiCard>
      </div>

      <div class="preference-column">
        <UiCard>
          <h3>Tasks list</h3>
          <div class="col" style="gap: 8px">
            <label class="row" style="gap: 8px; align-items: center; flex-wrap: wrap">
              <span class="muted">Default page size</span>
              <UiSelect v-model="tasksPageSize">
                <option v-for="opt in tasksPageSizeOptions" :key="opt" :value="String(opt)">
                  {{ opt }}
                </option>
              </UiSelect>
            </label>
            <p class="muted" style="margin: 0">Used when the Tasks list URL does not specify a page size.</p>
          </div>
        </UiCard>

        <UiCard>
          <h3>Filters</h3>
          <div class="col" style="gap: 10px">
            <label class="row" style="gap: 8px; align-items: center">
              <input v-model="shareFilters" type="checkbox" data-testid="preferences-share-filters" />
              Use the same search and filters on every page
            </label>
            <p class="muted" style="margin: 0">
              Tasks, Boards, Calendar, Sprints, Insights and Dependencies then keep one search and filter, so you can
              switch views without setting it up again. When off, each page remembers its own.
            </p>
            <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap">
              <UiButton @click="clearSavedFilters">Clear last used filter</UiButton>
              <span class="muted">Removes the saved search and filters of every page from this browser.</span>
            </div>
          </div>
        </UiCard>

        <UiCard>
          <h3>Task panel</h3>
          <div class="col" style="gap: 10px">
            <label class="row" style="gap: 8px; align-items: center">
              <input type="checkbox" v-model="showAttachments" /> Show attachments
            </label>
            <label class="row" style="gap: 8px; align-items: center">
              <input type="checkbox" v-model="showLinksInAttachments" :disabled="!showAttachments" /> Show link
              references next to attachments
            </label>
            <label class="row" style="gap: 8px; align-items: center">
              <input
                type="checkbox"
                v-model="autoDetectLinks"
                :disabled="!showAttachments || !showLinksInAttachments"
              />
              Auto-detect links in title/description
            </label>
            <p class="muted" style="margin: 0">Link detection adds missing link references when you edit a task.</p>
          </div>
        </UiCard>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import PageHeader from '../components/PageHeader.vue'
import { computed, onMounted, ref, watch } from 'vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiSelect from '../components/UiSelect.vue'
import { NAV_TABS, useNavTabs } from '../composables/useNavTabs'
import { getContrastingColor } from '../utils/color'
import { paletteShortcutLabel } from '../utils/keyboardShortcuts'
import {
  DEFAULT_STARTUP_DESTINATION,
  DEFAULT_TASKS_PAGE_SIZE,
  readShareFiltersPreference,
  readSingleKeyShortcutsPreference,
  readStartupDestination,
  readTaskPanelAutoDetectLinksPreference,
  SHARED_FILTER_STORAGE_KEY,
  readTaskPanelShowAttachmentsPreference,
  readTaskPanelShowLinksInAttachmentsPreference,
  readTasksPageSizePreference,
  STARTUP_DESTINATION_OPTIONS,
  storeShareFiltersPreference,
  storeSingleKeyShortcutsPreference,
  storePaletteButtonPreference,
  readPaletteButtonPreference,
  storeStartupDestination,
  storeTaskPanelAutoDetectLinksPreference,
  storeTaskPanelShowAttachmentsPreference,
  storeTaskPanelShowLinksInAttachmentsPreference,
  storeTasksPageSizePreference,
  type StartupDestination,
} from '../utils/preferences'
import {
  applyAccentPreference,
  applyThemeName,
  applyThemePreference,
  DEFAULT_ACCENT,
  getRepositoryTheme,
  normalizeAccent,
  readAccentPreference,
  readThemeNamePreference,
  readThemePreference,
  storeAccentPreference,
  storeThemeNamePreference,
  storeThemePreference,
  type ThemePreference,
} from '../utils/theme'
import ThemeSelect from '../components/ThemeSelect.vue'
import { useBranding } from '../composables/useBranding'

const { isTabVisible, setTabHidden } = useNavTabs()
const theme = ref<ThemePreference>('system')
// DEV-218: the user's colour theme; null follows the repository default.
// DEV-220: one list; the configured theme is tagged. Choosing it stores
// nothing (the browser follows the configuration); anything else is kept here.
const branding = useBranding()
const configuredTheme = computed(() => getRepositoryTheme())
// Usually a person's own ~/.lotar sets this; otherwise the repository.
const configuredSource = computed(() => {
  if (branding.themeSource === 'home') return 'your ~/.lotar'
  if (branding.themeSource === 'repository') return "this repository's Config"
  return 'LoTaR (nothing configured)'
})
const configuredNote = computed(() => {
  const where =
    branding.themeSource === 'home'
      ? 'the theme in your ~/.lotar'
      : branding.themeSource === 'repository'
        ? "this repository's theme (Config → Appearance)"
        : "LoTaR's default, as no theme is configured"
  return `Marked Configured: ${where}. Picking it follows the configuration; other choices are kept in this browser.`
})
const storedTheme = ref<string | null>(readThemeNamePreference())
const themeChoice = computed<string | null>({
  get: () => storedTheme.value ?? configuredTheme.value,
  set: (value) => {
    const next = value && value !== configuredTheme.value ? value : null
    storedTheme.value = next
    storeThemeNamePreference(next)
    applyThemeName(next)
  },
})
const accent = ref<string>(DEFAULT_ACCENT)
const accentEnabled = ref(false)
const startupDestination = ref<StartupDestination>(DEFAULT_STARTUP_DESTINATION)
const startupDestinationOptions = STARTUP_DESTINATION_OPTIONS

const showAttachments = ref(true)
const showLinksInAttachments = ref(true)
const autoDetectLinks = ref(true)

const shareFilters = ref(false)
const singleKeyShortcuts = ref(true)
const showPaletteButton = ref(readPaletteButtonPreference())
const paletteLabel = paletteShortcutLabel()

const tasksPageSizeOptions = [25, 50, 100, 200]
const tasksPageSize = ref(String(DEFAULT_TASKS_PAGE_SIZE))

onMounted(() => {
  const storedTheme = readThemePreference()
  theme.value = storedTheme
  applyThemePreference(storedTheme)

  const storedAccent = readAccentPreference()
  if (storedAccent) {
    accent.value = storedAccent
    accentEnabled.value = true
    applyAccentPreference(storedAccent)
  } else {
    accent.value = DEFAULT_ACCENT
    accentEnabled.value = false
    applyAccentPreference(null)
  }

  startupDestination.value = readStartupDestination()

  tasksPageSize.value = String(readTasksPageSizePreference())
  shareFilters.value = readShareFiltersPreference()
  singleKeyShortcuts.value = readSingleKeyShortcutsPreference()

  showAttachments.value = readTaskPanelShowAttachmentsPreference()
  showLinksInAttachments.value = readTaskPanelShowLinksInAttachmentsPreference()
  autoDetectLinks.value = readTaskPanelAutoDetectLinksPreference()
})

watch(theme, (value) => {
  applyThemePreference(value)
  storeThemePreference(value)
})

watch(accentEnabled, (enabled) => {
  if (enabled) {
    const normalized = normalizeAccent(accent.value) || DEFAULT_ACCENT
    if (normalized !== accent.value) accent.value = normalized
    applyAccentPreference(normalized)
    storeAccentPreference(normalized)
  } else {
    applyAccentPreference(null)
    storeAccentPreference(null)
  }
})

watch(accent, (value) => {
  if (!accentEnabled.value) return
  const normalized = normalizeAccent(value)
  if (!normalized) return
  if (normalized !== value) {
    accent.value = normalized
    return
  }
  applyAccentPreference(normalized)
  storeAccentPreference(normalized)
})

watch(startupDestination, (value) => {
  storeStartupDestination(value)
})

watch(shareFilters, (value) => {
  storeShareFiltersPreference(!!value)
})

watch(singleKeyShortcuts, (value) => {
  storeSingleKeyShortcutsPreference(!!value)
})

watch(showPaletteButton, (value) => {
  storePaletteButtonPreference(!!value)
})

watch(showAttachments, (value) => {
  storeTaskPanelShowAttachmentsPreference(!!value)
  if (!value) {
    showLinksInAttachments.value = false
    autoDetectLinks.value = false
  }
})

watch(showLinksInAttachments, (value) => {
  storeTaskPanelShowLinksInAttachmentsPreference(!!value)
  if (!value) {
    autoDetectLinks.value = false
  }
})

watch(autoDetectLinks, (value) => {
  storeTaskPanelAutoDetectLinksPreference(!!value)
})

watch(tasksPageSize, (value) => {
  const parsed = Number.parseInt(value, 10)
  storeTasksPageSizePreference(Number.isFinite(parsed) ? parsed : DEFAULT_TASKS_PAGE_SIZE)
})

const accentPreview = computed(() => (accentEnabled.value ? accent.value : DEFAULT_ACCENT))
const accentPreviewContrast = computed(() => getContrastingColor(accentPreview.value))

function resetAccent() {
  accentEnabled.value = false
  accent.value = DEFAULT_ACCENT
}

function resetTables() {
  const prefixes = [
    'lotar.taskTable.columns',
    'lotar.taskTable.sort',
    'lotar.taskTable.columnOrder',
    'lotar.sprints.columns',
    'lotar.sprints.sort',
    'lotar.sprints.columnOrder',
    'lotar.boardFields',
    'lotar.boardSort',
    'lotar.calendarHoverFields',
  ]
  try {
    const keys = Object.keys(localStorage)
    for (const k of keys) {
      if (prefixes.some((p) => k.startsWith(p))) {
        localStorage.removeItem(k)
      }
    }
  } catch {
    // Storage can be unavailable (private mode, blocked site data).
  }
}

function clearSavedFilters() {
  const filterKeys = [
    'lotar.tasks.filter',
    'lotar.sprints.filter',
    'lotar.calendar.filter',
    'lotar.boards.filter',
    'lotar.insights.filter',
    'lotar.dependencies.filter',
    SHARED_FILTER_STORAGE_KEY,
  ]
  try {
    for (const key of filterKeys) localStorage.removeItem(key)
  } catch {
    // Storage can be unavailable (private mode, blocked site data).
  }
  // Backwards compatibility: older builds may have used sessionStorage.
  try {
    for (const key of filterKeys) sessionStorage.removeItem(key)
  } catch {
    // Storage can be unavailable (private mode, blocked site data).
  }
}
</script>

<style scoped>
.preferences {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
}

.appearance {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.appearance__group {
  margin: 0;
  padding: 0;
  border: 0;
  min-width: 0;
}

/* Sized to its options, not the card. */
.preferences__startup {
  width: auto;
  max-width: 22rem;
}

.appearance__note {
  margin: 8px 0 0;
  font-size: var(--text-xs);
}

.appearance__group legend {
  margin-bottom: 8px;
  padding: 0;
  font-weight: 600;
}

.preferences-layout {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
  align-items: flex-start;
}

.preference-column {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.preferences input[type='color'] {
  border: none;
  background: transparent;
  padding: 0;
  width: 44px;
  height: 32px;
  cursor: pointer;
}

.accent-controls .accent-hex {
  max-width: 120px;
}

.chip.preview {
  border: 1px solid var(--color-border);
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
</style>
