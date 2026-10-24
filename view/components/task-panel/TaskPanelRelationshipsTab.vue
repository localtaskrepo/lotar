<template>
  <div class="task-panel__tab-panel relationship-editor" @dragover="onDragOver">
    <div class="relationship-toolbar">
      <span class="muted">Drag links between categories.</span>
      <a v-if="taskId && mode === 'edit'" :href="graphHref">Open dependency view</a>
    </div>
    <p v-if="error" role="alert">{{ error }}</p>
    <p v-if="disabled" class="muted">Restore this task before changing relationships.</p>
    <section v-for="category in relationshipCategories" :key="category.key" class="relationship-category"
      :aria-label="category.label" @drop="onDrop($event, category.key)">
      <h3 :title="category.description">{{ category.label }}</h3>
      <div class="relationship-values">
      <span v-if="!entries(category.key).length" class="muted relationship-empty">None</span>
      <ul class="relationship-chips">
        <li v-for="id in entries(category.key)" :key="id" :draggable="!blocked"
          @dragstart="onDragStart($event, category.key, id)" @dragend="dragging = null">
          <a :href="`/task/${encodeURIComponent(id)}`" @click="navigate($event, id)">{{ id }}</a>
          <button class="relationship-icon" type="button" :disabled="blocked" :aria-label="`Remove ${id} from ${category.label}`"
            :title="`Remove ${id} from ${category.label}`" @click="submit({ action: 'remove', kind: category.key, target: id })"><IconGlyph name="close" /></button>
        </li>
      </ul>
      <button :id="`${editorId}-${category.key}-add`" class="relationship-icon relationship-add-toggle" type="button" :disabled="blocked"
        :aria-label="`Add ${category.label.toLowerCase()}`" :title="`Add ${category.label.toLowerCase()}`" :aria-expanded="activeAdd === category.key" :aria-controls="`${editorId}-${category.key}-editor`"
        @click="toggleAdd(category.key)"><IconGlyph :name="activeAdd === category.key ? 'close' : 'plus'" /></button>
      </div>
      <div v-if="activeAdd === category.key" :id="`${editorId}-${category.key}-editor`" class="relationship-add" role="group" :aria-label="`Add ${category.label.toLowerCase()}`" @keydown.esc.stop="closeAdd(category.key)">
        <input :id="`${editorId}-${category.key}`" v-model="queries[category.key]" class="input" :disabled="blocked"
          :aria-label="`Add ${category.label.toLowerCase()}`" placeholder="Task ID or title" autocomplete="off" role="combobox" aria-autocomplete="list"
          :aria-expanded="!!suggestions[category.key]?.length" :aria-controls="`${editorId}-${category.key}-suggestions`"
          :aria-activedescendant="suggestions[category.key]?.length ? `${editorId}-${category.key}-option-${activeIndex[category.key] || 0}` : undefined"
          @input="search(category.key)" @keydown="onKey($event, category.key)" @blur="dismiss(category.key)" />
        <button class="btn" type="button" :disabled="blocked || !queries[category.key]?.trim()" @click="choose(category.key, queries[category.key] || '')">Add</button>
        <ul v-if="suggestions[category.key]?.length" :id="`${editorId}-${category.key}-suggestions`" role="listbox" :aria-label="`${category.label} suggestions`">
          <li v-for="(item, index) in suggestions[category.key]" :id="`${editorId}-${category.key}-option-${index}`"
            :key="item.id" role="option" :aria-selected="index === (activeIndex[category.key] || 0)">
            <button class="btn ghost" type="button" @mousedown.prevent @click="choose(category.key, item.id)">{{ item.id }} {{ item.title }}</button>
          </li>
        </ul>
      </div>
    </section>
    <section v-if="pending" role="alertdialog" aria-label="Confirm relationship change" class="relationship-confirm">
      <h3>Confirm relationship change</h3>
      <p>{{ pending.from_kind ? relationshipMoveDescription(pending.from_kind, pending.kind) : 'Add this relationship.' }}</p>
      <p v-if="pending.expected_current">Replace {{ pending.expected_current }} with {{ pending.target }} in {{ label(pending.kind) }}. The server will refuse if the current value has changed.</p>
      <p v-else>Move {{ pending.target }} to {{ label(pending.kind) }}.</p>
      <button class="btn primary" type="button" :disabled="blocked" @click="confirmPending">Confirm change</button>
      <button class="btn" type="button" :disabled="blocked" @click="pending = null; focusControl()">Cancel</button>
    </section>
    <details v-if="mode === 'edit' && taskId" class="relationship-preview" :open="graphExpanded" @toggle="graphExpanded = ($event.target as HTMLDetailsElement).open">
      <summary>Dependency preview</summary>
      <DependencyGraph v-if="graphExpanded" :root="taskId" :project="project" />
    </details>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, useId, watch } from 'vue'
