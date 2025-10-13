<template>
  <fieldset class="task-panel__group">
    <legend>Summary</legend>
    <UiInput
      v-model="form.title"
      placeholder="Title"
      required
      aria-required="true"
      @blur="emitFieldBlur('title')"
    />
    <div class="task-panel__row">
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
      <UiSelect v-model="form.task_type" required aria-required="true" @change="emitFieldBlur('task_type')">
        <option value="" disabled>Type</option>
        <option v-for="t in typeOptions" :key="t" :value="t">{{ t }}</option>
      </UiSelect>
    </div>
    <div class="task-panel__row">
      <UiSelect v-model="form.status" required aria-required="true" @change="emitStatusChange">
        <option value="" disabled>Status</option>
        <option v-for="s in statusOptions" :key="s" :value="s">{{ s }}</option>
      </UiSelect>
      <UiSelect v-model="form.priority" required aria-required="true" @change="emitFieldBlur('priority')">
        <option value="" disabled>Priority</option>
        <option v-for="p in priorityOptions" :key="p" :value="p">{{ p }}</option>
      </UiSelect>
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
  })

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
