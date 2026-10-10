<template>
  <div class="filter-bar" :class="{ 'filter-bar--panel-open': panelOpen }">
    <div class="row filter-bar__bar">
      <div class="filter-bar__search">
        <div class="filter-bar__search-box">
          <input
            :id="controlId('search')"
            ref="searchInput"
            :value="searchDraft"
            class="input filter-bar__search-input"
            :class="{ 'input--invalid': searchErrors.length > 0 }"
            type="text"
            role="combobox"
            aria-autocomplete="list"
            placeholder="Search or type filters, e.g. status:todo"
            aria-label="Search tasks with filter syntax"
            :aria-expanded="suggestionsOpen && suggestions.length ? 'true' : 'false'"
            :aria-controls="suggestionsOpen && suggestions.length ? controlId('suggestions') : undefined"
            :aria-activedescendant="suggestionsOpen && suggestions.length && activeSuggestion < suggestions.length ? controlId(`suggestion-${activeSuggestion}`) : undefined"
            :aria-invalid="searchErrors.length > 0 ? 'true' : 'false'"
            :aria-describedby="searchErrors.length ? `${controlId('search-hint')} ${controlId('search-error')}` : controlId('search-hint')"
            data-testid="filter-search"
            autocomplete="off"
            spellcheck="false"
            @focus="suggestionsOpen = true"
            @blur="onSearchBlur"
            @input="onSearchInput"
            @keydown="onSearchKeydown"
          />
          <button
            v-if="searchDraft.length"
            type="button"
            class="filter-bar__search-clear"
            aria-label="Clear search"
            title="Clear search"
            data-testid="filter-search-clear"
            @click="clearSearch"
          >×</button>
          <div
            v-if="suggestionsOpen && suggestions.length"
            :id="controlId('suggestions')"
            class="filter-bar__suggestions"
            role="listbox"
            aria-label="Filter suggestions"
          >
            <button
              v-for="(item, index) in suggestions"
              :key="item.insert"
              :id="controlId(`suggestion-${index}`)"
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
        <p class="filter-bar__search-hint" :id="controlId('search-hint')">{{ searchSyntaxHint }}</p>
        <p
          v-if="searchErrors.length"
          class="filter-bar__search-error"
          :id="controlId('search-error')"
          role="alert"
        >
          {{ searchErrors.join(' ') }}
        </p>
      </div>

      <UiButton
        :id="controlId('help-toggle')"
        type="button"
        class="filter-bar__help-toggle"
        variant="ghost"
        icon-only
        aria-label="Search filter help"
        title="Search filter help"
        :aria-expanded="helpOpen ? 'true' : 'false'"
        :aria-controls="controlId('help')"
        data-testid="filter-help-toggle"
        @click="toggleHelp"
      ><IconGlyph name="help" /></UiButton>

      <UiButton
        :id="controlId('toggle')"
        :variant="panelOpen ? 'primary' : ''"
        type="button"
        class="filter-bar__toggle"
        aria-label="Toggle filters"
        :aria-expanded="panelOpen ? 'true' : 'false'"
        :aria-controls="controlId('panel')"
        title="Filters"
        data-testid="filter-toggle"
        @click="panelOpen = !panelOpen"
      >
        <IconGlyph name="filter" />
        <span class="filter-bar__toggle-label">Filters</span>
        <span v-if="activeCount" class="filter-bar__count">{{ activeCount }}</span>
      </UiButton>

      <div class="filter-bar__actions">
        <slot name="actions" />
      </div>
    </div>

    <section
      v-if="helpOpen"
      :id="controlId('help')"
      class="filter-bar__help"
      role="region"
      tabindex="0"
      :aria-labelledby="controlId('help-title')"
      data-testid="filter-help"
    >
      <h3 :id="controlId('help-title')">Search filters</h3>
      <p>Combine search words with filters. Arrow keys navigate suggestions; Enter or Tab accepts one. Enter applies a typed filter. Quote values containing spaces.</p>
      <dl class="filter-bar__help-types">
        <div v-for="item in filterHelp" :key="item.syntax">
          <dt><code>{{ item.syntax }}</code><span>{{ item.label }}</span></dt>
          <dd>{{ item.description }}<span v-if="item.aliases"> Aliases: {{ item.aliases }}.</span></dd>
        </div>
      </dl>
      <p>Use commas for multiple statuses, priorities, types, sprints, tags or missing fields. For multi-select filters, colon adds selections; <code>key=value</code> replaces them. Custom fields use <code>field:name=value</code> or <code>name=value</code>; quote reserved custom names, such as <code>field:"sprint"=inc-2</code>.</p>
      <p>Suggestions combine configured options with values from tasks loaded in the current project scope. Search text, assignees and custom values can also be typed directly. Unknown colon terms and quoted literals remain search text.</p>
    </section>

    <div v-if="activeChips.length" class="filter-bar__chips-row" data-testid="filter-chips" role="group" aria-label="Applied filters">
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

    <section
      v-if="panelOpen"
      :id="controlId('panel')"
      class="filter-bar__panel"
      role="region"
      :aria-labelledby="controlId('panel-title')"
      data-testid="filter-panel"
    >
      <div class="filter-bar__panel-header">
        <h3 :id="controlId('panel-title')" class="filter-bar__panel-title">Filters</h3>
        <div class="filter-bar__panel-header-actions">
          <UiButton
            type="button"
            class="filter-bar__panel-clear"
            :disabled="!hasConditions"
            @click="onClear(true)"
          >
            Clear conditions
          </UiButton>
        </div>
      </div>

      <div class="filter-bar__panel-content">
        <div class="filter-bar__picks-column">
      <section class="filter-bar__section" :aria-labelledby="controlId('quick-picks-title')">
        <h4 :id="controlId('quick-picks-title')" class="filter-bar__section-title">Quick picks</h4>
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
      </section>

      <section v-if="$slots.panel" class="filter-bar__section" :aria-labelledby="controlId('view-options-title')">
        <h4 :id="controlId('view-options-title')" class="filter-bar__section-title">View options</h4>
        <slot name="panel" />
      </section>
        </div>

      <div class="filter-bar__fields-group" role="group" aria-label="Filter fields">
        <div class="filter-bar__fields">
          <div class="filter-bar__field filter-bar__field--project">
            <span class="filter-bar__field-label" :id="controlId('project-label')">Project</span>
            <div
              v-if="hasSingleProject"
              class="input filter-bar__project-static"
              :aria-labelledby="controlId('project-label')"
              data-testid="filter-project"
              :title="singleProjectLabel"
            >
              {{ singleProjectLabel }}
            </div>
            <UiSelect
              v-else
              :id="controlId('project')"
              v-model="project"
              :aria-labelledby="controlId('project-label')"
              data-testid="filter-project"
            >
              <option value="">Project</option>
              <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
            </UiSelect>
          </div>

          <div v-if="showStatusSelect" class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('status-label')">Status</span>
            <div ref="statusDropdown" class="filter-bar__dropdown">
              <button
                type="button"
                :id="controlId('status')"
                class="input filter-bar__dropdown-trigger"
                :aria-labelledby="controlId('status-label')"
                data-testid="filter-status"
                :title="statusTitle"
                :aria-expanded="statusMenuOpen ? 'true' : 'false'"
                :aria-controls="statusMenuOpen ? controlId('status-options') : undefined"
                @click="toggleStatusMenu"
              >
                <span class="filter-bar__dropdown-trigger-label">{{ statusTriggerLabel }}</span>
              </button>
              <div
                v-if="statusMenuOpen"
                :id="controlId('status-options')"
                class="filter-bar__menu-popover"
                role="group"
                aria-label="Status choices"
                @click.stop
              >
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
          </div>

          <div class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('priority-label')">Priority</span>
            <div ref="priorityDropdown" class="filter-bar__dropdown">
              <button
                type="button"
                :id="controlId('priority')"
                class="input filter-bar__dropdown-trigger"
                :aria-labelledby="controlId('priority-label')"
                data-testid="filter-priority"
                :title="priorityTitle"
                :aria-expanded="priorityMenuOpen ? 'true' : 'false'"
                :aria-controls="priorityMenuOpen ? controlId('priority-options') : undefined"
                @click="togglePriorityMenu"
              >
                <span class="filter-bar__dropdown-trigger-label">{{ priorityTriggerLabel }}</span>
              </button>
              <div
                v-if="priorityMenuOpen"
                :id="controlId('priority-options')"
                class="filter-bar__menu-popover"
                role="group"
                aria-label="Priority choices"
                @click.stop
              >
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
          </div>

          <div class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('type-label')">Type</span>
            <div ref="typeDropdown" class="filter-bar__dropdown">
              <button
                type="button"
                :id="controlId('type')"
                class="input filter-bar__dropdown-trigger"
                :aria-labelledby="controlId('type-label')"
                data-testid="filter-type"
                :title="typeTitle"
                :aria-expanded="typeMenuOpen ? 'true' : 'false'"
                :aria-controls="typeMenuOpen ? controlId('type-options') : undefined"
                @click="toggleTypeMenu"
              >
                <span class="filter-bar__dropdown-trigger-label">{{ typeTriggerLabel }}</span>
              </button>
              <div
                v-if="typeMenuOpen"
                :id="controlId('type-options')"
                class="filter-bar__menu-popover"
                role="group"
                aria-label="Type choices"
                @click.stop
              >
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
          </div>

          <div v-if="showSprintSelect" class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('sprint-label')">Sprint</span>
            <div ref="sprintDropdown" class="filter-bar__dropdown">
              <button
                type="button"
                :id="controlId('sprint')"
                class="input filter-bar__dropdown-trigger"
                :aria-labelledby="controlId('sprint-label')"
                data-testid="filter-sprint"
                :title="sprintTitle"
                :aria-expanded="sprintMenuOpen ? 'true' : 'false'"
                :aria-controls="sprintMenuOpen ? controlId('sprint-options') : undefined"
                @click="toggleSprintMenu"
              >
                <span class="filter-bar__dropdown-trigger-label">{{ sprintTriggerLabel }}</span>
              </button>
              <div
                v-if="sprintMenuOpen"
                :id="controlId('sprint-options')"
                class="filter-bar__menu-popover"
                role="group"
                aria-label="Sprint choices"
                @click.stop
              >
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
          </div>

          <div v-if="showOrderSelect" class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('order-label')">Sort direction</span>
            <UiSelect :id="controlId('order')" v-model="order" :aria-labelledby="controlId('order-label')">
              <option value="desc">Descending</option>
              <option value="asc">Ascending</option>
            </UiSelect>
          </div>

          <div class="filter-bar__field">
            <span class="filter-bar__field-label" :id="controlId('deletion-label')">Task visibility</span>
            <UiSelect
              :id="controlId('deletion')"
              v-model="deletion"
              :aria-labelledby="controlId('deletion-label')"
              data-testid="task-deletion-filter"
              title="Show active tasks, deleted tasks (trash), or both"
            >
              <option value="active">Active tasks</option>
              <option value="deleted">Deleted tasks</option>
              <option value="all">All tasks</option>
            </UiSelect>
          </div>

        </div>
      </div>
      </div>
    </section>
  </div>
