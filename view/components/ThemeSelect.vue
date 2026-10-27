<template>
  <div ref="root" class="theme-select" :class="{ 'theme-select--open': open }">
    <div class="theme-select__anchor">
      <button
        :id="id"
        ref="trigger"
        type="button"
        class="theme-select__trigger"
        aria-haspopup="listbox"
        :aria-expanded="open"
        :aria-controls="listId"
        :aria-label="ariaLabel ? `${ariaLabel}: ${currentLabel}` : undefined"
        data-testid="theme-select"
        @click="toggle"
        @keydown="onTriggerKey"
      >
        <ThemePreview class="theme-select__preview" :theme="shown.theme" :palette="shownPalette" />
        <span class="theme-select__text">
          <span class="theme-select__label">{{ currentLabel }}</span>
          <span v-if="currentHint" class="theme-select__hint">{{ currentHint }}</span>
        </span>
        <span v-if="isConfigured(shownValue) && !isUnset" class="theme-select__tag" :title="configuredSource">{{
          configuredTag
        }}</span>
        <IconGlyph class="theme-select__chevron" name="chevron-down" />
      </button>

      <div
        v-if="open"
        :id="listId"
        ref="list"
        class="theme-select__list"
        role="listbox"
        tabindex="-1"
        :aria-activedescendant="activeId"
        :aria-label="ariaLabel || 'Theme'"
        @keydown="onListKey"
      >
        <template v-for="group in groups" :key="group.label">
          <div class="theme-select__group" role="presentation">{{ group.label }}</div>
          <div
            v-for="option in group.options"
            :id="optionId(option.value)"
            :key="option.value"
            role="option"
            class="theme-select__option"
            :class="{
              'theme-select__option--active': option.value === activeValue,
              'theme-select__option--selected': option.value === selectedKey,
            }"
            :aria-selected="option.value === selectedKey"
            :data-value="option.value"
            @mousemove="activeValue = option.value"
            @click="choose(option.value)"
          >
            <ThemePreview class="theme-select__preview" :theme="option.theme" :palette="option.palette" />
            <span class="theme-select__text">
              <span class="theme-select__label">{{ option.name }}</span>
              <span class="theme-select__hint">{{ option.description }}</span>
            </span>
            <span v-if="option.configured" class="theme-select__tag" :title="configuredSource">{{
              configuredTag
            }}</span>
          </div>
        </template>
      </div>
    </div>

    <div
      v-if="palettes.length > 1 && !isUnset"
      class="theme-select__palettes"
      role="radiogroup"
      :aria-label="`${themeInfo?.name ?? 'Theme'} palette`"
      data-testid="theme-palettes"
    >
      <button
        v-for="option in palettes"
        :key="option.id"
        type="button"
        role="radio"
        class="theme-select__palette"
        :class="{ 'theme-select__palette--selected': option.id === shownPalette }"
        :aria-checked="option.id === shownPalette"
        :tabindex="option.id === shownPalette ? 0 : -1"
        :title="isConfigured(paletteValue(option.id)) ? `${option.name} (${configuredTag.toLowerCase()})` : option.name"
        @click="choosePalette(option.id)"
        @keydown="onPaletteKey($event, option.id)"
      >
        <span class="theme-select__swatch" :style="{ background: option.swatch }"></span>
        <span>{{ option.name }}</span>
        <span v-if="isConfigured(paletteValue(option.id))" class="theme-select__dot" aria-hidden="true"></span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import IconGlyph from './IconGlyph.vue'
import ThemePreview from './ThemePreview.vue'
import {
  CUSTOM_THEME,
  DEFAULT_THEME,
  effectivePalette,
  findTheme,
  formatThemeValue,
  parseThemeValue,
  THEMES,
  themeLabel,
  type PaletteInfo,
  type ThemeChoice,
} from '../utils/themes'

/** Selected value (`skin` or `skin:palette`); null = not set (see `unsetLabel`). */
const model = defineModel<string | null>({ required: true })

const props = withDefaults(
  defineProps<{
    /** Value the configuration resolves to; that option gets a tag. */
    configured?: string | null
    /** Where the configured value comes from, shown on hover. */
    configuredSource?: string
    /** Tag text for the configured option. */
    configuredTag?: string
    /** When set, null is a valid choice shown with this label (e.g. "Not set"). */
    unsetLabel?: string
    /** Name of the repository skin; offered as an option when set. */
    customName?: string | null
    id?: string
    ariaLabel?: string
  }>(),
  {
    configured: null,
    configuredSource: '',
    configuredTag: 'Configured',
    unsetLabel: undefined,
    customName: null,
    id: undefined,
    ariaLabel: '',
  },
)

const uid = `theme-select-${Math.random().toString(36).slice(2, 8)}`
const listId = `${uid}-list`
const UNSET = '__unset__'

