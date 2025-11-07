<template>
  <div class="task-editor col" style="gap: 12px;">
    <div class="row" style="gap:12px; align-items: center; flex-wrap: wrap;">
      <UiInput v-model="form.title" placeholder="Title" style="min-width: 320px; flex:1;" />
      <UiSelect v-model="form.project" :disabled="mode==='edit'">
        <option value="">Project</option>
        <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ formatProjectLabel(p) }}</option>
      </UiSelect>
      <UiSelect v-model="form.status">
        <option value="">Status</option>
        <option v-for="s in statuses" :key="s" :value="s">{{ s }}</option>
      </UiSelect>
      <UiSelect v-model="form.priority">
        <option value="">Priority</option>
        <option v-for="p in priorities" :key="p" :value="p">{{ p }}</option>
      </UiSelect>
      <UiSelect v-model="form.task_type">
        <option value="">Type</option>
        <option v-for="t in types" :key="t" :value="t">{{ t }}</option>
      </UiSelect>
    </div>
    <div class="row" style="gap:12px; align-items:center; flex-wrap: wrap;">
      <div class="col suggest-wrap">
        <UiInput v-model="form.reporter" placeholder="Reporter (@me supported)" @focus="reporterOpen=true" @keydown="onUserKey('reporter', $event)" />
        <ul v-if="reporterOpen && reporterOpts.length" class="suggest">
          <li v-for="(u,i) in reporterOpts" :key="u" :class="{ active: reporterIdx===i }" @mouseenter="reporterIdx=i" @mousedown.prevent="pickUser('reporter', u)">@{{ u }}</li>
        </ul>
      </div>
      <div class="col suggest-wrap">
        <UiInput v-model="form.assignee" placeholder="Assignee (@me supported)" @focus="assigneeOpen=true" @keydown="onUserKey('assignee', $event)" />
        <ul v-if="assigneeOpen && assigneeOpts.length" class="suggest">
          <li v-for="(u,i) in assigneeOpts" :key="u" :class="{ active: assigneeIdx===i }" @mouseenter="assigneeIdx=i" @mousedown.prevent="pickUser('assignee', u)">@{{ u }}</li>
        </ul>
      </div>
      <UiInput v-model="form.due_date" type="date" placeholder="Due date" />
      <UiInput v-model="form.effort" placeholder="Effort (e.g., 3d, 5h)" />
    </div>
  <UiInput v-model="form.subtitle" placeholder="Subtitle" />
    <textarea v-model="form.description" rows="5" placeholder="Description" />

    <div class="col" style="gap:6px;">
      <label class="muted">Tags</label>
      <div class="row" style="gap:8px; flex-wrap: wrap; align-items:flex-start;">
        <span class="chip" v-for="t in form.tags" :key="t">{{ t }} <button @click="removeTag(t)">×</button></span>
        <div class="col suggest-wrap">
          <input v-model="tagInput" @keydown.enter.prevent="onTagEnter" @focus="tagsOpen=true" placeholder="Add tag" class="tag-input input" />
          <ul v-if="tagsOpen && tagOpts.length" class="suggest">
            <li v-for="(t,i) in tagOpts" :key="t" :class="{ active: tagIdx===i }" @mouseenter="tagIdx=i" @mousedown.prevent="pickTag(t)">{{ t }}</li>
          </ul>
        </div>
        <UiButton size="sm" @click="addTag">Add</UiButton>
      </div>
    </div>

    <div v-if="mode==='edit'" class="col" style="gap:8px;">
      <label class="muted">Relationships</label>
      <div class="grid">
        <div class="col">
          <span class="muted small">Depends on</span>
          <input v-model="rels.depends_on" @keydown="onKey('depends_on', $event)" @input="onSuggest('depends_on')" placeholder="IDs comma separated" />
          <ul v-if="suggestions.depends_on.length" class="suggest">
            <li v-for="s in suggestions.depends_on" :key="s.id" @click="pick('depends_on', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Blocks</span>
          <input v-model="rels.blocks" @keydown="onKey('blocks', $event)" @input="onSuggest('blocks')" placeholder="IDs comma separated" />
          <ul v-if="suggestions.blocks.length" class="suggest">
            <li v-for="s in suggestions.blocks" :key="s.id" @click="pick('blocks', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Related</span>
          <input v-model="rels.related" @keydown="onKey('related', $event)" @input="onSuggest('related')" placeholder="IDs comma separated" />
          <ul v-if="suggestions.related.length" class="suggest">
            <li v-for="s in suggestions.related" :key="s.id" @click="pick('related', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Children</span>
          <input v-model="rels.children" @keydown="onKey('children', $event)" @input="onSuggest('children')" placeholder="IDs comma separated" />
          <ul v-if="suggestions.children.length" class="suggest">
            <li v-for="s in suggestions.children" :key="s.id" @click="pick('children', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Fixes</span>
          <input v-model="rels.fixes" @keydown="onKey('fixes', $event)" @input="onSuggest('fixes')" placeholder="IDs comma separated" />
          <ul v-if="suggestions.fixes.length" class="suggest">
            <li v-for="s in suggestions.fixes" :key="s.id" @click="pick('fixes', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Parent</span>
          <input v-model="rels.parent" @keydown="onKey('parent', $event)" @input="onSuggest('parent')" placeholder="ID" />
          <ul v-if="suggestions.parent.length" class="suggest">
            <li v-for="s in suggestions.parent" :key="s.id" @click="pick('parent', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
        <div class="col">
          <span class="muted small">Duplicate of</span>
          <input v-model="rels.duplicate_of" @keydown="onKey('duplicate_of', $event)" @input="onSuggest('duplicate_of')" placeholder="ID" />
          <ul v-if="suggestions.duplicate_of.length" class="suggest">
            <li v-for="s in suggestions.duplicate_of" :key="s.id" @click="pick('duplicate_of', s.id)">{{ s.id }} — {{ s.title }}</li>
          </ul>
        </div>
      </div>
    </div>

    <div class="col" style="gap:6px;">
      <label class="muted">Custom fields</label>
      <div v-for="(v,k) in localFields" :key="k" class="row" style="gap:8px; align-items:center;">
        <UiInput v-model="fieldKeysMap[k]" placeholder="Name" />
        <UiInput v-model="localFields[k]" :placeholder="`Value for ${fieldKeysMap[k] || k}`" style="flex:1;" />
        <UiButton size="sm" @click="removeField(k)">Remove</UiButton>
      </div>
      <div class="row" style="gap:8px; align-items:center;">
        <UiInput v-model="newFieldKey" placeholder="New field name" />
        <UiInput v-model="newFieldVal" placeholder="New field value" />
        <UiButton size="sm" @click="addField">Add field</UiButton>
      </div>
    </div>

    <div class="row" style="justify-content: end; gap:8px;">
      <UiButton @click="$emit('cancel')">Cancel</UiButton>
      <UiButton variant="primary" :disabled="!form.title.trim()" @click="emitSave">{{ mode==='create' ? 'Create' : 'Save' }}</UiButton>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { api } from '../api/client'
