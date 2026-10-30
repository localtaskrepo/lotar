<template>
  <div class="config-search" @keydown="onKeydown">
    <IconGlyph name="search" class="config-search__icon" aria-hidden="true" />
    <input
      ref="inputRef"
      v-model="query"
      type="search"
      class="input config-search__input"
      placeholder="Search settings"
      aria-label="Search settings"
      role="combobox"
      aria-autocomplete="list"
      :aria-expanded="open"
      :aria-controls="listId"
      :aria-activedescendant="open && results[active] ? `${listId}-${active}` : undefined"
      data-testid="config-search"
      @focus="onFocus"
      @input="onInput"
      @blur="closeSoon"
    />
    <ul v-if="open" :id="listId" class="config-search__results" role="listbox" aria-label="Matching settings">
      <li v-if="!results.length" class="config-search__empty" role="presentation">No settings match.</li>
      <li
        v-for="(result, index) in results"
        :id="`${listId}-${index}`"
        :key="result.key"
        role="option"
        :aria-selected="index === active"
        :class="['config-search__result', { 'is-active': index === active }]"
        @mousedown.prevent="pick(result)"
        @mousemove="active = index"
      >
        <span class="config-search__label">{{ result.label }}</span>
        <span class="config-search__section">{{ result.sectionLabel }}</span>
        <span v-if="result.detail" class="config-search__detail">{{ result.detail }}</span>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId } from 'vue'
import { CONFIG_SECTIONS, type ConfigSectionId } from '../utils/configSections'
import IconGlyph from './IconGlyph.vue'

/**
 * Search box for the Config page (DEV-166). It reads the settings straight
 * from the page (every section is rendered, hidden ones with v-show), so new
 * settings are searchable without keeping a separate list.
 */
const props = defineProps<{
  /** Element that holds the rendered sections. */
  root: HTMLElement | null
  /** Sections shown in the current scope. */
  sections: readonly ConfigSectionId[]
  /** Switch to a section; resolves once it is shown. */
  openSection: (id: ConfigSectionId) => Promise<void> | void
}>()

interface SearchResult {
  key: string
  label: string
  detail: string
  section: ConfigSectionId
  sectionLabel: string
  element: HTMLElement | null
  rank: number
}

const MAX_RESULTS = 12
const listId = `config-search-${useId()}`
const inputRef = ref<HTMLInputElement | null>(null)
const query = ref('')
const open = ref(false)
const active = ref(0)