const open = ref(false)
const activeValue = ref<string | null>(null)
const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const list = ref<HTMLElement | null>(null)

const isUnset = computed(() => model.value === null && props.unsetLabel !== undefined)
// What the trigger shows: the choice, or what "not set" resolves to.
const shownValue = computed(() => model.value ?? props.configured ?? DEFAULT_THEME)
const shown = computed<ThemeChoice>(
  () => parseThemeValue(shownValue.value, true) ?? { theme: DEFAULT_THEME, palette: null },
)
const shownPalette = computed(() => effectivePalette(shown.value))
const themeInfo = computed(() => findTheme(shown.value.theme))
const palettes = computed<readonly PaletteInfo[]>(() => themeInfo.value?.palettes ?? [])
const selectedKey = computed(() => (isUnset.value ? UNSET : shown.value.theme))

const customLabel = computed(() => props.customName ?? 'Repository skin')
const currentLabel = computed(() =>
  isUnset.value ? (props.unsetLabel ?? '') : themeLabel(shown.value, customLabel.value),
)
const currentHint = computed(() => {
  if (isUnset.value) return themeLabel(shown.value, customLabel.value)
  return shown.value.theme === CUSTOM_THEME ? 'From the repository' : (themeInfo.value?.description ?? '')
})

function sameChoice(a: string | null | undefined, b: string | null | undefined): boolean {
  const left = parseThemeValue(a, true)
  const right = parseThemeValue(b, true)
  return !!left && !!right && formatThemeValue(left) === formatThemeValue(right)
}
function isConfigured(value: string | null): boolean {
  return !!props.configured && sameChoice(value, props.configured)
}
function paletteValue(palette: string): string {
  return formatThemeValue({ theme: shown.value.theme, palette })
}

interface Option {
  value: string
  theme: string
  palette: string | null
  name: string
  description: string
  configured: boolean
}

const configuredSkin = computed(() => parseThemeValue(props.configured, true)?.theme ?? null)

const groups = computed(() => {
  const option = (theme: string, name: string, description: string): Option => ({
    value: theme,
    theme,
    // Show the current palette for the current skin, else the skin's default.
    palette: theme === shown.value.theme ? shownPalette.value : null,
    name,
    description,
    configured: configuredSkin.value === theme,
  })
  const result: Array<{ label: string; options: Option[] }> = []
  if (props.unsetLabel !== undefined) {
    const fallback = props.configured ?? DEFAULT_THEME
    result.push({
      label: 'Default',
      options: [
        {
          value: UNSET,
          theme: parseThemeValue(fallback, true)?.theme ?? DEFAULT_THEME,
          palette: null,
          name: props.unsetLabel,
          description: themeLabel(parseThemeValue(fallback, true) ?? { theme: DEFAULT_THEME, palette: null }),
          configured: false,
        },
      ],
    })
  }
  for (const [label, group] of [
    ['Classic', 'classic'],
    ['Playful', 'playful'],
  ] as const) {
    result.push({
      label,
      options: THEMES.filter((theme) => theme.group === group).map((theme) =>
        option(theme.id, theme.name, theme.description),
      ),
    })
  }
  if (props.customName !== null) {
    result.push({
      label: 'Repository',
      options: [option(CUSTOM_THEME, customLabel.value, 'The skin file set for this repository.')],
    })
  }
  return result
})
const options = computed(() => groups.value.flatMap((group) => group.options))

function optionId(value: string): string {
  return `${uid}-${value}`
}
const activeId = computed(() => (activeValue.value ? optionId(activeValue.value) : undefined))

function emitValue(value: string | null) {
  if (value === UNSET) {
    model.value = null
    return
  }
  model.value = value
}

function choose(value: string) {
  if (value !== UNSET && value === shown.value.theme && !isUnset.value) {
    close(true)
    return
  }
  // A different skin starts on its default palette.
  emitValue(value === UNSET ? UNSET : formatThemeValue({ theme: value as ThemeChoice['theme'], palette: null }))
  close(true)
}

function choosePalette(palette: string) {
  emitValue(paletteValue(palette))
}

function onPaletteKey(event: KeyboardEvent, current: string) {
  const ids = palettes.value.map((option) => option.id)
  const index = ids.indexOf(current)
  const step =
    event.key === 'ArrowRight' || event.key === 'ArrowDown'
      ? 1
      : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
        ? -1
        : 0
  if (!step) return
  event.preventDefault()
  const next = ids[(index + step + ids.length) % ids.length]!
  choosePalette(next)
  void nextTick(() => root.value?.querySelector<HTMLElement>('.theme-select__palette--selected')?.focus())
}

function scrollActiveIntoView() {
  void nextTick(() => {
    const element = activeValue.value ? document.getElementById(optionId(activeValue.value)) : null
    element?.scrollIntoView?.({ block: 'nearest' })
  })
}