import type { ProjectDTO } from '../api/types'
import { fromDateInputValue, toDateInputValue } from '../utils/date'
import { formatProjectLabel } from '../utils/projectLabels'
import UiButton from './UiButton.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

type Mode = 'create' | 'edit'
const props = defineProps<{
  mode: Mode
  modelValue: any
  projects: ProjectDTO[]
  statuses: string[]
  priorities: string[]
  types: string[]
  // Optional: project prefix used for suggestions; fallback to form.project
  suggestProject?: string
}>()
const emit = defineEmits<{ (e:'update:modelValue', v:any): void; (e:'save', v:any): void; (e:'cancel'): void }>()

const form = reactive<any>({ ...props.modelValue })
form.due_date = toDateInputValue(form.due_date)
watch(
  () => props.modelValue,
  (value) => {
    const next = { ...(value || {}) }
    next.due_date = toDateInputValue((next as any).due_date)
    Object.assign(form, next)
  },
)
watch(
  form,
  () => {
    const payload = { ...form }
    payload.due_date = fromDateInputValue(form.due_date) ?? undefined
    emit('update:modelValue', payload)
  },
  { deep: true },
)

const projects = computed(() => props.projects)
const statuses = computed(() => props.statuses)
const priorities = computed(() => props.priorities)
const types = computed(() => props.types)

// Tags chips
const tagInput = ref('')
function addTag(){ const t = tagInput.value.trim(); if (!t) return; form.tags ||= []; if (!form.tags.includes(t)) form.tags.push(t); tagInput.value = '' }
function removeTag(t: string){ form.tags = (form.tags || []).filter((x: string) => x !== t) }

