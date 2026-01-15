<template>
  <section class="preferences">
    <h1>Preferences</h1>

    <div class="preferences-layout">
      <div class="preference-column">
        <UiCard>
          <h3>Theme</h3>
          <div class="col" style="gap:8px;">
            <label class="row" style="gap:8px; align-items:center;">
              <input type="radio" name="theme" value="system" v-model="theme" /> System (follow OS)
            </label>
            <label class="row" style="gap:8px; align-items:center;">
              <input type="radio" name="theme" value="light" v-model="theme" /> Light
            </label>
            <label class="row" style="gap:8px; align-items:center;">
              <input type="radio" name="theme" value="dark" v-model="theme" /> Dark
            </label>
          </div>
        </UiCard>

        <UiCard>
          <h3>Accent color</h3>
          <div class="col" style="gap:12px;">
            <label class="row" style="gap:8px; align-items:center;">
              <input type="checkbox" v-model="accentEnabled" /> Use custom accent color
            </label>
            <div v-if="accentEnabled" class="accent-controls row" style="gap:12px; align-items:center; flex-wrap: wrap;">
              <input class="accent-color" type="color" v-model="accent" aria-label="Accent color" />
              <UiInput class="accent-hex" v-model="accent" maxlength="7" placeholder="#rrggbb" aria-label="Accent color hex" />
              <span class="chip preview" :style="{ background: accentPreview, color: accentPreviewContrast }">Preview</span>
              <UiButton variant="ghost" @click="resetAccent">Reset</UiButton>
            </div>
            <p class="muted" style="margin:0;">Accent affects highlights, focus rings, and selection states across the app.</p>
          </div>
        </UiCard>
      </div>

      <div class="preference-column">
        <UiCard>
          <h3>Startup destination</h3>
          <div class="col" style="gap:8px;">
            <UiSelect v-model="startupDestination">
              <option v-for="option in startupDestinationOptions" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </UiSelect>
            <p class="muted" style="margin:0;">Choose where LoTaR lands after loading. “Remember where I left off” reopens the last main section you visited.</p>
          </div>
        </UiCard>

        <UiCard>
          <h3>Tables</h3>
          <div class="row" style="gap:8px; align-items:center; flex-wrap: wrap;">
            <UiButton @click="resetTables">Reset saved columns and sorting</UiButton>
            <span class="muted">Clears saved TaskTable column visibility and sort for all projects.</span>
          </div>
        </UiCard>

        <UiCard>
          <h3>Filters</h3>
          <div class="row" style="gap:8px; align-items:center; flex-wrap: wrap;">
            <UiButton @click="clearSavedFilters">Clear last used filter</UiButton>
            <span class="muted">Removes the saved FilterBar state from this browser.</span>
          </div>
        </UiCard>

        <UiCard>
          <h3>Tasks list</h3>
          <div class="col" style="gap:8px;">
            <label class="row" style="gap:8px; align-items:center; flex-wrap: wrap;">
              <span class="muted">Default page size</span>
              <UiSelect v-model="tasksPageSize">
                <option v-for="opt in tasksPageSizeOptions" :key="opt" :value="String(opt)">
                  {{ opt }}
                </option>
              </UiSelect>
            </label>
            <p class="muted" style="margin:0;">Used when the Tasks list URL does not specify a page size.</p>
          </div>
        </UiCard>

        <UiCard>
          <h3>Task panel</h3>
          <div class="col" style="gap:10px;">
            <label class="row" style="gap:8px; align-items:center;">
              <input type="checkbox" v-model="showAttachments" /> Show attachments
            </label>
            <label class="row" style="gap:8px; align-items:center;">
              <input type="checkbox" v-model="showLinksInAttachments" :disabled="!showAttachments" /> Show link references next to attachments
            </label>
            <label class="row" style="gap:8px; align-items:center;">
              <input type="checkbox" v-model="autoDetectLinks" :disabled="!showAttachments || !showLinksInAttachments" /> Auto-detect links in title/description
            </label>
            <p class="muted" style="margin:0;">Link detection adds missing link references when you edit a task.</p>
          </div>
        </UiCard>

      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import UiSelect from '../components/UiSelect.vue'
import { getContrastingColor } from '../utils/color'
import {
    DEFAULT_STARTUP_DESTINATION,
    DEFAULT_TASKS_PAGE_SIZE,
    readStartupDestination,
    readTaskPanelAutoDetectLinksPreference,
    readTaskPanelShowAttachmentsPreference,
    readTaskPanelShowLinksInAttachmentsPreference,
    readTasksPageSizePreference,
    STARTUP_DESTINATION_OPTIONS,
    storeStartupDestination,
    storeTaskPanelAutoDetectLinksPreference,
    storeTaskPanelShowAttachmentsPreference,
    storeTaskPanelShowLinksInAttachmentsPreference,
    storeTasksPageSizePreference,
    type StartupDestination,
} from '../utils/preferences'
import {
    applyAccentPreference,
    applyThemePreference,
    DEFAULT_ACCENT,
    normalizeAccent,
    readAccentPreference,
    readThemePreference,
    storeAccentPreference,
    storeThemePreference,
    type ThemePreference,
} from '../utils/theme'

const theme = ref<ThemePreference>('system')
const accent = ref<string>(DEFAULT_ACCENT)
const accentEnabled = ref(false)
const startupDestination = ref<StartupDestination>(DEFAULT_STARTUP_DESTINATION)
const startupDestinationOptions = STARTUP_DESTINATION_OPTIONS

const showAttachments = ref(true)
const showLinksInAttachments = ref(true)
const autoDetectLinks = ref(true)

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

function resetTables(){
  try {
    const keys = Object.keys(localStorage)
    for (const k of keys) {
      if (
        k.startsWith('lotar.taskTable.columns') ||
        k.startsWith('lotar.taskTable.sort') ||
        k.startsWith('lotar.taskTable.columnOrder') ||
        k.startsWith('lotar.sprints.columns') ||
        k.startsWith('lotar.sprints.sort') ||
        k.startsWith('lotar.sprints.columnOrder')
      ) {
        localStorage.removeItem(k)
      }
    }
  } catch {}
}

function clearSavedFilters(){
  const filterKeys = ['lotar.tasks.filter', 'lotar.sprints.filter', 'lotar.calendar.filter', 'lotar.boards.filter']
  try {
    for (const key of filterKeys) localStorage.removeItem(key)
  } catch {}
  // Backwards compatibility: older builds may have used sessionStorage.
  try {
    for (const key of filterKeys) sessionStorage.removeItem(key)
  } catch {}
}
</script>

<style scoped>
.preferences {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
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

.preferences input[type="color"] {
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