function openList() {
  open.value = true
  activeValue.value = selectedKey.value
  void nextTick(() => {
    list.value?.focus()
    scrollActiveIntoView()
  })
}

function close(focusTrigger: boolean) {
  open.value = false
  if (focusTrigger) void nextTick(() => trigger.value?.focus())
}

function toggle() {
  if (open.value) close(false)
  else openList()
}

function onTriggerKey(event: KeyboardEvent) {
  if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
    event.preventDefault()
    openList()
  }
}

let typed = ''
let typedAt = 0
function onListKey(event: KeyboardEvent) {
  const values = options.value.map((option) => option.value)
  const index = Math.max(0, values.indexOf(activeValue.value ?? ''))
  switch (event.key) {
    case 'ArrowDown':
      activeValue.value = values[Math.min(values.length - 1, index + 1)] ?? null
      break
    case 'ArrowUp':
      activeValue.value = values[Math.max(0, index - 1)] ?? null
      break
    case 'Home':
      activeValue.value = values[0] ?? null
      break
    case 'End':
      activeValue.value = values[values.length - 1] ?? null
      break
    case 'Enter':
    case ' ':
      if (activeValue.value) choose(activeValue.value)
      break
    case 'Escape':
      close(true)
      break
    case 'Tab':
      close(false)
      return
    default:
      if (event.key.length === 1 && /\S/.test(event.key)) {
        // Type-ahead on option names.
        const now = Date.now()
        typed = now - typedAt > 700 ? event.key.toLowerCase() : typed + event.key.toLowerCase()
        typedAt = now
        const match = options.value.find((option) => option.name.toLowerCase().startsWith(typed))
        if (match) activeValue.value = match.value
        break
      }
      return
  }
  event.preventDefault()
  scrollActiveIntoView()
}

function onDocumentPointer(event: PointerEvent) {
  if (open.value && root.value && !root.value.contains(event.target as Node)) close(false)
}
watch(open, (isOpen) => {
  if (isOpen) document.addEventListener('pointerdown', onDocumentPointer, true)
  else document.removeEventListener('pointerdown', onDocumentPointer, true)
})
onUnmounted(() => document.removeEventListener('pointerdown', onDocumentPointer, true))
</script>

<style scoped>
.theme-select {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 34rem;
}

.theme-select__anchor {
  position: relative;
}

.theme-select__trigger {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 6px 10px 6px 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-base);
  background: var(--field-background);
  box-shadow: var(--field-shadow);
  color: var(--color-fg);
  text-align: left;
  cursor: pointer;
}

.theme-select__trigger:hover {
  border-color: var(--color-border-strong);
}

.theme-select__trigger:focus-visible {
  outline: none;
  border-color: var(--color-accent);
  box-shadow: var(--focus-ring);
}

.theme-select__preview {
  flex: 0 0 120px;
  height: 64px;
}

.theme-select__text {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.theme-select__label {
  font-weight: 600;
}

.theme-select__hint {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--color-muted);
}

.theme-select__tag {
  flex-shrink: 0;
  padding: 1px 8px;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--color-accent-strong);
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
}

.theme-select__chevron {
  flex-shrink: 0;
  color: var(--color-muted);
  transition: transform var(--duration-fast) var(--ease-standard);
}

.theme-select--open .theme-select__chevron {
  transform: rotate(180deg);
}

.theme-select__list {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: var(--z-popover);
  max-height: min(60vh, 520px);
  overflow-y: auto;
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-popover);
  background: var(--color-surface-contrast);
  box-shadow: var(--shadow-popover);
}

.theme-select__list:focus {
  outline: none;
}

.theme-select__group {
  padding: 8px 8px 4px;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--color-muted);
}

.theme-select__option {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  cursor: pointer;
}

.theme-select__option--active {
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
}

.theme-select__option--selected .theme-select__label::after {
  content: ' ✓';
  color: var(--color-accent-strong);
}

.theme-select__option .theme-select__preview {
  flex-basis: 112px;
  height: 60px;
}

.theme-select__palettes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.theme-select__palette {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px 3px 4px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--color-fg);
  font-size: var(--text-xs);
  cursor: pointer;
}

.theme-select__palette:hover {
  border-color: var(--color-border-strong);
}

.theme-select__palette--selected {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 1px var(--color-accent);
}

.theme-select__palette:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.theme-select__swatch {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  border: 1px solid color-mix(in oklab, var(--color-fg) 20%, transparent);
}

/* Marks the configured palette. */
.theme-select__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-accent-strong);
}

@media (max-width: 640px) {
  .theme-select__preview,
  .theme-select__option .theme-select__preview {
    flex-basis: 96px;
    height: 56px;
  }
}
</style>
