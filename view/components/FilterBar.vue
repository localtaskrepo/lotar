<template>
  <div class="filter-bar" :class="{ 'filter-bar--panel-open': panelOpen }">
    <div class="row filter-bar__bar">
      <div class="filter-bar__search">
        <input
          ref="searchInput"
          :value="searchDraft"
          class="input filter-bar__search-input"
          type="text"
          placeholder="Search or type filters, e.g. status:todo"
          aria-label="Search tasks with filter syntax"
          data-testid="filter-search"
          autocomplete="off"
          spellcheck="false"
          @focus="suggestionsOpen = true"
          @blur="onSearchBlur"
          @input="onSearchInput"
          @keydown="onSearchKeydown"
        />
        <div
          v-if="suggestionsOpen && suggestions.length"
          class="filter-bar__suggestions"
          role="listbox"
          aria-label="Filter suggestions"
        >
          <button
            v-for="(item, index) in suggestions"
            :key="item.insert"
            type="button"
            class="filter-bar__suggestion"
            :class="{ 'is-active': index === activeSuggestion }"
            role="option"
            :aria-selected="index === activeSuggestion ? 'true' : 'false'"
            @mousedown.prevent="pickSuggestion(item)"
            @mousemove="activeSuggestion = index"
          >
            <span class="filter-bar__suggestion-label">{{ item.label }}</span>
            <span v-if="item.hint" class="filter-bar__suggestion-hint">{{ item.hint }}</span>
          </button>
        </div>
      </div>

      <UiButton
        :variant="panelOpen ? 'primary' : ''"
        type="button"
        class="filter-bar__toggle"
        aria-label="Toggle filters"
        :aria-expanded="panelOpen ? 'true' : 'false'"
        data-testid="filter-toggle"
        @click="panelOpen = !panelOpen"
      >
        <span>Filters</span>
        <span v-if="activeCount" class="filter-bar__count">{{ activeCount }}</span>
      </UiButton>

      <div class="filter-bar__actions">
        <slot name="actions" />
      </div>
    </div>

    <div v-if="activeChips.length" class="filter-bar__chips-row" data-testid="filter-chips">
      <span v-for="chip in activeChips" :key="`${chip.key}-${chip.value}`" class="filter-bar__chip">
        <span class="filter-bar__chip-label">{{ chip.label }}</span>
        <span v-if="chip.display" class="filter-bar__chip-value">{{ chip.display }}</span>
        <button
          type="button"
          class="filter-bar__chip-remove"
          :aria-label="`Remove filter ${chip.label} ${chip.display || chip.value}`"
          @click="removeChip(chip)"
        >×</button>
      </span>
    </div>

    <div v-if="panelOpen" class="filter-bar__panel" data-testid="filter-panel">
      <SmartListChips
        :statuses="statuses"
        :priorities="priorities"
        :value="smartChipsValue"
        :custom-presets="customPresets"
        :enable-due-soon="enableDueSoon"
        :enable-recent="enableRecent"
        @update:value="onSmartChipsUpdate"
        @preset="appendCustomFilter"
      />

      <slot name="panel" />

      <div class="row filter-bar__panel-controls">
        <div
          v-if="hasSingleProject"
          class="input filter-bar__project-static"
          aria-label="Project filter"
          data-testid="filter-project"
          :title="singleProjectLabel"
        >
          {{ singleProjectLabel }}
        </div>
        <UiSelect v-else v-model="project" aria-label="Project filter" data-testid="filter-project">
          <option value="">Project</option>
          <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
        </UiSelect>

        <div v-if="showStatusSelect" ref="statusDropdown" class="filter-bar__dropdown">
          <button
            type="button"
            class="input filter-bar__dropdown-trigger"
            aria-label="Status filter"
            data-testid="filter-status"
            :title="statusTitle"
            :aria-expanded="statusMenuOpen ? 'true' : 'false'"
            @click="toggleStatusMenu"
          >
            <span class="filter-bar__dropdown-trigger-label">{{ statusTriggerLabel }}</span>
          </button>
          <div v-if="statusMenuOpen" class="filter-bar__menu-popover" role="menu" @click.stop>
            <div v-if="statusHasSelections" class="filter-bar__menu-actions">
              <button type="button" class="filter-bar__menu-action" @click="clearStatus">Clear</button>
              <button type="button" class="filter-bar__menu-action" @click="invertStatus">Invert</button>
            </div>
            <label v-for="s in statuses" :key="s" class="filter-bar__menu-item">
              <input type="checkbox" :checked="statusSelectionSet.has(s)" @change="toggleStatusValue(s)" />
              <span>{{ s }}</span>
            </label>
          </div>
        </div>

        <div ref="priorityDropdown" class="filter-bar__dropdown">
          <button
            type="button"
            class="input filter-bar__dropdown-trigger"
            aria-label="Priority filter"
            data-testid="filter-priority"
            :title="priorityTitle"
            :aria-expanded="priorityMenuOpen ? 'true' : 'false'"
            @click="togglePriorityMenu"
          >
            <span class="filter-bar__dropdown-trigger-label">{{ priorityTriggerLabel }}</span>
          </button>
          <div v-if="priorityMenuOpen" class="filter-bar__menu-popover" role="menu" @click.stop>
            <div v-if="priorityHasSelections" class="filter-bar__menu-actions">
              <button type="button" class="filter-bar__menu-action" @click="clearPriority">Clear</button>
              <button type="button" class="filter-bar__menu-action" @click="invertPriority">Invert</button>
            </div>
            <label v-for="p in priorities" :key="p" class="filter-bar__menu-item">
              <input type="checkbox" :checked="prioritySelectionSet.has(p)" @change="togglePriorityValue(p)" />
              <span>{{ p }}</span>
            </label>
          </div>
        </div>

        <div ref="typeDropdown" class="filter-bar__dropdown">
          <button
            type="button"
            class="input filter-bar__dropdown-trigger"
            aria-label="Type filter"
            data-testid="filter-type"
            :title="typeTitle"
            :aria-expanded="typeMenuOpen ? 'true' : 'false'"
            @click="toggleTypeMenu"
          >
            <span class="filter-bar__dropdown-trigger-label">{{ typeTriggerLabel }}</span>
          </button>
          <div v-if="typeMenuOpen" class="filter-bar__menu-popover" role="menu" @click.stop>
            <div v-if="typeHasSelections" class="filter-bar__menu-actions">
              <button type="button" class="filter-bar__menu-action" @click="clearType">Clear</button>
              <button type="button" class="filter-bar__menu-action" @click="invertType">Invert</button>
            </div>
            <label v-for="t in types" :key="t" class="filter-bar__menu-item">
              <input type="checkbox" :checked="typeSelectionSet.has(t)" @change="toggleTypeValue(t)" />
              <span>{{ t }}</span>
            </label>
          </div>
        </div>

        <div v-if="showSprintSelect" ref="sprintDropdown" class="filter-bar__dropdown">
          <button
            type="button"
            class="input filter-bar__dropdown-trigger"
            aria-label="Sprint filter"
            data-testid="filter-sprint"
            :title="sprintTitle"
            :aria-expanded="sprintMenuOpen ? 'true' : 'false'"
            @click="toggleSprintMenu"
          >
            <span class="filter-bar__dropdown-trigger-label">{{ sprintTriggerLabel }}</span>
          </button>
          <div v-if="sprintMenuOpen" class="filter-bar__menu-popover" role="menu" @click.stop>
            <div v-if="sprintHasSelections" class="filter-bar__menu-actions">
              <button type="button" class="filter-bar__menu-action" @click="clearSprint">Clear</button>
              <button type="button" class="filter-bar__menu-action" @click="invertSprint">Invert</button>
            </div>
            <label v-for="opt in sprintOptions" :key="opt.id" class="filter-bar__menu-item">
              <input type="checkbox" :checked="sprintSelectionSet.has(String(opt.id))" @change="toggleSprintValue(String(opt.id))" />
              <span>{{ opt.label }}</span>
            </label>
          </div>
        </div>

        <UiSelect v-if="showOrderSelect" v-model="order">
          <option value="desc">Newest</option>
          <option value="asc">Oldest</option>
        </UiSelect>

        <UiInput v-model="tags" placeholder="Tags" />

        <div class="filter-bar__custom">
          <UiInput
            ref="customFilterInput"
            v-model="extraFilters"
            placeholder="Custom filters (key=value, e.g. field:iteration=beta)"
            :class="{ 'input--invalid': hasCustomFilterError }"
            aria-label="Custom filters"
          />
          <div class="filter-bar__custom-hint-wrapper">
            <button
              type="button"
              class="filter-bar__custom-hint-btn"
              :class="{ 'filter-bar__custom-hint-btn--error': hasCustomFilterError }"
              :title="customFilterHint"
              :aria-describedby="customHintPopoverId"
              :aria-expanded="customHintVisible ? 'true' : 'false'"
              aria-label="Custom filter help"
              data-testid="custom-filter-hint"
              @mouseenter="showCustomHint"
              @mouseleave="hideCustomHint"
              @focus="showCustomHint"
              @blur="hideCustomHint"
            >
              ?
            </button>
            <div
              v-if="shouldRenderCustomHint"
              :id="customHintPopoverId"
              class="filter-bar__custom-hint-popover"
              role="tooltip"
              data-testid="custom-filter-hint-popover"
            >
              {{ customFilterHint }}
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch, watchEffect } from 'vue'
import { listFromCsv } from '../composables/useFilterBuilder'
import { storageGetJson, storageRemove, storageSetJson } from '../utils/storage'
import { useProjects } from '../composables/useProjects'
import {
    chipsForFilterValue,
    GRAMMAR_KEYS,
    parseFilterQuery,
    suggestForFragment,
    type FilterChip,
    type SuggestionItem,
} from '../composables/useFilterGrammar'
import { formatProjectLabel } from '../utils/projectLabels'
import SmartListChips from './SmartListChips.vue'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