// Custom fields (string values for simplicity)
const localFields = reactive<Record<string, string>>({})
const fieldKeysMap = reactive<Record<string,string>>({})
const newFieldKey = ref('')
const newFieldVal = ref('')
function normalizeFieldKey(key: string) {
  return (key || '').trim()
}
function loadFields(){
  localFieldsClear()
  const src = form.custom_fields || {}
  for (const rawKey of Object.keys(src)) {
    const targetKey = normalizeFieldKey(rawKey)
    if (!targetKey) continue
    fieldKeysMap[targetKey] = targetKey
    localFields[targetKey] = String((src as any)[rawKey])
  }
}
function localFieldsClear(){ for (const k of Object.keys(localFields)) delete localFields[k]; for (const k of Object.keys(fieldKeysMap)) delete fieldKeysMap[k] }
function addField(){ const k = newFieldKey.value.trim(); if (!k) return; if (localFields[k] !== undefined) return; localFields[k] = newFieldVal.value; fieldKeysMap[k] = k; newFieldKey.value = ''; newFieldVal.value = '' }
function removeField(k: string){ delete localFields[k]; delete fieldKeysMap[k] }
function exportFields(): Record<string,string> {
  // Normalize renamed keys
  const out: Record<string,string> = {}
  for (const oldKey of Object.keys(localFields)) {
    const newKey = normalizeFieldKey(fieldKeysMap[oldKey] || oldKey)
    if (!newKey) continue
    out[newKey] = localFields[oldKey]
  }
  return out
}
loadFields()

// Preload configured custom fields (if present in config) for create mode
const configuredFieldNames = ref<string[]>([])
async function preloadFields() {
  try {
    const proj = (form.project || (form.id ? String(form.id).split('-')[0] : '')) || undefined
    const cfg = await api.showConfig(proj)
    const names = Array.isArray(cfg?.custom_fields) ? cfg.custom_fields : []
    const normalized = new Set<string>()
    names.forEach((x: any) => {
      const key = normalizeFieldKey(String(x))
      if (key) normalized.add(key)
    })
    configuredFieldNames.value = Array.from(normalized)
    for (const n of configuredFieldNames.value) { if (localFields[n] === undefined) { localFields[n] = ''; fieldKeysMap[n] = n } }
  } catch {}
}
preloadFields()

// Relationships with suggestions (comma-separated)
const rels = reactive<Record<string,string>>({ depends_on: '', blocks: '', related: '', children: '', fixes: '', parent: '', duplicate_of: '' })
function loadRels(){
  const r = form.relationships || {}
  rels.depends_on = (r.depends_on || []).join(', ')
  rels.blocks = (r.blocks || []).join(', ')
  rels.related = (r.related || []).join(', ')
  rels.children = (r.children || []).join(', ')
  rels.fixes = (r.fixes || []).join(', ')
  rels.parent = r.parent || ''
  rels.duplicate_of = r.duplicate_of || ''
}
loadRels()
function exportRels(){
  return {
    depends_on: rels.depends_on.split(',').map(s => s.trim()).filter(Boolean),
    blocks: rels.blocks.split(',').map(s => s.trim()).filter(Boolean),
    related: rels.related.split(',').map(s => s.trim()).filter(Boolean),
    children: rels.children.split(',').map(s => s.trim()).filter(Boolean),
    fixes: rels.fixes.split(',').map(s => s.trim()).filter(Boolean),
    parent: rels.parent.trim() || undefined,
    duplicate_of: rels.duplicate_of.trim() || undefined,
  }
}

const suggestions = ref<Record<string, Array<{ id: string; title: string }>>>({ depends_on: [], blocks: [], related: [], children: [], fixes: [], parent: [], duplicate_of: [] })
let timer: any = null
function lastToken(s: string){ const parts = s.split(','); return parts[parts.length-1].trim() }
async function onSuggest(field: string){
  const proj = props.suggestProject || form.project || (form.id ? String(form.id).split('-')[0] : '')
  const q = lastToken((rels as any)[field] || '')
  if (timer) clearTimeout(timer)
  timer = setTimeout(async () => {
    if (!q || q.length < 2) { (suggestions.value as any)[field] = []; return }
    try { (suggestions.value as any)[field] = await api.suggestTasks(q, proj) } catch { (suggestions.value as any)[field] = [] }
  }, 150)
}
function pick(field: string, id: string){
  if (field === 'parent' || field === 'duplicate_of') { (rels as any)[field] = id } else {
    const current = (rels as any)[field] as string
    const list = current ? current.split(',').map(s => s.trim()).filter(Boolean) : []
    if (!list.includes(id)) list.push(id)
    ;(rels as any)[field] = list.join(', ')
  }
  ;(suggestions.value as any)[field] = []
}
function onKey(field: string, e: KeyboardEvent){ if (e.key === 'Enter') { const list = (suggestions.value as any)[field] as any[]; if (list?.length) pick(field, list[0].id) } }