</template>
<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useId, watch, watchEffect } from 'vue'
import { listFromCsv } from '../composables/useFilterBuilder'
import { storageGetJson, storageRemove, storageSetJson } from '../utils/storage'
import { useProjects } from '../composables/useProjects'
import {
    chipsForFilterValue,
    canonicalCustomFilterKey,
    filterFragment,
    findGrammarKey,
    GRAMMAR_KEYS,
    parseFilterQuery,
    serializeFilterToken,
    suggestForFragment,
    type FilterChip,
    type SuggestionItem,
} from '../composables/useFilterGrammar'
import { formatProjectLabel } from '../utils/projectLabels'
import SmartListChips from './SmartListChips.vue'
import UiButton from './UiButton.vue'
import IconGlyph from './IconGlyph.vue'
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
    tagOptions?: string[]
    assigneeOptions?: string[]
    customFieldValues?: Record<string, string[]>
    customFieldOptions?: string[]
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
const searchPending = ref(false)
const suggestionsOpen = ref(false)
const activeSuggestion = ref(0)
const project = ref('')
const status = ref('')
const priority = ref('')
const type = ref('')
const sprintFilter = ref('')
const order = ref<'asc'|'desc'>('desc')
// DEV-92 deletion visibility: 'active' (default) never emits a filter key —
// the server default and clean URLs stay untouched; 'deleted'/'all' become
// part of the filter value (and therefore the query key and URL).
const deletion = ref<'active' | 'deleted' | 'all'>('active')
// Server sort key (`sort_by` wire value). The table headers own the value; the
// filter bar only preserves it across re-emits so it survives filter edits.
const sortBy = ref('')
const tags = ref('')
const assignee = ref('')
const dueDate = ref('')
const recent = ref('')
const needs = ref('')
const customFilters = ref<Record<string, string>>({})
const searchErrors = ref<string[]>([])
const filterId = useId()
const controlId = (name: string) => `${filterId}-filter-${name}`
const searchSyntaxHint = 'Enter applies filters: status:Todo field:iteration=beta.'
let searchBlurTimer: number | undefined
// Keys that have dedicated UI controls and must not fall through to the
// custom-field map. Derived from the grammar so aliases stay in one place.
const CUSTOM_UI_KEYS = new Set(['q', 'order', 'deletion', 'sort_by', ...GRAMMAR_KEYS.map((meta) => meta.key)])
const showStatusSelect = computed(() => props.showStatus)
const showOrderSelect = computed(() => props.showOrder !== false)