interface CustomPreset {
  label: string
  expression: string
}

const props = withDefaults(
  defineProps<{
    statuses?: string[]
    priorities?: string[]
    types?: string[]
    sprintOptions?: Array<{ id: number; label: string }>
    value?: Record<string, string>
    storageKey?: string
    showStatus?: boolean
    emitProjectKey?: boolean
    showOrder?: boolean
    customPresets?: CustomPreset[]
    enableDueSoon?: boolean
    enableRecent?: boolean
  }>(),
  {
    showStatus: true,
  },
)
const emit = defineEmits<{ (e:'update:value', v: Record<string,string>): void }>()

const query = ref('')
const searchInput = ref<HTMLInputElement | null>(null)
const searchDraft = ref('')
const suggestionsOpen = ref(false)
const activeSuggestion = ref(0)
const project = ref('')
const status = ref('')
const priority = ref('')
const type = ref('')
const sprintFilter = ref('')
const order = ref<'asc'|'desc'>('desc')
const tags = ref('')
const assignee = ref('')
const dueDate = ref('')
const recent = ref('')
const needs = ref('')
const extraFilters = ref('')
const customFilterErrors = ref<string[]>([])
const customFilterInput = ref<{ focus: () => void } | null>(null)
let lastSyncedExtras = ''
// Keys that have dedicated UI controls and must not fall through to the
// custom-filter box. Derived from the grammar so aliases stay in one place.
const CUSTOM_UI_KEYS = new Set(['q', 'order', ...GRAMMAR_KEYS.map((meta) => meta.key)])
// Normalized alias -> canonical field, shared with the grammar's alias table.
const RESERVED_FIELD_ALIASES: Record<string, string> = (() => {
  const map: Record<string, string> = {
    q: 'q',
    query: 'q',
    text: 'q',
    textquery: 'q',
    search: 'q',
    order: 'order',
    sort: 'order',
  }
  for (const meta of GRAMMAR_KEYS) {
    for (const alias of meta.aliases) map[alias] = meta.key
    map[meta.key] = meta.key
  }
  return map
})()
const customHintPopoverId = `custom-filter-hint-${Math.random().toString(36).slice(2, 8)}`
const customHintVisible = ref(false)
const showStatusSelect = computed(() => props.showStatus)
const showOrderSelect = computed(() => props.showOrder !== false)