function emitSave(){
  const payload: any = { ...form }
  payload.custom_fields = exportFields()
  if (props.mode === 'edit') payload.relationships = exportRels()
  emit('save', payload)
}

// Light autocomplete for assignee/reporter: @me and whoami
const whoami = ref('')
api.whoami().then(v => whoami.value = v || '')
watch(() => form.assignee, (v) => { if (v === '@me' && whoami.value) form.assignee = whoami.value })
watch(() => form.reporter, (v) => { if (v === '@me' && whoami.value) form.reporter = whoami.value })

// Client-side suggestions for users, tags, and categories
const allUsers = ref<string[]>([])
const allTags = ref<string[]>([])
async function preloadLists(){
  try {
    // Use current filter: for now, list tasks in the selected project to gather unique values
    const proj = form.project || (form.id ? String(form.id).split('-')[0] : undefined)
    const list = await api.listTasks({ project: proj } as any)
    const users = new Set<string>()
    const tags = new Set<string>()
    for (const t of list) {
      if (t.assignee) users.add(t.assignee)
      if ((t as any).reporter) users.add((t as any).reporter)
      ;(t.tags || []).forEach(x => tags.add(x))
    }
    allUsers.value = Array.from(users).sort((a,b) => a.localeCompare(b))
    allTags.value = Array.from(tags).sort((a,b) => a.localeCompare(b))
  } catch {}
}
preloadLists()
watch(() => form.project, () => preloadLists())

// Users autocomplete state
const reporterOpen = ref(false)
const reporterIdx = ref(0)
const assigneeOpen = ref(false)
const assigneeIdx = ref(0)
const reporterOpts = computed(() => suggestFrom(allUsers.value, form.reporter))
const assigneeOpts = computed(() => suggestFrom(allUsers.value, form.assignee))
function suggestFrom(list: string[], q?: string){ const s = (q || '').toLowerCase(); return s ? list.filter(u => u.toLowerCase().includes(s)) : list.slice(0, 8) }
function onUserKey(which: 'reporter'|'assignee', e: KeyboardEvent){
  const open = which === 'reporter' ? reporterOpen : assigneeOpen
  const idx = which === 'reporter' ? reporterIdx : assigneeIdx
  const opts = which === 'reporter' ? reporterOpts.value : assigneeOpts.value
  if (!open.value) open.value = true
  if (e.key === 'ArrowDown') { e.preventDefault(); idx.value = (idx.value + 1) % Math.max(opts.length, 1) }
  else if (e.key === 'ArrowUp') { e.preventDefault(); idx.value = (idx.value - 1 + Math.max(opts.length, 1)) % Math.max(opts.length, 1) }
  else if (e.key === 'Enter' && opts.length) { e.preventDefault(); pickUser(which, opts[idx.value] || opts[0]) }
  else if (e.key === 'Escape') { open.value = false }
}
function pickUser(which: 'reporter'|'assignee', u: string){ if (which === 'reporter') { form.reporter = u; reporterOpen.value = false } else { form.assignee = u; assigneeOpen.value = false } }

// Tags autocomplete
const tagsOpen = ref(false)
const tagIdx = ref(0)
const tagOpts = computed(() => suggestFrom(allTags.value.filter(t => !(form.tags || []).includes(t)), tagInput.value))
function onTagEnter(){ if (tagOpts.value.length) { pickTag(tagOpts.value[tagIdx.value] || tagOpts.value[0]) } else { addTag() } }
function pickTag(t: string){ if (!form.tags) form.tags = []; if (!form.tags.includes(t)) form.tags.push(t); tagInput.value = ''; tagsOpen.value = false }

</script>

<style scoped>
.task-editor textarea { width: 100%; font: inherit; padding: 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); }
.chip { background: var(--surface-2); border: 1px solid var(--border); padding: 2px 6px; border-radius: 10px; }
.chip button { margin-left: 6px; }
.tag-input { min-width: 120px; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 8px; }
.suggest { list-style: none; padding: 0; margin: 6px 0; border: 1px solid var(--border); border-radius: 6px; max-height: 160px; overflow: auto; }
.suggest li { padding: 6px 8px; cursor: pointer; }
.suggest li:hover { background: var(--surface-2); }
.muted.small { font-size: 12px; }
</style>