import { api } from '../../api/client'
import { useTaskPanelController } from '../../composables/useTaskPanelController'
import { relationshipCategories, relationshipEntries, relationshipMime, relationshipMoveDescription, singletonRelationship, type RelationshipCategory, type RelationshipIntent } from '../../utils/relationships'
import DependencyGraph from '../DependencyGraph.vue'
import IconGlyph from '../IconGlyph.vue'

const props = defineProps<{
  mode: 'create' | 'edit'
  taskId?: string
  project?: string
  disabled?: boolean
  relationships: Record<string, string>
  mutate: (intent: RelationshipIntent) => Promise<void>
}>()
const editorId = useId()
const { navigateTask } = useTaskPanelController()
const busy = ref(false)
const blocked = computed(() => busy.value || props.disabled)
const error = ref('')
const queries = reactive<Record<string, string>>({})
const suggestions = reactive<Record<string, Array<{ id: string; title: string }>>>({})
const activeIndex = reactive<Record<string, number>>({})
const pending = ref<RelationshipIntent | null>(null)
const dragging = ref<{ kind: RelationshipCategory; id: string } | null>(null)
const activeAdd = ref<RelationshipCategory | null>(null)
const graphExpanded = ref(false)
const graphHref = computed(() => `/dependencies?${new URLSearchParams({ root: props.taskId || '', ...(props.project ? { project: props.project } : {}) })}`)
let generation = 0
let searchSequence = 0
const requests: Record<string, number> = {}
const timers = new Map<string, ReturnType<typeof setTimeout>>()
function clear() {
  generation += 1
  busy.value = false
  error.value = ''
  pending.value = null
  dragging.value = null
  activeAdd.value = null
  graphExpanded.value = false
  timers.forEach(clearTimeout)
  timers.clear()
  for (const key of Object.keys(queries)) delete queries[key]
  for (const key of Object.keys(suggestions)) delete suggestions[key]
  for (const key of Object.keys(requests)) delete requests[key]
}
watch(() => [props.taskId, props.mode, props.project, props.disabled], clear)
onBeforeUnmount(clear)
const entries = (kind: RelationshipCategory) => relationshipEntries(props.relationships, kind)
const label = (kind: RelationshipCategory) => relationshipCategories.find((entry) => entry.key === kind)?.label || kind
function navigate(event: MouseEvent, id: string) {
  if (event.button || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return
  event.preventDefault()
  void navigateTask(id)
}
function dismiss(kind: string) {
  requests[kind] = ++searchSequence
  clearTimeout(timers.get(kind))
  suggestions[kind] = []
}
async function toggleAdd(kind: RelationshipCategory) {
  if (activeAdd.value === kind) { closeAdd(kind); return }
  if (activeAdd.value) dismiss(activeAdd.value)
  activeAdd.value = kind
  const scope = generation
  await nextTick()
  if (scope === generation) document.getElementById(`${editorId}-${kind}`)?.focus()
}
function focusControl(fallback?: RelationshipCategory) {
  const id = activeAdd.value ? `${editorId}-${activeAdd.value}` : fallback ? `${editorId}-${fallback}-add` : ''
  document.getElementById(id)?.focus()
}
function closeAdd(kind: RelationshipCategory) {
  dismiss(kind)
  activeAdd.value = null
  document.getElementById(`${editorId}-${kind}-add`)?.focus()
}
function search(kind: RelationshipCategory) {
  dismiss(kind)
  const scope = generation
  const request = requests[kind] = ++searchSequence
  const query = (queries[kind] || '').trim()
  if (!query) return
  timers.set(kind, setTimeout(async () => {
    try {
      const result = await api.suggestTasks(query, props.project || undefined, 12)
      if (scope !== generation || requests[kind] !== request) return
      suggestions[kind] = result.filter((item) => item.id !== props.taskId && !entries(kind).includes(item.id))
      activeIndex[kind] = 0
    } catch {
      if (scope === generation && requests[kind] === request) suggestions[kind] = []
    }
  }, 160))
}
function onKey(event: KeyboardEvent, kind: RelationshipCategory) {
  const list = suggestions[kind] || []
  if (event.key === 'Escape') { dismiss(kind); return }
  if (event.key === 'Enter') {
    event.preventDefault()
    event.stopPropagation()
    const item = list[activeIndex[kind] || 0]
    choose(kind, item?.id || queries[kind] || '')
    return
  }
  if (!list.length) return
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    activeIndex[kind] = ((activeIndex[kind] || 0) + (event.key === 'ArrowDown' ? 1 : -1) + list.length) % list.length
  }
}
function choose(kind: RelationshipCategory, raw: string, from_kind?: RelationshipCategory) {
  const target = raw.trim()
  if (blocked.value || !target || from_kind === kind || (!from_kind && entries(kind).includes(target))) return
  if (target === props.taskId) { error.value = 'A task cannot reference itself.'; return }
  dismiss(kind)
  const current = singletonRelationship(kind) ? entries(kind)[0] : undefined
  const intent: RelationshipIntent = { action: from_kind ? 'change_kind' : 'add', kind, target, ...(from_kind ? { from_kind } : {}) }
  if (current && current !== target) intent.expected_current = current
  if (intent.expected_current || (from_kind && relationshipMoveDescription(from_kind, kind).startsWith('This reverses'))) {
    pending.value = intent
  } else void submit(intent)
}
async function submit(intent: RelationshipIntent) {
  if (blocked.value) return
  const scope = generation
  busy.value = true
  error.value = ''
  try {
    await props.mutate(intent)
    if (scope === generation) { queries[intent.kind] = ''; pending.value = null }
  } catch (cause) {
    if (scope === generation) { error.value = cause instanceof Error ? cause.message : 'Relationship change failed. Reload and retry.'; pending.value = null }
  } finally {
    if (scope === generation) {
      busy.value = false
      await nextTick()
      if (scope === generation) focusControl(intent.kind)
    }
  }
}
function confirmPending() { if (pending.value) void submit(pending.value) }
function onDragStart(event: DragEvent, kind: RelationshipCategory, id: string) {
  if (blocked.value) { event.preventDefault(); return }
  dragging.value = { kind, id }
  event.dataTransfer?.setData(relationshipMime, JSON.stringify({ task: props.taskId, kind, id }))
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}
function onDragOver(event: DragEvent) {
  if (!blocked.value && dragging.value && Array.from(event.dataTransfer?.types || []).includes(relationshipMime)) event.preventDefault()
}
function onDrop(event: DragEvent, kind: RelationshipCategory) {
  if (!dragging.value || !Array.from(event.dataTransfer?.types || []).includes(relationshipMime)) return
  event.preventDefault()
  event.stopPropagation()
  choose(kind, dragging.value.id, dragging.value.kind)
  dragging.value = null
}
</script>