const panelOpen = ref(false)

const { projects, refresh } = useProjects()
const singleProject = computed(() => (projects.value.length === 1 ? projects.value[0] : null))
const hasSingleProject = computed(() => !!singleProject.value)
const singleProjectLabel = computed(() => {
  const p = singleProject.value
  return p ? formatProjectLabel(p) : ''
})

const DOCUMENT_CLICK_OPTS: AddEventListenerOptions = { capture: true }


function joinCsv(values: string[]): string {
  return values.join(',')
}

function toggleInCsv(csv: string, value: string): string {
  const trimmed = value.trim()
  if (!trimmed) return csv
  const next = new Set(listFromCsv(csv))
  if (next.has(trimmed)) {
    next.delete(trimmed)
  } else {
    next.add(trimmed)
  }
  return joinCsv(Array.from(next))
}

function mergeIntoCsv(csv: string, value: string): string {
  const trimmed = value.trim()
  if (!trimmed) return csv
  const next = new Set(listFromCsv(csv))
  next.add(trimmed)
  return joinCsv(Array.from(next))
}

function invertCsv(csv: string, universe: readonly string[]): string {
  if (!universe.length) return ''
  const selected = new Set(listFromCsv(csv))
  const next = universe
    .map((v) => v.trim())
    .filter(Boolean)
    .filter((v) => !selected.has(v))
  return joinCsv(next)
}