const panelOpen = ref(false)
const helpOpen = ref(false)
const helpDescriptions: Record<string, string> = {
  status: 'Configured workflow statuses.',
  priority: 'Configured priorities.',
  type: 'Configured task types.',
  sprints: 'Sprint IDs; suggestions show sprint names.',
  project: 'Project prefixes.',
  assignee: 'Member name, @me for yourself, or __none__ for unassigned tasks.',
  tags: 'Existing tags, or a tag you type.',
  due: 'today, soon, later or overdue.',
  recent: '7d: modified within the last seven days.',
  needs: 'effort or due: tasks missing those fields.',
  mine: 'true: tasks assigned to you.',
}
const filterHelp = computed(() => [
  ...GRAMMAR_KEYS.map(meta => ({ syntax: `${meta.key}:value`, label: meta.label, description: helpDescriptions[meta.key] || '', aliases: meta.aliases.filter(alias => alias !== meta.key).join(', ') })),
  { syntax: 'field:name=value', label: 'Custom field', description: 'A custom field name and value; known names and values are suggested.', aliases: '' },
  { syntax: 'q="search words"', label: 'Search text', description: 'Plain words search tasks; q= explicitly sets the search text.', aliases: '' },
  { syntax: 'deletion=active', label: 'Visibility', description: 'active, deleted or all.', aliases: '' },
  { syntax: 'order=asc', label: 'Sort direction', description: 'asc or desc.', aliases: '' },
  { syntax: 'sort_by=priority', label: 'Sort field', description: 'priority, status, effort, due-date, created, modified, assignee, reporter, title, type, project, id, tags, sprints or custom:name.', aliases: '' },
])

