<template>
  <div ref="root" class="theme-picker" :class="{ 'theme-picker--list': listMode }" data-testid="theme-picker">
    <label class="theme-option theme-option--default" :class="{ 'theme-option--selected': model === null }">
      <input v-model="model" type="radio" :name="name" :value="null" class="theme-option__input" />
      <ThemePreview :theme="defaultTheme" />
      <span class="theme-option__text">
        <span class="theme-option__name">Configured default</span>
        <span class="theme-option__hint muted">{{ defaultHint }}</span>
      </span>
    </label>

    <section v-for="group in groups" :key="group.id" class="theme-picker__group" :aria-label="group.label">
      <h4 class="theme-picker__heading">{{ group.label }}</h4>
      <div class="theme-picker__grid" :style="gridStyle(group.themes.length)">
        <label
          v-for="option in group.themes"
          :key="option.id"
          class="theme-option"
          :class="{ 'theme-option--selected': model === option.id }"
        >
          <input v-model="model" type="radio" :name="name" :value="option.id" class="theme-option__input" />
          <ThemePreview :theme="option.id" />
          <span class="theme-option__text">
            <span class="theme-option__name">{{ option.name }}</span>
            <span class="theme-option__hint muted">{{ option.description }}</span>
          </span>
        </label>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { balancedColumns } from '../utils/themeLayout'
import { THEMES, type ThemeId } from '../utils/themes'
import ThemePreview from './ThemePreview.vue'

const model = defineModel<ThemeId | null>({ required: true })

defineProps<{
  /** The theme "Configured default" currently resolves to. */
  defaultTheme: ThemeId
  /** Where that default comes from, e.g. "Forest · from your ~/.lotar". */
  defaultHint: string
  name?: string
}>()

const GROUP_LABELS: Record<string, string> = { classic: 'Classic', playful: 'Playful' }

const groups = computed(() => {
  const byGroup = new Map<string, Array<(typeof THEMES)[number]>>()
  for (const theme of THEMES) {
    const group = (theme as { group?: string }).group ?? 'classic'
    byGroup.set(group, [...(byGroup.get(group) ?? []), theme])
  }
  return [...byGroup.entries()].map(([id, themes]) => ({ id, label: GROUP_LABELS[id] ?? id, themes }))
})

// Columns that never strand a lone card: the last row is full or more than
// half full; when no column count works, show a one-column list instead.
const MIN_CARD = 168
const GAP = 12
const width = ref(0)
const root = ref<HTMLElement | null>(null)

const fitting = computed(() => Math.max(1, Math.floor((width.value + GAP) / (MIN_CARD + GAP))))
const listMode = computed(() => groups.value.some((group) => balancedColumns(group.themes.length, fitting.value) === 1))

function gridStyle(count: number) {
  if (listMode.value) return undefined
  return { gridTemplateColumns: `repeat(${balancedColumns(count, fitting.value)}, minmax(0, 1fr))` }
}

let observer: ResizeObserver | null = null
onMounted(() => {
  const element = root.value
  if (!element) return
  width.value = element.clientWidth
  if (typeof ResizeObserver === 'undefined') return
  observer = new ResizeObserver((entries) => {
    width.value = entries[0]?.contentRect.width ?? element.clientWidth
  })
  observer.observe(element)
})
onUnmounted(() => observer?.disconnect())
</script>

<style scoped>
.theme-picker {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.theme-picker__heading {
  margin: 0 0 8px;
  font-size: var(--text-xs);
  font-family: var(--font-sans);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  text-shadow: none;
  color: var(--color-muted);
}

.theme-picker__grid {
  display: grid;
  gap: 12px;
}

.theme-option {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-base);
  cursor: pointer;
}

.theme-option:hover {
  border-color: var(--color-border-strong);
}

.theme-option--selected {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 1px var(--color-accent);
}

.theme-option:focus-within {
  box-shadow: var(--focus-ring);
}

/* The default sits apart from the themes, as a wide row. */
.theme-option--default {
  flex-direction: row;
  align-items: center;
  gap: 12px;
}

.theme-option--default :deep(.theme-preview) {
  flex: 0 0 168px;
}

.theme-option__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

/* The radio stays focusable and announced; the card is its visible control. */
.theme-option__input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.theme-option__name {
  font-weight: 600;
}

.theme-option__hint {
  font-size: var(--text-xs);
}

/* Narrow: one theme per row, preview beside the text. */
.theme-picker--list .theme-picker__grid {
  grid-template-columns: minmax(0, 1fr);
}

.theme-picker--list .theme-option {
  flex-direction: row;
  align-items: center;
  gap: 12px;
}

.theme-picker--list .theme-option :deep(.theme-preview) {
  flex: 0 0 120px;
  height: 56px;
}
</style>
