<template>
  <ConfigGroup
    title="Appearance"
    description="How LoTaR looks for everyone using this repository. People can still pick their own theme and mode in Preferences."
  >
    <div class="field-grid" data-testid="config-appearance">
      <div class="field field--full">
        <label class="field-label" for="config-ui-theme">Default theme</label>
        <ThemeSelect
          id="config-ui-theme"
          v-model="themeModel"
          unset-label="Not set"
          :custom-name="uiThemeFile.trim() ? 'Repository skin' : null"
          aria-label="Default theme"
        />
        <p class="field-hint">People who haven't picked their own theme in Preferences see this one.</p>
        <p v-if="homeOverride" class="field-hint field-hint--note" data-testid="config-ui-theme-home">
          Your ~/.lotar sets {{ homeOverride }}, which you see instead. Others see this setting unless their own
          ~/.lotar sets a theme.
        </p>
        <p v-if="errors.ui_theme" class="field-error">{{ errors.ui_theme }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label" for="config-ui-theme-file">Skin file</label>
        <div class="appearance-skin">
          <UiInput
            id="config-ui-theme-file"
            v-model="uiThemeFile"
            maxlength="512"
            placeholder="assets/lotar-theme.css"
            @blur="emit('validate', 'ui_theme_file')"
          />
          <input
            ref="skinPicker"
            type="file"
            accept=".css,text/css"
            hidden
            data-testid="config-ui-theme-file-upload"
            @change="onSkinPicked"
          />
          <UiButton type="button" :disabled="uploading === 'theme'" @click="skinPicker?.click()">
            {{ uploading === 'theme' ? 'Uploading…' : 'Upload…' }}
          </UiButton>
        </div>
        <p class="field-hint">
          Your own theme: a CSS file in the repository that sets theme variables such as
          <code>--color-accent: #c0392b;</code> (rules for dark mode go under a selector or media query mentioning
          <code>dark</code>). Copy a block from LoTaR's themes as a start. Only variables are used; <code>url()</code>,
          imports and other rules are ignored. Then pick <strong>Repository skin</strong> above.
        </p>
        <p v-if="errors.ui_theme_file" class="field-error">{{ errors.ui_theme_file }}</p>
        <p v-for="problem in skinProblems" :key="problem" class="field-error">{{ problem }}</p>
      </div>

      <div class="field field--full">
        <label class="field-label" for="config-ui-name">Name</label>
        <UiInput
          id="config-ui-name"
          v-model="uiName"
          maxlength="40"
          placeholder="LoTaR"
          @blur="emit('validate', 'ui_name')"
        />
        <p class="field-hint">Shown next to the logo and as the browser tab title.</p>
        <p v-if="errors.ui_name" class="field-error">{{ errors.ui_name }}</p>
      </div>

      <div v-for="slot in slots" :key="slot.key" class="field field--full">
        <label class="field-label" :for="`config-ui-${slot.image}`">{{ slot.label }}</label>
        <div class="appearance-image">
          <span
            class="appearance-image__preview"
            :class="{ 'appearance-image__preview--dark': slot.image === 'logo-dark' }"
          >
            <img v-if="previewFor(slot)" :src="previewFor(slot)!" :alt="`${slot.label} preview`" />
            <span v-else class="muted">{{ slot.models.value ? 'Save to preview' : 'None' }}</span>
          </span>
          <UiInput
            :id="`config-ui-${slot.image}`"
            v-model="slot.models.value"
            maxlength="512"
            :placeholder="slot.placeholder"
            @blur="emit('validate', slot.key)"
          />
          <input
            :ref="(el) => (pickers[slot.image] = el as HTMLInputElement | null)"
            type="file"
            accept="image/*,.svg,.ico"
            hidden
            :data-testid="`config-ui-${slot.image}-file`"
            @change="onPicked(slot, $event)"
          />
          <UiButton type="button" :disabled="uploading === slot.image" @click="pickers[slot.image]?.click()">
            {{ uploading === slot.image ? 'Uploading…' : 'Upload…' }}
          </UiButton>
        </div>
        <p class="field-hint">{{ slot.hint }}</p>
        <p v-if="errors[slot.key]" class="field-error">{{ errors[slot.key] }}</p>
        <p v-for="problem in problemsFor(slot)" :key="problem" class="field-error">{{ problem }}</p>
      </div>
    </div>
  </ConfigGroup>
</template>

<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, watch, type Ref } from 'vue'
import { api } from '../api/client'
import { useBranding } from '../composables/useBranding'
import { fileToBase64 } from '../utils/taskAttachments'
import { parseThemeValue, themeLabel } from '../utils/themes'
import { previewConfiguredTheme } from '../utils/theme'
import ConfigGroup from './ConfigGroup.vue'
import ThemeSelect from './ThemeSelect.vue'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'
import { showToast } from './toast'

type FieldKey = 'ui_theme' | 'ui_name' | 'ui_logo' | 'ui_logo_dark' | 'ui_icon' | 'ui_theme_file'
type ImageSlot = 'logo' | 'logo-dark' | 'icon'

const uiTheme = defineModel<string>('uiTheme', { required: true })
const uiName = defineModel<string>('uiName', { required: true })
const uiLogo = defineModel<string>('uiLogo', { required: true })
const uiLogoDark = defineModel<string>('uiLogoDark', { required: true })
const uiIcon = defineModel<string>('uiIcon', { required: true })
const uiThemeFile = defineModel<string>('uiThemeFile', { required: true })