function toggleHelp() {
  helpOpen.value = !helpOpen.value
  suggestionsOpen.value = false
}

function closePanel() {
  closeAllMenus()
  panelOpen.value = false
  nextTick(() => document.getElementById(controlId('toggle'))?.focus())
}

function customFieldsFrom(value: Record<string, unknown>): Record<string, string> {
  return Object.fromEntries(Object.entries(value).filter(([key, raw]) => !CUSTOM_UI_KEYS.has(key) && typeof raw === 'string' && !!raw)) as Record<string, string>
}

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
  if (helpOpen.value && !['help', 'help-toggle', 'toggle', 'panel'].some(part => document.getElementById(controlId(part))?.contains(target))) helpOpen.value = false

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
  if (!searchInput.value?.isConnected || searchInput.value.closest('[inert], [hidden]')) return
  if (event.defaultPrevented) return
  if (event.key === 'Escape') {
    const openMenu = statusMenuOpen.value ? 'status' : priorityMenuOpen.value ? 'priority' : typeMenuOpen.value ? 'type' : sprintMenuOpen.value ? 'sprint' : ''
    if (openMenu) {
      closeAllMenus()
      document.getElementById(controlId(openMenu))?.focus()
      event.preventDefault()
      event.stopImmediatePropagation()
    } else if (helpOpen.value && (document.getElementById(controlId('help'))?.contains(event.target as Node) || document.getElementById(controlId('help-toggle'))?.contains(event.target as Node) || (event.target === searchInput.value && !panelOpen.value))) {
      helpOpen.value = false
      document.getElementById(controlId('help-toggle'))?.focus()
      event.preventDefault()
      event.stopImmediatePropagation()
    } else if (panelOpen.value) {
      const target = event.target as Node | null
      if (target && (document.getElementById(controlId('panel'))?.contains(target) || target === searchInput.value)) {
        closePanel()
        event.preventDefault()
        event.stopImmediatePropagation()
      }
    }
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
    deletion.value = normalizeDeletionValue(saved.deletion)
        // Sort persistence has a single owner: the page's per-project sort
        // storage (`lotar.tasks.sort::*`). Deliberately do NOT restore
        // sort_by/order from this snapshot — a stale one must never override
        // the page-restored sort on a plain route reload.
        customFilters.value = customFieldsFrom(saved)
      }
    }
  } catch {}
})

