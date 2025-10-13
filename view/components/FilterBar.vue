<template>
  <div class="row" style="flex-wrap: wrap; gap:8px;">
    <UiInput v-model="query" placeholder="Search…" />
    <UiSelect v-model="project">
      <option value="">Project</option>
      <option v-for="p in projects" :key="p.prefix" :value="p.prefix">{{ p.name }}</option>
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
  <UiButton @click="onClear">Clear</UiButton>
  </div>
</template>
<script setup lang="ts">
import { onMounted, ref, watch, watchEffect } from 'vue'
import { useProjects } from '../composables/useProjects'
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
  }
})

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
  try { sessionStorage.removeItem(FILTER_KEY) } catch {}
  const empty: Record<string,string> = { order: 'desc' }
  emit('update:value', empty)
}

// Emit whenever any field changes; parent debounces/refetches
watch([query, project, status, priority, type, order, mine, tags], emitFilter, { deep: false })
</script>
<style scoped></style>