<style scoped>
.relationship-editor { display: grid; gap: 0; }
.relationship-toolbar { display: flex; align-items: center; gap: 8px; padding-bottom: 12px; flex-wrap: wrap; font-size: .85em; min-width: 0; }
.relationship-toolbar > span { flex: 1; }
.relationship-category { display: grid; grid-template-columns: 100px minmax(0, 1fr); align-items: center; column-gap: 12px; padding: 7px 0; border-top: 1px solid var(--border); }
.relationship-category h3 { margin: 0; font-size: .85em; font-weight: 600; }
.relationship-values { display: flex; gap: 6px; align-items: center; min-width: 0; }
.relationship-empty { font-size: .85em; }
.relationship-chips { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 5px; min-width: 0; }
.relationship-chips:empty { display: none; }
.relationship-chips li { display: inline-flex; align-items: center; gap: 4px; border: 1px solid var(--border); padding: 2px 3px 2px 8px; border-radius: 20px; font-size: .85em; max-width: 100%; }
.relationship-chips li > a { overflow-wrap: anywhere; min-width: 0; }
.relationship-icon { display: inline-flex; justify-content: center; align-items: center; width: 26px; height: 26px; flex: 0 0 26px; padding: 0; color: var(--muted); background: transparent; border: 0; border-radius: var(--radius-sm); cursor: pointer; }
.relationship-icon:hover { color: var(--fg); background: var(--surface-contrast); }
.relationship-icon:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.relationship-icon:disabled { opacity: .5; cursor: default; }
.relationship-add-toggle { margin-left: auto; }
.relationship-add { grid-column: 1 / -1; margin-top: 8px; padding: 8px; background: var(--surface-contrast); border-radius: var(--radius-sm); }
.relationship-add { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.relationship-add input { box-sizing: border-box; min-width: 0; max-width: 100%; flex: 1; min-height: var(--control-height); height: var(--control-height); }
.relationship-add ul { flex-basis: 100%; list-style: none; padding: 0; }
.relationship-add [aria-selected="true"] { background: var(--surface-contrast); outline: 1px solid var(--accent); }
.relationship-confirm { position: sticky; bottom: 0; padding: 16px; background: var(--surface, var(--bg)); border: 2px solid var(--border); border-radius: var(--radius-sm); z-index: 2; }
.relationship-confirm button + button { margin-left: 8px; }
.relationship-preview { border-top: 1px solid var(--border); margin-top: 4px; }
.relationship-preview > summary { padding: 12px 0; cursor: pointer; font-size: .9em; color: var(--muted); }
@media (max-width: 600px) { .relationship-category { grid-template-columns: 88px minmax(0, 1fr); column-gap: 8px; } .relationship-toolbar > span { flex-basis: 100%; } }
</style>
