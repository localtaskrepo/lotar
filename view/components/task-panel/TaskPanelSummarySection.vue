<template>
  <!-- DEV-239: the large two-column view shows the title on its own (part
       "title") and the properties, labelled, in the right column ("fields"). -->
  <div v-if="part === 'title'" class="task-panel__title-field">
    <UiInput
      v-model="form.title"
      class="task-panel__title-input"
      placeholder="Title"
      aria-label="Title"
      required
      aria-required="true"
      @blur="emitFieldBlur('title')"
    />
  </div>
  <fieldset v-else class="task-panel__group" :class="{ 'task-panel__properties': labelled }">
    <legend>{{ labelled ? 'Properties' : 'Summary' }}</legend>
    <UiInput
      v-if="part === 'all'"
      v-model="form.title"
      placeholder="Title"
      required
      aria-required="true"
      @blur="emitFieldBlur('title')"
    />
    <div class="task-panel__row">
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Project</span>
        <UiSelect
          v-model="form.project"
          :disabled="mode === 'edit'"
          required
          aria-required="true"
          @change="emitProjectChange"
        >
          <option value="" disabled>Select project…</option>
          <option v-for="p in projectOptions" :key="p.prefix" :value="p.prefix">
            {{ projectLabel(p) }}
          </option>
        </UiSelect>
      </label>
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Type</span>
        <UiSelect v-model="form.task_type" required aria-required="true" @change="emitFieldBlur('task_type')">
          <option value="" disabled>Type</option>
          <option v-for="t in typeOptions" :key="t" :value="t">{{ t }}</option>
        </UiSelect>
      </label>
    </div>
    <div class="task-panel__row">
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Status</span>
        <UiSelect v-model="form.status" required aria-required="true" @change="emitStatusChange">
          <option value="" disabled>Status</option>
          <option v-for="s in statusOptions" :key="s" :value="s">{{ s }}</option>
        </UiSelect>
      </label>
      <label class="task-panel__field">
        <span v-if="labelled" class="task-panel__field-label">Priority</span>
        <UiSelect v-model="form.priority" required aria-required="true" @change="emitFieldBlur('priority')">
          <option value="" disabled>Priority</option>
          <option v-for="p in priorityOptions" :key="p" :value="p">{{ p }}</option>
        </UiSelect>
      </label>
    </div>
  </fieldset>
</template>

<script setup lang="ts">
import { computed, isRef, type PropType, type Ref } from 'vue'
import UiInput from '../UiInput.vue'
import UiSelect from '../UiSelect.vue'

interface ProjectOption {
  prefix: string
  name?: string | null
}

const props = defineProps({
  form: { type: Object as PropType<Record<string, any>>, required: true },
  mode: { type: String as PropType<'create' | 'edit'>, required: true },
  projects: {
    type: [Array, Object] as PropType<ProjectOption[] | Ref<ProjectOption[]>>,
    default: () => [],
  },
  projectLabel: { type: Function as PropType<(project: ProjectOption) => string>, required: true },
  types: {
    type: [Array, Object] as PropType<string[] | Ref<string[]>>,
    default: () => [],
  },
  statuses: {
    type: [Array, Object] as PropType<string[] | Ref<string[]>>,
    default: () => [],
  },
  priorities: {
    type: [Array, Object] as PropType<string[] | Ref<string[]>>,
    default: () => [],
  },
  /** `all` (side panel), or the large view's separate `title` and `fields` parts. */
  part: { type: String as PropType<'all' | 'title' | 'fields'>, default: 'all' },
})

const labelled = computed(() => props.part === 'fields')

const projectOptions = computed(() => toArray<ProjectOption>(props.projects))
const typeOptions = computed(() => toArray<string>(props.types))
const statusOptions = computed(() => toArray<string>(props.statuses))
const priorityOptions = computed(() => toArray<string>(props.priorities))

const emit = defineEmits<{
  (e: 'fieldBlur', field: string): void
  (e: 'projectChange'): void
  (e: 'updateStatus', status: string): void
}>()

function emitFieldBlur(field: string) {
  emit('fieldBlur', field)
}

function emitProjectChange() {
  emit('projectChange')
}

function emitStatusChange() {
  emit('updateStatus', props.form.status)
}

function toArray<T>(value: T[] | Ref<T[]> | undefined | null): T[] {
  if (isRef(value)) {
    const inner = value.value
    return Array.isArray(inner) ? inner : []
  }
  return Array.isArray(value) ? value : []
}
</script>
