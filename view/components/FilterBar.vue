<template>
  <div class="row" style="flex-wrap: wrap; gap:8px;">
    <UiInput v-model="query" placeholder="Search…" />
    <UiSelect v-model="project">
      <option value="">Project</option>
      <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
    </UiSelect>
    <UiSelect v-model="status">
      <option value="">Status</option>
      <option v-for="s in statuses" :key="s" :value="s">{{ s }}</option>
    </UiSelect>
    <UiSelect v-model="priority">
      <option value="">Priority</option>
      <option v-for="p in priorities" :key="p" :value="p">{{ p }}</option>
    </UiSelect>
    <UiSelect v-model="type">
      <option value="">Type</option>
      <option v-for="t in types" :key="t" :value="t">{{ t }}</option>
    </UiSelect>
    <UiSelect v-model="order">
      <option value="desc">Newest</option>
      <option value="asc">Oldest</option>
    </UiSelect>
    <label class="row" style="gap:6px; align-items:center;">
      <input type="checkbox" v-model="mine" /> My tasks
    </label>
    <UiInput v-model="tags" placeholder="Tags (comma)" />
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
  <UiButton @click="onClear">Clear</UiButton>
  </div>
</template>
<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch, watchEffect } from 'vue'
import { useProjects } from '../composables/useProjects'
import { formatProjectLabel } from '../utils/projectLabels'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

const props = defineProps<{ statuses?: string[]; priorities?: string[]; types?: string[]; value?: Record<string, string> }>()
const emit = defineEmits<{ (e:'update:value', v: Record<string,string>): void }>()

const query = ref('')
const project = ref('')
const status = ref('')
const priority = ref('')
const type = ref('')
const order = ref<'asc'|'desc'>('desc')
const mine = ref(false)
const tags = ref('')
const assigneeOverride = ref('')
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

// Persist last used filter to sessionStorage for convenience
const FILTER_KEY = 'lotar.tasks.filter'
onMounted(() => {
  try {
    const hasIncoming = props.value && Object.keys(props.value).length > 0
    if (!hasIncoming) {
      const saved = JSON.parse(sessionStorage.getItem(FILTER_KEY) || 'null')
      if (saved && typeof saved === 'object') {
        query.value = saved.q || ''
        project.value = saved.project || ''
        status.value = saved.status || ''
        priority.value = saved.priority || ''
        type.value = saved.type || ''
        mine.value = saved.assignee === '@me'
        assigneeOverride.value = saved.assignee && saved.assignee !== '@me' ? saved.assignee : ''
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

const { projects, refresh } = useProjects()
onMounted(() => { refresh() })

watchEffect(() => {
  if (props.value) {
    query.value = props.value.q || ''
    project.value = props.value.project || ''
    status.value = props.value.status || ''
    priority.value = props.value.priority || ''
    type.value = props.value.type || ''
    const incomingAssignee = props.value.assignee || ''
    const isMine = props.value.mine === 'true' || incomingAssignee === '@me'
    mine.value = isMine
    assigneeOverride.value = !isMine && incomingAssignee ? incomingAssignee : ''
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
  if (project.value) v.project = project.value
  if (status.value) v.status = status.value
  if (priority.value) v.priority = priority.value
  if (type.value) v.type = type.value
  if (mine.value) {
    v.assignee = '@me'
  } else if (assigneeOverride.value) {
    v.assignee = assigneeOverride.value
  }
  if (tags.value) v.tags = tags.value
  v.order = order.value
  const parsed = parseCustomFilters(extraFilters.value)
  customFilterErrors.value = parsed.errors
  Object.entries(parsed.map).forEach(([key, value]) => {
    if (value) v[key] = value
  })
  try { sessionStorage.setItem(FILTER_KEY, JSON.stringify(v)) } catch {}
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
  mine.value = false
  assigneeOverride.value = ''
  order.value = 'desc'
  extraFilters.value = ''
  lastSyncedExtras = ''
  customFilterErrors.value = []
  try { sessionStorage.removeItem(FILTER_KEY) } catch {}
  const empty: Record<string,string> = { order: 'desc' }
  emit('update:value', empty)
}

// Emit whenever any field changes; parent debounces/refetches
watch([query, project, status, priority, type, order, mine, tags, extraFilters], emitFilter, { deep: false })

defineExpose({ appendCustomFilter })
</script>
<style scoped>
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
  border-radius: 999px;
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
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface, var(--bg));
  box-shadow: 0 8px 24px rgba(15, 23, 42, 0.18);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
  line-height: 1.3;
  pointer-events: none;
  z-index: 2;
}

:global(.input.input--invalid) {
  border-color: var(--color-danger);
}

:global(.input.input--invalid:focus-visible) {
  box-shadow: var(--focus-ring-danger);
}
</style>