function ownText(element: Element | null): string {
  if (!element) return ''
  // Leave out provenance badges ("Global", "Project") inside labels.
  const clone = element.cloneNode(true) as Element
  clone.querySelectorAll('.provenance').forEach((badge) => badge.remove())
  return (clone.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function collect(): Omit<SearchResult, 'rank'>[] {
  const entries: Omit<SearchResult, 'rank'>[] = []
  const labelOf = new Map(CONFIG_SECTIONS.map((section) => [section.id, section.label]))
  for (const section of CONFIG_SECTIONS) {
    if (!props.sections.includes(section.id)) continue
    entries.push({
      key: `section:${section.id}`,
      label: section.label,
      detail: section.summary,
      section: section.id,
      sectionLabel: 'Section',
      element: null,
    })
  }
  const root = props.root
  if (!root) return entries
  const units = root.querySelectorAll<HTMLElement>('.cfg-toggle, .field, .config-group')
  units.forEach((element, index) => {
    const sectionElement = element.closest<HTMLElement>('[data-section]')
    const section = sectionElement?.dataset.section as ConfigSectionId | undefined
    if (!section || !props.sections.includes(section)) return
    let label: string
    let detail: string
    if (element.classList.contains('cfg-toggle')) {
      label = ownText(element.querySelector('.cfg-toggle__label'))
      detail = ownText(element.querySelector('.cfg-toggle__description'))
    } else if (element.classList.contains('config-group')) {
      label = ownText(element.querySelector('.config-group__titles h3'))
      detail = ownText(element.querySelector('.config-group__description'))
    } else {
      label = ownText(element.querySelector(':scope > .field-label, :scope > label'))
      detail = ownText(element.querySelector(':scope > .field-hint'))
    }
    if (!label) return
    entries.push({
      key: `${section}:${index}`,
      label,
      detail,
      section,
      sectionLabel: labelOf.get(section) ?? section,
      element,
    })
  })
  return entries
}

const results = computed<SearchResult[]>(() => {
  const terms = query.value.toLowerCase().split(/\s+/).filter(Boolean)
  if (!terms.length || !open.value) return []
  const seen = new Set<string>()
  const matches: SearchResult[] = []
  for (const entry of collect()) {
    const label = entry.label.toLowerCase()
    const haystack = `${label} ${entry.detail.toLowerCase()} ${entry.sectionLabel.toLowerCase()}`
    if (!terms.every((term) => haystack.includes(term))) continue
    const dedupe = `${entry.section}:${label}`
    if (seen.has(dedupe)) continue
    seen.add(dedupe)
    // Label hits first, then word starts, then description-only hits.
    const rank = terms.every((term) => label.includes(term))
      ? terms.every((term) => label.startsWith(term) || label.includes(` ${term}`))
        ? 0
        : 1
      : 2
    matches.push({ ...entry, rank })
  }
  return matches.sort((a, b) => a.rank - b.rank).slice(0, MAX_RESULTS)
})

// Closing waits a moment after blur so a click on a result still lands; a
// return to the box cancels it, or a fast refocus would lose the new results.
let closeTimer: ReturnType<typeof setTimeout> | null = null

function cancelClose() {
  if (closeTimer !== null) clearTimeout(closeTimer)
  closeTimer = null
}

function onFocus() {
  cancelClose()
  open.value = Boolean(query.value.trim())
}

function onInput() {
  cancelClose()
  open.value = Boolean(query.value.trim())
  active.value = 0
}

function closeSoon() {
  cancelClose()
  closeTimer = setTimeout(() => {
    closeTimer = null
    open.value = false
  }, 120)
}

onBeforeUnmount(cancelClose)

async function pick(result: SearchResult) {
  open.value = false
  await props.openSection(result.section)
  await nextTick()
  const target = result.element
  if (!target || !target.isConnected) return
  target.scrollIntoView?.({ block: 'center', behavior: 'smooth' })
  target.classList.add('config-search-hit')
  setTimeout(() => target.classList.remove('config-search-hit'), 2000)
  const control = target.querySelector<HTMLElement>('input, select, textarea, button')
  control?.focus({ preventScroll: true })
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    if (open.value || query.value) {
      event.preventDefault()
      event.stopPropagation()
      if (open.value) open.value = false
      else query.value = ''
    }
    return
  }
  if (!open.value || !results.value.length) return
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    active.value = (active.value + 1) % results.value.length
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    active.value = (active.value - 1 + results.value.length) % results.value.length
  } else if (event.key === 'Enter') {
    event.preventDefault()
    const result = results.value[active.value]
    if (result) void pick(result)
  }
}

defineExpose({ focus: () => inputRef.value?.focus() })
</script>

<style scoped>
.config-search {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
  width: min(280px, 100%);
}

.config-search__icon {
  position: absolute;
  left: 10px;
  color: var(--color-muted);
  pointer-events: none;
}

.config-search__input {
  width: 100%;
  padding-left: 32px;
}

.config-search__results {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: var(--z-popover, 50);
  width: max(100%, 340px);
  max-width: calc(100vw - 32px);
  max-height: 360px;
  overflow-y: auto;
  margin: 0;
  padding: 4px;
  list-style: none;
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-popover);
}

.config-search__result {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 2px 12px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.config-search__result.is-active {
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
}

.config-search__label {
  font-weight: 600;
  font-size: var(--text-sm, 0.875rem);
}

.config-search__section {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  white-space: nowrap;
}

.config-search__detail {
  grid-column: 1 / -1;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.config-search__empty {
  padding: 6px 8px;
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
}

:global(.config-search-hit) {
  outline: 2px solid var(--color-accent);
  outline-offset: 4px;
  border-radius: var(--radius-md);
  transition: outline-color 0.4s ease;
}
</style>