onUnmounted(() => {
  document.removeEventListener('click', onDocumentClick, DOCUMENT_CLICK_OPTS)
  document.removeEventListener('keydown', onDocumentKeydown)
  if (searchBlurTimer !== undefined) window.clearTimeout(searchBlurTimer)
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
    if (!searchPending.value && document.activeElement !== searchInput.value) {
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
    deletion.value = normalizeDeletionValue(props.value.deletion)
    const o = props.value.order
    order.value = (o === 'asc' || o === 'desc') ? o : order.value
    sortBy.value = props.value.sort_by || ''
    const extras = customFieldsFrom(props.value)
    if (JSON.stringify(extras) !== JSON.stringify(customFilters.value)) customFilters.value = extras
  } else if (Object.keys(customFilters.value).length) {
    customFilters.value = {}
  }
})


function appendCustomFilter(expr: string) {
  const trimmed = expr.trim()
  if (!trimmed) return
  const token = trimmed.endsWith('=') ? serializeFilterToken(trimmed.slice(0, -1), '') : trimmed
  const current = searchDraft.value.trimEnd()
  if (!current.endsWith(token)) searchDraft.value = [current, token].filter(Boolean).join(' ')
  searchErrors.value = []
  searchPending.value = true
  if (!trimmed.endsWith('=')) applySearchDraft()
  suggestionsOpen.value = true
  nextTick(() => searchInput.value?.focus())
}

// --- Grammar search with suggestions ---

const suggestionSource = computed(() => ({
  statuses: props.statuses ?? [],
  priorities: props.priorities ?? [],
  types: props.types ?? [],
  sprints: props.sprintOptions ?? [],
  projects: projects.value ?? [],
  tags: props.tagOptions ?? [],
  assignees: props.assigneeOptions ?? [],
  customFieldValues: props.customFieldValues ?? {},
  customFields: [...new Set([
    ...(props.customFieldOptions ?? []).filter(name => name && name !== '*'),
    ...(props.customPresets ?? []).map(preset => canonicalCustomFilterKey(preset.expression.replace(/=$/, '')).replace(/^field:/i, '')),
    ...Object.keys(props.customFieldValues ?? {}),
  ])],
}))

const suggestions = computed<SuggestionItem[]>(() => {
  const fragment = currentFragment()
  if (!fragment && !searchDraft.value.trim()) return []
  return suggestForFragment(fragment, suggestionSource.value)
})

watch(suggestions, next => {
  if (activeSuggestion.value >= next.length) activeSuggestion.value = 0
})

function currentFragment(): string {
  return filterFragment(searchDraft.value).fragment
}

function replaceFragment(replacement: string): void {
  const { prefix } = filterFragment(searchDraft.value)
  // Key prefixes are inserted without a trailing space so the
  // current fragment stays "key:" and value suggestions appear immediately.
  const suffix = !replacement || replacement.endsWith(':') || replacement.endsWith('=') ? '' : ' '
  searchDraft.value = `${prefix}${replacement}${suffix}`
}

