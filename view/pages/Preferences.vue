<template>
  <section class="col preferences" style="gap:16px; max-width: 720px;">
    <h1>Preferences</h1>

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
          <UiInput class="accent-hex" v-model="accent" maxlength="7" placeholder="#0ea5e9" aria-label="Accent color hex" />
          <span class="chip preview" :style="{ background: accentPreview, color: accentPreviewContrast }">Preview</span>
          <UiButton variant="ghost" @click="resetAccent">Reset</UiButton>
        </div>
        <p class="muted" style="margin:0;">Accent affects highlights, focus rings, and selection states across the app.</p>
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
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiInput from '../components/UiInput.vue'
import { getContrastingColor } from '../utils/color'
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

const accentPreview = computed(() => (accentEnabled.value ? accent.value : DEFAULT_ACCENT))
const accentPreviewContrast = computed(() => getContrastingColor(accentPreview.value))

function resetAccent() {
  accentEnabled.value = false
  accent.value = DEFAULT_ACCENT
}

function resetTables(){
  try {
    const keys = Object.keys(localStorage)
    for (const k of keys) { if (k.startsWith('lotar.taskTable.columns') || k.startsWith('lotar.taskTable.sort')) localStorage.removeItem(k) }
  } catch {}
}

function clearSavedFilters(){
  try { sessionStorage.removeItem('lotar.tasks.filter') } catch {}
}
</script>

<style scoped>
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