const statusTitle = computed(() => {
  const values = listFromCsv(status.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const statusHasSelections = computed(() => listFromCsv(status.value).length > 0)
const statusSelections = computed(() => listFromCsv(status.value))
const statusSelectionSet = computed(() => new Set(statusSelections.value))
const statusTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Status', statusSelections.value))
const priorityTitle = computed(() => {
  const values = listFromCsv(priority.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const priorityHasSelections = computed(() => listFromCsv(priority.value).length > 0)
const prioritySelections = computed(() => listFromCsv(priority.value))
const prioritySelectionSet = computed(() => new Set(prioritySelections.value))
const priorityTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Priority', prioritySelections.value))
const typeTitle = computed(() => {
  const values = listFromCsv(type.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const typeHasSelections = computed(() => listFromCsv(type.value).length > 0)
const typeSelections = computed(() => listFromCsv(type.value))
const typeSelectionSet = computed(() => new Set(typeSelections.value))
const typeTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Type', typeSelections.value))

const showSprintSelect = computed(() => (props.sprintOptions ?? []).length > 0)
const sprintTitle = computed(() => {
  const values = listFromCsv(sprintFilter.value)
  if (!values.length) return ''
  const opts = props.sprintOptions ?? []
  const labels = values.map((id) => opts.find((o) => String(o.id) === id)?.label ?? `#${id}`)
  return `Selected: ${labels.join(', ')}`
})
const sprintHasSelections = computed(() => listFromCsv(sprintFilter.value).length > 0)
const sprintSelections = computed(() => listFromCsv(sprintFilter.value))
const sprintSelectionSet = computed(() => new Set(sprintSelections.value))
const sprintTriggerLabel = computed(() => {
  const selected = sprintSelections.value
  if (!selected.length) return 'Sprint'
  const opts = props.sprintOptions ?? []
  const labels = selected.map((id) => opts.find((o) => String(o.id) === id)?.label ?? `#${id}`)
  return formatMultiSelectTriggerLabel('Sprint', labels)
})

function formatMultiSelectTriggerLabel(label: string, selected: string[]): string {
  if (!selected.length) return label
  if (selected.length <= 2) return `${label}: ${selected.join(', ')}`
  return `${label}: ${selected.slice(0, 2).join(', ')} (+${selected.length - 2})`
}

const statusMenuOpen = ref(false)
const priorityMenuOpen = ref(false)
const typeMenuOpen = ref(false)
const sprintMenuOpen = ref(false)

const statusDropdown = ref<HTMLElement | null>(null)
const priorityDropdown = ref<HTMLElement | null>(null)
const typeDropdown = ref<HTMLElement | null>(null)
const sprintDropdown = ref<HTMLElement | null>(null)

function closeAllMenus() {
  statusMenuOpen.value = false
  priorityMenuOpen.value = false
  typeMenuOpen.value = false
  sprintMenuOpen.value = false
}

function toggleStatusMenu() {
  const next = !statusMenuOpen.value
  closeAllMenus()
  statusMenuOpen.value = next
}

function togglePriorityMenu() {
  const next = !priorityMenuOpen.value
  closeAllMenus()
  priorityMenuOpen.value = next
}

function toggleTypeMenu() {
  const next = !typeMenuOpen.value
  closeAllMenus()
  typeMenuOpen.value = next
}

function toggleSprintMenu() {
  const next = !sprintMenuOpen.value
  closeAllMenus()
  sprintMenuOpen.value = next
}

function toggleStatusValue(value: string) {
  status.value = toggleInCsv(status.value, value)
}

function togglePriorityValue(value: string) {
  priority.value = toggleInCsv(priority.value, value)
}

function toggleTypeValue(value: string) {
  type.value = toggleInCsv(type.value, value)
}

function toggleSprintValue(id: string) {
  sprintFilter.value = toggleInCsv(sprintFilter.value, id)
}

function clearStatus() {
  status.value = ''
}

function invertStatus() {
  status.value = invertCsv(status.value, props.statuses ?? [])
}

function clearPriority() {
  priority.value = ''
}

function invertPriority() {
  priority.value = invertCsv(priority.value, props.priorities ?? [])
}

function clearType() {
  type.value = ''
}

function invertType() {
  type.value = invertCsv(type.value, props.types ?? [])
}

function clearSprint() {
  sprintFilter.value = ''
}

function invertSprint() {
  const ids = (props.sprintOptions ?? []).map((o) => String(o.id))
  sprintFilter.value = invertCsv(sprintFilter.value, ids)
}

function onDocumentClick(event: MouseEvent) {
  const target = event.target as Node | null
  if (!target) return

  if (statusMenuOpen.value && statusDropdown.value && !statusDropdown.value.contains(target)) {
    statusMenuOpen.value = false
  }
  if (priorityMenuOpen.value && priorityDropdown.value && !priorityDropdown.value.contains(target)) {
    priorityMenuOpen.value = false
  }
  if (sprintMenuOpen.value && sprintDropdown.value && !sprintDropdown.value.contains(target)) {
    sprintMenuOpen.value = false
  }
  if (typeMenuOpen.value && typeDropdown.value && !typeDropdown.value.contains(target)) {
    typeMenuOpen.value = false
  }
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    closeAllMenus()
  }
  // `/` focuses the filter search from anywhere outside a text field.
  const target = event.target as HTMLElement | null
  const editing = !!target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
  if (event.key === '/' && !editing) {
    event.preventDefault()
    searchInput.value?.focus()
  }
}

function hasMeaningfulIncoming(value?: Record<string, string>): boolean {
  if (!value) return false
  for (const [key, raw] of Object.entries(value)) {
    if (key === 'order') {
      if (raw === 'asc') return true
      continue
    }
    if ((raw || '').trim().length) return true
  }
  return false
}

// Persist last used filter to localStorage for convenience
const FILTER_KEY = computed(() => props.storageKey || 'lotar.tasks.filter')
onMounted(() => {
  document.addEventListener('click', onDocumentClick, DOCUMENT_CLICK_OPTS)
  document.addEventListener('keydown', onDocumentKeydown)
  try {
    const hasIncoming = hasMeaningfulIncoming(props.value)
    if (!hasIncoming) {
      const saved = storageGetJson<Record<string, unknown>>(FILTER_KEY.value)
      if (saved && typeof saved === 'object') {
        const asText = (v: unknown) => (typeof v === 'string' ? v : '')
        query.value = asText(saved.q)
        searchDraft.value = query.value
        project.value = asText(saved.project)
        status.value = asText(saved.status)
        priority.value = asText(saved.priority)
        type.value = asText(saved.type)
        sprintFilter.value = asText(saved.sprints)
        assignee.value = asText(saved.assignee)
        tags.value = asText(saved.tags)
        dueDate.value = asText(saved.due)
        recent.value = asText(saved.recent)
        needs.value = asText(saved.needs)
        order.value = (saved.order === 'asc' || saved.order === 'desc') ? saved.order : 'desc'
        const extras = Object.entries(saved)
          .filter(([key]) => !CUSTOM_UI_KEYS.has(key))
          .map(([key, value]) => `${key}=${value}`)
          .join(', ')
        if (extras) {
          extraFilters.value = extras
          lastSyncedExtras = extras
        }
      }
    }
  } catch {}
})

onUnmounted(() => {
  document.removeEventListener('click', onDocumentClick, DOCUMENT_CLICK_OPTS)
  document.removeEventListener('keydown', onDocumentKeydown)
})

onMounted(() => {
  refresh()
})

watchEffect(() => {
  const p = singleProject.value
  if (!p) return
  const prefix = (p.prefix ?? '').trim()
  if (!prefix) return
  if (project.value !== prefix) {
    project.value = prefix
  }
})

// localStorage is shared per browser origin, so a saved filter written by a
// different LoTaR instance (same host/port, different workspace) can reference
// a project this server does not know. Drop it once the real list arrives.
// Matching stays lenient (prefix or name, case-insensitive, either-direction
// prefix overlap) because the server itself resolves full project names to
// generated prefixes, e.g. name "ALPHA" -> prefix "ALPH".
watch(projects, (list) => {
  if (!list.length) return
  if (project.value && !isKnownProject(list, project.value)) {
    project.value = ''
  }
})

function isKnownProject(list: Array<{ prefix?: string; name?: string }>, value: string): boolean {
  const v = value.trim().toLowerCase()
  if (!v) return false
  return list.some((p) => {
    const prefix = (p.prefix ?? '').trim().toLowerCase()
    if (!prefix) return false
    if (prefix === v) return true
    const name = (p.name ?? '').trim().toLowerCase()
    if (name && name === v) return true
    return v.startsWith(prefix) || prefix.startsWith(v)
  })
}

let lastPropsSnapshot = ''
watchEffect(() => {
  if (props.value) {
    // Only hydrate from props when the incoming value actually changed; otherwise
    // unrelated effect triggers (template refs, option lists) would clobber state
    // the user just typed.
    const snapshot = JSON.stringify(props.value)
    if (snapshot === lastPropsSnapshot) return
    lastPropsSnapshot = snapshot
    query.value = props.value.q || ''
    if (document.activeElement !== searchInput.value) {
      searchDraft.value = query.value
    }
    project.value = props.value.project || ''
    status.value = props.value.status || ''
    priority.value = props.value.priority || ''
    type.value = props.value.type || ''
    sprintFilter.value = props.value.sprints || ''
    const incomingAssignee = props.value.assignee || ''
    const isMine = props.value.mine === 'true' || incomingAssignee === '@me'
    assignee.value = isMine ? '@me' : incomingAssignee
    tags.value = props.value.tags || ''
    dueDate.value = props.value.due || ''
    recent.value = props.value.recent || ''
    needs.value = props.value.needs || ''
    const o = props.value.order
    order.value = (o === 'asc' || o === 'desc') ? o : order.value
    const extras = Object.entries(props.value)
      .filter(([key]) => !CUSTOM_UI_KEYS.has(key))
      .map(([key, value]) => `${key}=${value}`)
      .join(', ')
    if (extras !== lastSyncedExtras) {
      lastSyncedExtras = extras
      extraFilters.value = extras
    }
  } else if (lastSyncedExtras) {
    lastSyncedExtras = ''
    extraFilters.value = ''
  }
})


function normalizeReservedKey(input: string): string {
  return input.toLowerCase().replace(/[-_\s]+/g, '')
}

function canonicalizeReservedFieldName(name: string): string {
  if (!name) return ''
  const normalized = normalizeReservedKey(name)
  return RESERVED_FIELD_ALIASES[normalized] || ''
}

function canonicalizeCustomKey(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return ''
  const lower = trimmed.toLowerCase()
  if (lower.startsWith('field:')) {
    const value = trimmed.slice(trimmed.indexOf(':') + 1).trim()
    const builtin = canonicalizeReservedFieldName(value)
    if (value && builtin) {
      return builtin
    }
    return value ? `field:${value.toLowerCase()}` : ''
  }
  return lower
}

function parseCustomFilters(input: string): { map: Record<string, string>; errors: string[] } {
  const map: Record<string, string> = {}
  const errors: string[] = []

  listFromCsv(input).forEach((part) => {
    const eq = part.indexOf('=')
    if (eq <= 0) {
      errors.push(`"${part}" is missing "="`)
      return
    }
    const key = part.slice(0, eq).trim()
    const value = part.slice(eq + 1).trim()
    if (!key) {
      errors.push('Missing key before "="')
      return
    }
    if (!value) {
      errors.push(`Add a value for "${key}"`)
      return
    }
    const canonical = canonicalizeCustomKey(key)
    if (!canonical) {
      errors.push(`Invalid filter name "${key}"`)
      return
    }
    map[canonical] = value
  })

  return { map, errors }
}

function formatHelperMessage(errors: string[]): string {
  if (!errors.length) {
    return extraFilters.value.trim()
      ? 'Type another key=value pair or pick a preset chip to insert one.'
      : 'Format: key=value. Separate multiple filters with commas.'
  }
  if (errors.length === 1) return errors[0] ?? ''
  const [first, second] = errors
  const suffix = errors.length > 2 ? ` (+${errors.length - 2} more)` : ''
  return `${first ?? ''}; ${second || ''}${suffix}`.trim()
}

const customFilterHint = computed(() => formatHelperMessage(customFilterErrors.value))
const hasCustomFilterError = computed(() => customFilterErrors.value.length > 0)
const shouldRenderCustomHint = computed(() => customHintVisible.value && customFilterHint.value.trim().length > 0)

function appendCustomFilter(expr: string) {
  const trimmed = expr.trim()
  if (!trimmed) return
  const tokens = listFromCsv(extraFilters.value)
  if (!tokens.includes(trimmed)) {
    tokens.push(trimmed)
    extraFilters.value = tokens.join(', ')
    nextTick(() => customFilterInput.value?.focus())
  } else {
    nextTick(() => customFilterInput.value?.focus())
  }
}

function showCustomHint() {
  customHintVisible.value = true
}

function hideCustomHint() {
  customHintVisible.value = false
}

// --- Grammar search with suggestions ---

const suggestionSource = computed(() => ({
  statuses: props.statuses ?? [],
  priorities: props.priorities ?? [],
  types: props.types ?? [],
  sprints: props.sprintOptions ?? [],
  projects: projects.value ?? [],
}))

const suggestions = computed<SuggestionItem[]>(() => {
  const fragment = currentFragment()
  if (!fragment && !searchDraft.value.trim()) return []
  return suggestForFragment(fragment, suggestionSource.value)
})

function currentFragment(): string {
  const text = searchDraft.value
  const start = Math.max(text.lastIndexOf(' '), text.lastIndexOf('\t')) + 1
  return text.slice(start)
}

function replaceFragment(replacement: string): void {
  const text = searchDraft.value
  const start = Math.max(text.lastIndexOf(' '), text.lastIndexOf('\t')) + 1
  // Key prefixes (ending in ':') are inserted without a trailing space so the
  // current fragment stays "key:" and value suggestions appear immediately.
  const suffix = replacement.endsWith(':') ? '' : ' '
  searchDraft.value = `${text.slice(0, start)}${replacement}${suffix}`
}

function onSearchInput(event: Event) {
  suggestionsOpen.value = true
  activeSuggestion.value = 0
  const value = (event.target as HTMLInputElement | null)?.value ?? ''
  searchDraft.value = value
  // Live free-text search mirrors the raw draft (pre-Enter parsing keeps old behavior).
  query.value = value
}

function onSearchBlur() {
  // Delay so mousedown-based picks register before the blur clears state.
  window.setTimeout(() => {
    suggestionsOpen.value = false
  }, 120)
}

function pickSuggestion(item: SuggestionItem) {
  const valuePart = item.insert.slice(item.insert.indexOf(':') + 1)
  const keyPart = item.insert.slice(0, item.insert.indexOf(':'))
  if (valuePart) {
    applyGrammarFilters({ [keyPart]: valuePart })
    replaceFragment('')
    searchDraft.value = searchDraft.value.replace(/\s+$/, '')
    query.value = searchDraft.value
    suggestionsOpen.value = false
    activeSuggestion.value = 0
  } else {
    // Key prefix picked: keep the list open so the values for this key are
    // offered right away (the prefix is inserted without a trailing space).
    replaceFragment(item.insert)
    activeSuggestion.value = 0
    suggestionsOpen.value = true
  }
  searchInput.value?.focus()
}

function onSearchKeydown(event: KeyboardEvent) {
  if (suggestionsOpen.value && suggestions.value.length) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      activeSuggestion.value = (activeSuggestion.value + 1) % suggestions.value.length
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      activeSuggestion.value = (activeSuggestion.value - 1 + suggestions.value.length) % suggestions.value.length
      return
    }
    if (event.key === 'Tab') {
      event.preventDefault()
      const highlighted = suggestions.value[activeSuggestion.value]
      if (highlighted) pickSuggestion(highlighted)
      return
    }
  }
  if (event.key === 'Enter') {
    event.preventDefault()
    const highlighted = suggestionsOpen.value ? suggestions.value[activeSuggestion.value] : undefined
    if (highlighted) {
      pickSuggestion(highlighted)
      return
    }
    applySearchDraft()
    suggestionsOpen.value = false
  } else if (event.key === 'Escape') {
    suggestionsOpen.value = false
  }
}

function applyGrammarFilters(filters: Record<string, string>) {
  for (const [key, value] of Object.entries(filters)) {
    switch (key) {
      case 'status':
        status.value = mergeIntoCsv(status.value, value)
        break
      case 'priority':
        priority.value = mergeIntoCsv(priority.value, value)
        break
      case 'type':
        type.value = mergeIntoCsv(type.value, value)
        break
      case 'sprints':
        sprintFilter.value = mergeIntoCsv(sprintFilter.value, value)
        break
      case 'assignee':
        assignee.value = value
        break
      case 'tags':
        tags.value = mergeIntoCsv(tags.value, value)
        break
      case 'due':
        dueDate.value = value
        break
      case 'recent':
        recent.value = value
        break
      case 'needs':
        needs.value = mergeIntoCsv(needs.value, value)
        break
      case 'mine':
        if (value === 'true') assignee.value = '@me'
        break
    }
  }
}

function applySearchDraft() {
  const parsed = parseFilterQuery(searchDraft.value)
  applyGrammarFilters(parsed.filters)
  query.value = parsed.text
  searchDraft.value = parsed.text
}

// --- Active filter chips ---

const structuredFilterValue = computed<Record<string, string>>(() => {
  const v: Record<string, string> = {}
  if (!hasSingleProject.value && project.value) v.project = project.value
  if (status.value) v.status = status.value
  if (priority.value) v.priority = priority.value
  if (type.value) v.type = type.value
  if (sprintFilter.value) v.sprints = sprintFilter.value
  if (assignee.value) v.assignee = assignee.value
  if (dueDate.value) v.due = dueDate.value
  if (recent.value) v.recent = recent.value
  if (needs.value) v.needs = needs.value
  return v
})

const activeChips = computed<FilterChip[]>(() =>
  chipsForFilterValue(structuredFilterValue.value, suggestionSource.value),
)

const activeCount = computed(() => activeChips.value.length)

function removeChip(chip: FilterChip) {
  switch (chip.key) {
    case 'project':
      project.value = ''
      break
    case 'status':
      status.value = removeCsvValue(status.value, chip.value)
      break
    case 'priority':
      priority.value = removeCsvValue(priority.value, chip.value)
      break
    case 'type':
      type.value = removeCsvValue(type.value, chip.value)
      break
    case 'sprints':
      sprintFilter.value = removeCsvValue(sprintFilter.value, chip.value)
      break
    case 'tags':
      tags.value = removeCsvValue(tags.value, chip.value)
      break
    case 'needs':
      needs.value = removeCsvValue(needs.value, chip.value)
      break
    case 'assignee':
      assignee.value = ''
      break
    case 'due':
      dueDate.value = ''
      break
    case 'recent':
      recent.value = ''
      break
  }
}

function removeCsvValue(csv: string, value: string): string {
  const next = listFromCsv(csv).filter((v) => v !== value)
  return joinCsv(next)
}

// --- Smart chips integration ---

const smartChipsValue = computed<Record<string, string>>(() => ({
  ...(status.value ? { status: status.value } : {}),
  ...(priority.value ? { priority: priority.value } : {}),
  ...(type.value ? { type: type.value } : {}),
  ...(assignee.value ? { assignee: assignee.value } : {}),
  ...(tags.value ? { tags: tags.value } : {}),
  ...(dueDate.value ? { due: dueDate.value } : {}),
  ...(recent.value ? { recent: recent.value } : {}),
  ...(needs.value ? { needs: needs.value } : {}),
}))

function onSmartChipsUpdate(next: Record<string, string>) {
  status.value = next.status || ''
  priority.value = next.priority || ''
  type.value = next.type || ''
  assignee.value = next.assignee || ''
  tags.value = next.tags || ''
  dueDate.value = next.due || ''
  recent.value = next.recent || ''
  needs.value = next.needs || ''
}

function emitFilter(){
  const v: Record<string,string> = {}
  if (query.value) v.q = query.value
  const shouldEmitProject = props.emitProjectKey || !!project.value
  if (shouldEmitProject) v.project = project.value || ''
  if (status.value) v.status = status.value
  if (priority.value) v.priority = priority.value
  if (type.value) v.type = type.value
  if (sprintFilter.value) v.sprints = sprintFilter.value
  if (assignee.value) v.assignee = assignee.value
  if (tags.value) v.tags = tags.value
  if (dueDate.value) v.due = dueDate.value
  if (recent.value) v.recent = recent.value
  if (needs.value) v.needs = needs.value
  if (showOrderSelect.value) {
    v.order = order.value
  }
  const parsed = parseCustomFilters(extraFilters.value)
  customFilterErrors.value = parsed.errors
  Object.entries(parsed.map).forEach(([key, value]) => {
    if (value) v[key] = value
  })
  storageSetJson(FILTER_KEY.value, v)
  emit('update:value', v)
}
function onClear(){
  // Reset all local state and emit an empty filter
  query.value = ''
  searchDraft.value = ''
  project.value = ''
  status.value = ''
  priority.value = ''
  type.value = ''
  sprintFilter.value = ''
  tags.value = ''
  assignee.value = ''
  dueDate.value = ''
  recent.value = ''
  needs.value = ''
  if (showOrderSelect.value) {
    order.value = 'desc'
  }
  extraFilters.value = ''
  lastSyncedExtras = ''
  customFilterErrors.value = []
  storageRemove(FILTER_KEY.value)
  const empty: Record<string,string> = {}
  if (showOrderSelect.value) {
    empty.order = 'desc'
  }
  emit('update:value', empty)
}

// Emit whenever any field changes; parent debounces/refetches
watch([query, project, status, priority, type, sprintFilter, order, tags, extraFilters, assignee, dueDate, recent, needs], emitFilter, { deep: false })

defineExpose({ appendCustomFilter, clear: onClear })
</script>
<style scoped>
.filter-bar {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.filter-bar__bar {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.filter-bar__search {
  position: relative;
  flex: 1 1 180px;
  min-width: 140px;
  /* Cap the stretch so ultrawide layouts keep the bar compact; page-level
     actions right-align against the container's max-width instead. */
  max-width: 440px;
}

.filter-bar__actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 8px;
  margin-left: auto;
}

.filter-bar__search-input {
  width: 100%;
  /* Form controls refuse to shrink below their intrinsic size without this,
     which made the search box overrun the Filters button on narrow screens. */
  min-width: 0;
  box-sizing: border-box;
}

.filter-bar__toggle {
  flex-shrink: 0;
}

.filter-bar__suggestions {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: var(--z-popover);
  display: flex;
  flex-direction: column;
  padding: 4px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  box-shadow: var(--shadow-md);
  max-height: 260px;
  overflow: auto;
}

.filter-bar__suggestion {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.filter-bar__suggestion.is-active {
  background: color-mix(in oklab, var(--color-surface) 75%, transparent);
}

.filter-bar__suggestion-label {
  font-size: var(--text-sm, 0.875rem);
}

.filter-bar__suggestion-hint {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
}

.filter-bar__chips-row {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.filter-bar__chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 2px 4px 2px 10px;
  border: 1px solid color-mix(in oklab, var(--color-accent) 35%, var(--color-border));
  border-radius: var(--radius-pill, 999px);
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
  font-size: var(--text-xs, 0.75rem);
  white-space: nowrap;
}

.filter-bar__chip-label {
  color: var(--color-muted);
}

.filter-bar__chip-value {
  font-weight: 600;
}

.filter-bar__chip-remove {
  border: none;
  background: transparent;
  color: var(--color-muted);
  cursor: pointer;
  padding: 0 6px;
  border-radius: var(--radius-pill, 999px);
  line-height: 1.4;
}

.filter-bar__chip-remove:hover {
  background: color-mix(in oklab, var(--color-danger) 15%, transparent);
  color: var(--color-danger);
}

.filter-bar__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: var(--radius-pill, 999px);
  background: color-mix(in oklab, var(--color-accent) 25%, transparent);
  font-size: var(--text-xs, 0.75rem);
}

.filter-bar__panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 55%, transparent);
}

.filter-bar__panel-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.filter-bar__dropdown {
  position: relative;
  display: inline-flex;
}

.filter-bar__project-static {
  min-height: 32px;
  display: inline-flex;
  align-items: center;
  padding: calc(var(--space-2, 0.5rem) - 4px) var(--space-3, 0.75rem);
  border-color: transparent;
  background: transparent;
  color: var(--color-muted);
  box-shadow: none;
  cursor: default;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.filter-bar__dropdown-trigger {
  min-height: 32px;
  padding: calc(var(--space-2, 0.5rem) - 2px) var(--space-3, 0.75rem);
}

.filter-bar__dropdown-trigger-label {
  display: inline-block;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.filter-bar__menu-popover {
  position: absolute;
  top: calc(100% + var(--space-2, 0.5rem));
  left: 0;
  padding: var(--space-2, 0.5rem);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  box-shadow: var(--shadow-md);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  z-index: var(--z-popover);
  min-width: 220px;
  max-height: 280px;
  overflow: auto;
}

.filter-bar__menu-actions {
  display: flex;
  gap: var(--space-2, 0.5rem);
  flex-wrap: nowrap;
  align-items: center;
}

.filter-bar__menu-action {
  background: transparent;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--duration-fast) var(--ease-standard), border var(--duration-fast) var(--ease-standard);
}

.filter-bar__menu-action:hover {
  background: color-mix(in oklab, var(--color-surface) 70%, transparent);
  border-color: var(--color-border-strong);
}

.filter-bar__menu-item {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
  border-radius: var(--radius-md);
  cursor: pointer;
  user-select: none;
}

.filter-bar__menu-item:hover {
  background: color-mix(in oklab, var(--color-surface) 75%, transparent);
}

.filter-bar__menu-item input[type='checkbox'] {
  cursor: pointer;
}

.filter-bar__custom {
  flex: 1 1 280px;
  min-width: 240px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.filter-bar__custom-hint-wrapper {
  position: relative;
  display: inline-flex;
}

.filter-bar__custom-hint-btn {
  border: 1px solid var(--color-border);
  background: transparent;
  border-radius: var(--radius-pill);
  width: 22px;
  height: 22px;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  cursor: help;
  line-height: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.filter-bar__custom-hint-btn:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.filter-bar__custom-hint-btn--error {
  color: var(--color-danger);
  border-color: var(--color-danger);
}

.filter-bar__custom-hint-popover {
  position: absolute;
  bottom: calc(100% + 6px);
  right: 0;
  max-width: 260px;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface, var(--bg));
  box-shadow: var(--shadow-float);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  line-height: 1.3;
  pointer-events: none;
  z-index: var(--z-tooltip);
}

:global(.input.input--invalid) {
  border-color: var(--color-danger);
}

:global(.input.input--invalid:focus-visible) {
  box-shadow: var(--focus-ring-danger);
}
</style>