function onSearchInput(event: Event) {
  suggestionsOpen.value = true
  activeSuggestion.value = 0
  const value = (event.target as HTMLInputElement | null)?.value ?? ''
  searchDraft.value = value
  searchErrors.value = []
  const parsed = parseFilterQuery(value)
  const fragment = currentFragment()
  const fragmentKey = fragment.split(':')[0] || ''
  const keyPrefix = fragment.toLowerCase()
  const completingKey = !!keyPrefix && ['field', 'q', 'order', 'sort_by', 'deletion', ...GRAMMAR_KEYS.flatMap(meta => meta.aliases)].some(key => key.startsWith(keyPrefix))
  searchPending.value = !!(Object.keys(parsed.filters).length || parsed.errors?.length || completingKey || keyPrefix.startsWith('field:') || (fragment.includes(':') && findGrammarKey(fragmentKey)))
  // Structured drafts are committed on Enter; normal text still searches live.
  if (!searchPending.value) query.value = parsed.text
}

function onSearchBlur() {
  // Delay so mousedown-based picks register before the blur clears state.
  if (searchBlurTimer !== undefined) window.clearTimeout(searchBlurTimer)
  searchBlurTimer = window.setTimeout(() => {
    if (document.activeElement !== searchInput.value) suggestionsOpen.value = false
  }, 120)
}

function pickSuggestion(item: SuggestionItem) {
  if (!item.insert.endsWith(':') && !item.insert.endsWith('=')) {
    replaceFragment(item.insert)
    applySearchDraft()
    suggestionsOpen.value = false
    activeSuggestion.value = 0
  } else {
    // Key prefix picked: keep the list open so the values for this key are
    // offered right away (the prefix is inserted without a trailing space).
    replaceFragment(item.insert)
    searchPending.value = true
    activeSuggestion.value = 0
    suggestionsOpen.value = true
  }
  searchInput.value?.focus()
}

function clearSearch() {
  query.value = ''
  searchDraft.value = ''
  searchPending.value = false
  searchErrors.value = []
  suggestionsOpen.value = false
  activeSuggestion.value = 0
  if (searchBlurTimer !== undefined) window.clearTimeout(searchBlurTimer)
  searchBlurTimer = undefined
  nextTick(() => searchInput.value?.focus())
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
    const highlighted = suggestionsOpen.value && currentFragment().trim() ? suggestions.value[activeSuggestion.value] : undefined
    if (highlighted) {
      pickSuggestion(highlighted)
      return
    }
    applySearchDraft()
    suggestionsOpen.value = false
  } else if (event.key === 'Escape') {
    if (suggestionsOpen.value && suggestions.value.length) {
      suggestionsOpen.value = false
      event.preventDefault()
      event.stopPropagation()
    } else suggestionsOpen.value = false
  }
}

function applyGrammarFilters(filters: Record<string, string>, replaceKeys: string[] = []) {
  const replacements = new Set(replaceKeys)
  const csvValue = (key: string, current: string, value: string) => replacements.has(key) ? value : mergeIntoCsv(current, value)
  const extras: Record<string, string> = Object.assign(Object.create(null), customFilters.value)
  for (const [key, value] of Object.entries(filters)) {
    switch (key) {
      case 'status':
        status.value = csvValue(key, status.value, value)
        break
      case 'priority':
        priority.value = csvValue(key, priority.value, value)
        break
      case 'type':
        type.value = csvValue(key, type.value, value)
        break
      case 'sprints':
        sprintFilter.value = csvValue(key, sprintFilter.value, value)
        break
      case 'assignee':
        assignee.value = value
        break
      case 'tags':
        tags.value = csvValue(key, tags.value, value)
        break
      case 'due':
        dueDate.value = value
        break
      case 'recent':
        recent.value = value
        break
      case 'needs':
        needs.value = csvValue(key, needs.value, value)
        break
      case 'mine':
        if (value === 'true') assignee.value = '@me'
        break
      case 'project':
        project.value = value
        break
      case 'order':
        order.value = value as 'asc' | 'desc'
        break
      case 'sort_by':
        sortBy.value = value
        break
      case 'deletion':
        deletion.value = value as 'active' | 'deleted' | 'all'
        break
      case 'q':
        break
      default:
        extras[key] = value
    }
  }
  if (JSON.stringify(extras) !== JSON.stringify(customFilters.value)) customFilters.value = extras
}