const props = defineProps<{
  errors: Partial<Record<FieldKey, string | null>>
  /** Saved values, to know when the server's image URL matches the field. */
  saved: { uiLogo: string; uiLogoDark: string; uiIcon: string; uiThemeFile: string }
}>()

const emit = defineEmits<{ (e: 'validate', field: FieldKey): void }>()

const branding = useBranding()

// The select's null is "not set" (an empty field).
const themeModel = computed<string | null>({
  get: () => uiTheme.value.trim() || null,
  set: (value) => {
    uiTheme.value = value ?? ''
    emit('validate', 'ui_theme')
  },
})

// Live preview: while editing, show the theme to everyone here who follows the
// configuration (not when their own ~/.lotar overrides it). Leaving the page
// ends the preview, so unsaved edits never stick.
watch(
  () => uiTheme.value,
  (value) => {
    if (branding.themeSource === 'home') return
    previewConfiguredTheme(value.trim() || null)
  },
)
onUnmounted(() => previewConfiguredTheme(undefined))
// A personal ~/.lotar theme wins over the repository's for this person.
const homeOverride = computed(() => {
  if (branding.themeSource !== 'home' || !branding.theme) return null
  const choice = parseThemeValue(branding.theme, true)
  return choice ? themeLabel(choice) : branding.theme
})
const skinProblems = computed(() =>
  uiThemeFile.value.trim() === props.saved.uiThemeFile.trim()
    ? branding.problems.filter((problem) => problem.startsWith('Skin:'))
    : [],
)
const skinPicker = ref<HTMLInputElement | null>(null)

async function onSkinPicked(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  uploading.value = 'theme'
  try {
    const { path } = await api.uploadBranding({
      image: 'theme',
      filename: file.name,
      content_base64: await fileToBase64(file),
    })
    uiThemeFile.value = path
    emit('validate', 'ui_theme_file')
    showToast('Skin uploaded. Pick Repository skin as the theme and save.')
  } catch (error) {
    showToast(error instanceof Error ? error.message.replace(/^POST \S+ failed: /, '') : 'Upload failed')
  } finally {
    uploading.value = null
  }
}

interface Slot {
  key: FieldKey
  image: ImageSlot
  label: string
  placeholder: string
  hint: string
  models: Ref<string>
  saved: () => string
  url: () => string | null
}

const slots: Slot[] = [
  {
    key: 'ui_logo',
    image: 'logo',
    label: 'Logo',
    placeholder: 'assets/logo.svg',
    hint: 'Replaces the LoTaR logo in the header. A path relative to the repository, or upload one (stored in .tasks/@branding).',
    models: uiLogo,
    saved: () => props.saved.uiLogo,
    url: () => branding.logo,
  },
  {
    key: 'ui_logo_dark',
    image: 'logo-dark',
    label: 'Logo for dark mode',
    placeholder: 'Same as the logo',
    hint: 'Optional version for dark backgrounds.',
    models: uiLogoDark,
    saved: () => props.saved.uiLogoDark,
    url: () => branding.logoDark,
  },
  {
    key: 'ui_icon',
    image: 'icon',
    label: 'App icon',
    placeholder: 'assets/app-icon.svg',
    hint: 'Shown in the browser tab, e.g. your app’s own icon. Square images (SVG, PNG or ICO) work best.',
    models: uiIcon,
    saved: () => props.saved.uiIcon,
    url: () => branding.icon,
  },
]

// Local previews of files uploaded but not saved yet.
const pending = reactive<Partial<Record<ImageSlot, { path: string; url: string }>>>({})
const pickers = reactive<Partial<Record<ImageSlot, HTMLInputElement | null>>>({})
const uploading = ref<ImageSlot | 'theme' | null>(null)

function previewFor(slot: Slot): string | null {
  const value = slot.models.value.trim()
  const local = pending[slot.image]
  if (local && local.path === value) return local.url
  if (value && value === slot.saved().trim()) return slot.url()
  return null
}

const SLOT_NAMES: Record<ImageSlot, string> = { logo: 'Logo', 'logo-dark': 'Dark logo', icon: 'Icon' }

function problemsFor(slot: Slot): string[] {
  if (slot.models.value.trim() !== slot.saved().trim()) return []
  return branding.problems.filter((problem) => problem.startsWith(`${SLOT_NAMES[slot.image]}:`))
}

async function onPicked(slot: Slot, event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  uploading.value = slot.image
  try {
    const { path } = await api.uploadBranding({
      image: slot.image,
      filename: file.name,
      content_base64: await fileToBase64(file),
    })
    const previous = pending[slot.image]
    if (previous) URL.revokeObjectURL(previous.url)
    pending[slot.image] = { path, url: URL.createObjectURL(file) }
    slot.models.value = path
    emit('validate', slot.key)
    showToast(`${slot.label} uploaded. Save to apply it.`)
  } catch (error) {
    showToast(error instanceof Error ? error.message.replace(/^POST \S+ failed: /, '') : 'Upload failed')
  } finally {
    uploading.value = null
  }
}

onUnmounted(() => {
  for (const entry of Object.values(pending)) if (entry) URL.revokeObjectURL(entry.url)
})
</script>

<style scoped>
.appearance-skin {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.field-hint--note {
  color: var(--color-fg);
}

.appearance-image {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.appearance-image__preview {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 40px;
  padding: 4px;
  border: 1px dashed var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  font-size: var(--text-xs);
}

/* Show the dark logo the way it will appear: on a dark header. */
.appearance-image__preview--dark {
  background: #111315;
}

.appearance-image__preview img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}

@media (max-width: 640px) {
  .appearance-skin,
  .appearance-image {
    grid-template-columns: 1fr;
  }
}
</style>
