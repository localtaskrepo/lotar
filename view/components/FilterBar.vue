<template>
  <div class="row" style="flex-wrap: wrap; gap:8px;">
    <UiInput v-model="query" placeholder="Search…" />
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
</template>
<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch, watchEffect } from 'vue'
import { useProjects } from '../composables/useProjects'
import { formatProjectLabel } from '../utils/projectLabels'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

const props = withDefaults(
  defineProps<{
    statuses?: string[]
    priorities?: string[]
    types?: string[]
    value?: Record<string, string>
    storageKey?: string
    showStatus?: boolean
    emitProjectKey?: boolean
    showOrder?: boolean
  }>(),
  {
    showStatus: true,
  },
)
const emit = defineEmits<{ (e:'update:value', v: Record<string,string>): void }>()

const query = ref('')
const project = ref('')
const status = ref('')
const priority = ref('')
const type = ref('')
const order = ref<'asc'|'desc'>('desc')
const tags = ref('')
const assignee = ref('')
const extraFilters = ref('')
const customFilterErrors = ref<string[]>([])
const customFilterInput = ref<{ focus: () => void } | null>(null)
let lastSyncedExtras = ''
const CUSTOM_UI_KEYS = new Set(['q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'order', 'due', 'recent', 'needs'])
const RESERVED_FIELD_ALIASES: Record<string, string> = {
  q: 'q',
  query: 'q',
  text: 'q',
  textquery: 'q',
  search: 'q',
  project: 'project',
  projectkey: 'project',
  status: 'status',
  state: 'status',
  priority: 'priority',
  prio: 'priority',
  type: 'type',
  tasktype: 'type',
  assignee: 'assignee',
  owner: 'assignee',
  tags: 'tags',
  tag: 'tags',
  order: 'order',
  sort: 'order',
  due: 'due',
  duedate: 'due',
  dueon: 'due',
  recent: 'recent',
  needs: 'needs',
  need: 'needs',
}
const customHintPopoverId = `custom-filter-hint-${Math.random().toString(36).slice(2, 8)}`
const customHintVisible = ref(false)
const showStatusSelect = computed(() => props.showStatus)
const showOrderSelect = computed(() => props.showOrder !== false)

const { projects, refresh } = useProjects()
const singleProject = computed(() => (projects.value.length === 1 ? projects.value[0] : null))
const hasSingleProject = computed(() => !!singleProject.value)
const singleProjectLabel = computed(() => {
  const p = singleProject.value
  return p ? formatProjectLabel(p) : ''
})

const DOCUMENT_CLICK_OPTS: AddEventListenerOptions = { capture: true }

function splitCsv(value: string): string[] {
  return value
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean)
}

function joinCsv(values: string[]): string {
  return values.join(',')
}

function toggleInCsv(csv: string, value: string): string {
  const trimmed = value.trim()
  if (!trimmed) return csv
  const next = new Set(splitCsv(csv))
  if (next.has(trimmed)) {
    next.delete(trimmed)
  } else {
    next.add(trimmed)
  }
  return joinCsv(Array.from(next))
}

function invertCsv(csv: string, universe: readonly string[]): string {
  if (!universe.length) return ''
  const selected = new Set(splitCsv(csv))
  const next = universe
    .map((v) => v.trim())
    .filter(Boolean)
    .filter((v) => !selected.has(v))
  return joinCsv(next)
}

const statusTitle = computed(() => {
  const values = splitCsv(status.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const statusHasSelections = computed(() => splitCsv(status.value).length > 0)
const statusSelections = computed(() => splitCsv(status.value))
const statusSelectionSet = computed(() => new Set(statusSelections.value))
const statusTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Status', statusSelections.value))
const priorityTitle = computed(() => {
  const values = splitCsv(priority.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const priorityHasSelections = computed(() => splitCsv(priority.value).length > 0)
const prioritySelections = computed(() => splitCsv(priority.value))
const prioritySelectionSet = computed(() => new Set(prioritySelections.value))
const priorityTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Priority', prioritySelections.value))
const typeTitle = computed(() => {
  const values = splitCsv(type.value)
  return values.length ? `Selected: ${values.join(', ')}` : ''
})
const typeHasSelections = computed(() => splitCsv(type.value).length > 0)
const typeSelections = computed(() => splitCsv(type.value))
const typeSelectionSet = computed(() => new Set(typeSelections.value))
const typeTriggerLabel = computed(() => formatMultiSelectTriggerLabel('Type', typeSelections.value))

function formatMultiSelectTriggerLabel(label: string, selected: string[]): string {
  if (!selected.length) return label
  if (selected.length <= 2) return `${label}: ${selected.join(', ')}`
  return `${label}: ${selected.slice(0, 2).join(', ')} (+${selected.length - 2})`
}

const statusMenuOpen = ref(false)
const priorityMenuOpen = ref(false)
const typeMenuOpen = ref(false)

const statusDropdown = ref<HTMLElement | null>(null)
const priorityDropdown = ref<HTMLElement | null>(null)
const typeDropdown = ref<HTMLElement | null>(null)

function closeAllMenus() {
  statusMenuOpen.value = false
  priorityMenuOpen.value = false
  typeMenuOpen.value = false
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

function toggleStatusValue(value: string) {
  status.value = toggleInCsv(status.value, value)
}

function togglePriorityValue(value: string) {
  priority.value = toggleInCsv(priority.value, value)
}

function toggleTypeValue(value: string) {
  type.value = toggleInCsv(type.value, value)
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

function onDocumentClick(event: MouseEvent) {
  const target = event.target as Node | null
  if (!target) return

  if (statusMenuOpen.value && statusDropdown.value && !statusDropdown.value.contains(target)) {
    statusMenuOpen.value = false
  }
  if (priorityMenuOpen.value && priorityDropdown.value && !priorityDropdown.value.contains(target)) {
    priorityMenuOpen.value = false
  }
  if (typeMenuOpen.value && typeDropdown.value && !typeDropdown.value.contains(target)) {
    typeMenuOpen.value = false
  }
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    closeAllMenus()
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
      const saved = JSON.parse(localStorage.getItem(FILTER_KEY.value) || 'null')
      if (saved && typeof saved === 'object') {
        query.value = saved.q || ''
        project.value = saved.project || ''
        status.value = saved.status || ''
        priority.value = saved.priority || ''
        type.value = saved.type || ''
        assignee.value = saved.assignee || ''
        tags.value = saved.tags || ''
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

watchEffect(() => {
  if (props.value) {
    query.value = props.value.q || ''
    project.value = props.value.project || ''
    status.value = props.value.status || ''
    priority.value = props.value.priority || ''
    type.value = props.value.type || ''
    const incomingAssignee = props.value.assignee || ''
    const isMine = props.value.mine === 'true' || incomingAssignee === '@me'
    assignee.value = isMine ? '@me' : incomingAssignee
    tags.value = props.value.tags || ''
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

function splitCustomTokens(input: string): string[] {
  return input
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean)
}

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

  splitCustomTokens(input).forEach((part) => {
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
  if (errors.length === 1) return errors[0]
  const [first, second] = errors
  const suffix = errors.length > 2 ? ` (+${errors.length - 2} more)` : ''
  return `${first}; ${second || ''}${suffix}`.trim()
}

const customFilterHint = computed(() => formatHelperMessage(customFilterErrors.value))
const hasCustomFilterError = computed(() => customFilterErrors.value.length > 0)
const shouldRenderCustomHint = computed(() => customHintVisible.value && customFilterHint.value.trim().length > 0)

function appendCustomFilter(expr: string) {
  const trimmed = expr.trim()
  if (!trimmed) return
  const tokens = splitCustomTokens(extraFilters.value)
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

function emitFilter(){
  const v: Record<string,string> = {}
  if (query.value) v.q = query.value
  const shouldEmitProject = props.emitProjectKey || !!project.value
  if (shouldEmitProject) v.project = project.value || ''
  if (status.value) v.status = status.value
  if (priority.value) v.priority = priority.value
  if (type.value) v.type = type.value
  if (assignee.value) v.assignee = assignee.value
  if (tags.value) v.tags = tags.value
  if (showOrderSelect.value) {
    v.order = order.value
  }
  const parsed = parseCustomFilters(extraFilters.value)
  customFilterErrors.value = parsed.errors
  Object.entries(parsed.map).forEach(([key, value]) => {
    if (value) v[key] = value
  })
  try { localStorage.setItem(FILTER_KEY.value, JSON.stringify(v)) } catch {}
  emit('update:value', v)
}
function onClear(){
  // Reset all local state and emit an empty filter
  query.value = ''
  project.value = ''
  status.value = ''
  priority.value = ''
  type.value = ''
  tags.value = ''
  assignee.value = ''
  if (showOrderSelect.value) {
    order.value = 'desc'
  }
  extraFilters.value = ''
  lastSyncedExtras = ''
  customFilterErrors.value = []
  try { localStorage.removeItem(FILTER_KEY.value) } catch {}
  const empty: Record<string,string> = {}
  if (showOrderSelect.value) {
    empty.order = 'desc'
  }
  emit('update:value', empty)
}

// Emit whenever any field changes; parent debounces/refetches
watch([query, project, status, priority, type, order, tags, extraFilters], emitFilter, { deep: false })

defineExpose({ appendCustomFilter, clear: onClear })
</script>
<style scoped>
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