function applySearchDraft() {
  const parsed = parseFilterQuery(searchDraft.value)
  const errors = [...(parsed.errors ?? [])]
  if (parsed.filters.order && !['asc', 'desc'].includes(parsed.filters.order)) errors.push('Sort direction must be asc or desc.')
  if (parsed.filters.deletion && !['active', 'deleted', 'all'].includes(parsed.filters.deletion)) errors.push('Visibility must be active, deleted, or all.')
  searchErrors.value = errors
  if (errors.length) return
  applyGrammarFilters(parsed.filters, parsed.replaceKeys)
  query.value = parsed.filters.q ?? parsed.text
  searchDraft.value = query.value
  searchPending.value = false
}

// --- Active filter chips ---

const structuredFilterValue = computed<Record<string, string>>(() => {
  const v: Record<string, string> = { ...customFilters.value }
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

const activeChips = computed<FilterChip[]>(() => {
  const chips = chipsForFilterValue(structuredFilterValue.value, suggestionSource.value)
  for (const tag of listFromCsv(tags.value)) chips.push({ key: 'tags', label: 'Tag', value: tag, display: tag })
  if (deletion.value !== 'active') chips.push({ key: 'deletion', label: 'Visibility', value: deletion.value, display: deletion.value === 'deleted' ? 'Deleted tasks' : 'All tasks' })
  return chips
})

const activeCount = computed(() => activeChips.value.length + (query.value.trim() ? 1 : 0))
const hasConditions = computed(() => !!(query.value || status.value || priority.value || type.value || sprintFilter.value || assignee.value || tags.value || dueDate.value || recent.value || needs.value || deletion.value !== 'active' || Object.keys(customFilters.value).length))

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
      case 'deletion':
        deletion.value = 'active'
        break
      default: {
        const next = { ...customFilters.value }
        delete next[chip.key]
        customFilters.value = next
      }
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
  const v: Record<string,string> = { ...customFilters.value }
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
  if (deletion.value !== 'active') v.deletion = deletion.value
  v.order = order.value
  if (sortBy.value) {
    v.sort_by = sortBy.value
  }
  // Persist the snapshot WITHOUT the sort keys (see the restore note above):
  // the live emit carries them for the current page, the snapshot must not.
  const { sort_by: _snapshotSortBy, order: _snapshotOrder, ...snapshot } = v
  storageSetJson(FILTER_KEY.value, snapshot)
  emit('update:value', v)
}
function onClear(keepScope = false){
  // Reset all local state and emit an empty filter
  query.value = ''
  searchDraft.value = ''
  searchPending.value = false
  searchErrors.value = []
  if (!keepScope) project.value = ''
  status.value = ''
  priority.value = ''
  type.value = ''
  sprintFilter.value = ''
  tags.value = ''
  assignee.value = ''
  dueDate.value = ''
  recent.value = ''
  needs.value = ''
  if (!keepScope) {
    order.value = 'desc'
    sortBy.value = ''
  }
  deletion.value = 'active'
  customFilters.value = {}
  storageRemove(FILTER_KEY.value)
  const empty: Record<string,string> = {}
  empty.order = order.value
  if (keepScope && (props.emitProjectKey || project.value)) empty.project = project.value
  if (keepScope && sortBy.value) empty.sort_by = sortBy.value
  emit('update:value', empty)
}

function normalizeDeletionValue(value: unknown): 'active' | 'deleted' | 'all' {
  return value === 'deleted' || value === 'all' ? value : 'active'
}

// Emit whenever any field changes; parent debounces/refetches
watch([query, project, status, priority, type, sprintFilter, order, sortBy, tags, customFilters, assignee, dueDate, recent, needs, deletion], emitFilter, { deep: false })

defineExpose({ appendCustomFilter, clear: onClear })
</script>
<style scoped>
.filter-bar {
  container-type: inline-size;
  container-name: filter-controls;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.filter-bar__bar {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  flex-wrap: wrap;
}

.filter-bar__search {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1 1 180px;
  min-width: 140px;
  /* Cap the stretch so ultrawide layouts keep the bar compact; page-level
     actions right-align against the container's max-width instead. */
  max-width: 440px;
}

.filter-bar__search-box {
  position: relative;
}

.filter-bar__search-hint {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  clip-path: inset(50%);
  white-space: nowrap;
  border: 0;
}

.filter-bar__search-error {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-danger);
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
  padding-right: 36px;
}

.filter-bar__search-clear {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: var(--radius, 4px);
  background: transparent;
  color: var(--color-muted);
  font-size: 18px;
  cursor: pointer;
}

.filter-bar__search-clear:hover,
.filter-bar__search-clear:focus-visible {
  background: var(--color-surface);
  color: var(--color-fg);
}

.filter-bar__toggle {
  flex-shrink: 0;
}

.filter-bar__help-toggle.btn {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  padding: 0;
  margin-left: -4px;
  border: 0;
}

.filter-bar__help-toggle.btn[aria-expanded="true"] {
  background: color-mix(in oklab, var(--color-accent) 14%, transparent);
  color: var(--color-accent);
}

.filter-bar__help {
  padding: 14px 16px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  font-size: var(--text-sm, 0.875rem);
  max-height: min(60vh, 520px);
  overflow: auto;
}

.filter-bar__help h3 {
  margin: 0;
  font-size: inherit;
}

.filter-bar__help p {
  margin: 8px 0 0;
  color: var(--color-muted);
}

.filter-bar__help-types {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 250px), 1fr));
  gap: 12px 24px;
  margin: 16px 0;
}

.filter-bar__help dt {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
}

.filter-bar__help dt span {
  color: var(--color-muted);
  font-size: var(--text-xs, 0.75rem);
}

.filter-bar__help dd {
  margin: 4px 0 0;
  color: var(--color-muted);
}

.filter-bar__help code {
  overflow-wrap: anywhere;
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

.filter-bar__panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.filter-bar__panel-title {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
  font-weight: 600;
}

.filter-bar__panel-header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.filter-bar__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.filter-bar__section-title,
.filter-bar__field-label {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  color: var(--color-muted);
  line-height: 1.4;
}

.filter-bar__panel-content,
.filter-bar__picks-column {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
  min-width: 0;
}

.filter-bar__fields-group {
  min-width: 0;
}

@container filter-controls (min-width: 1000px) {
  .filter-bar__panel-content {
    display: grid;
    grid-template-columns: minmax(240px, 0.4fr) minmax(0, 1fr);
    align-items: start;
    gap: var(--space-4, 1rem);
  }
}

.filter-bar__fields {
  display: grid;
  gap: var(--space-2, 0.5rem);
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 156px), 1fr));
}

.filter-bar__field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  min-width: 0;
}

.filter-bar__field-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Selects, inputs, and dropdown triggers fill their grid cell so the panel
   never grows horizontal overflow; labels ellipsize instead. */
.filter-bar__field .input {
  width: 100%;
  box-sizing: border-box;
}

.filter-bar__dropdown {
  position: relative;
  display: flex;
  width: 100%;
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
  display: flex;
  align-items: center;
  text-align: left;
}

.filter-bar__dropdown-trigger-label {
  flex: 1 1 auto;
  min-width: 0;
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
  min-width: min(220px, calc(100vw - 24px));
  max-width: calc(100vw - 24px);
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
  flex-shrink: 0;
}

.filter-bar__menu-item span {
  min-width: 0;
  overflow-wrap: anywhere;
}

@media (max-width: 640px) {
  .filter-bar__toggle {
    position: relative;
    width: 36px;
    height: 36px;
    padding: 0;
    gap: 0;
  }

  .filter-bar__toggle-label {
    display: none;
  }

  .filter-bar__toggle .filter-bar__count {
    position: absolute;
    top: -5px;
    right: -5px;
    min-width: 14px;
    height: 14px;
    font-size: 10px;
  }

  .filter-bar__actions {
    flex-wrap: nowrap;
  }

  .filter-bar__fields {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .filter-bar__field--project {
    grid-column: 1 / -1;
  }

  .filter-bar__menu-popover {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    max-width: 100%;
  }
}

.filter-bar__search-input.input--invalid {
  border-color: var(--color-danger);
}

.filter-bar__search-input.input--invalid:focus-visible {
  box-shadow: var(--focus-ring-danger);
}
</style>
