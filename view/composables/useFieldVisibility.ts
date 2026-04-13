import { computed, ref, watch, type Ref } from 'vue';

export type FieldOption = { key: string; label: string }
export type FieldSettings = Record<string, boolean>

const BUILTIN_FIELDS: FieldOption[] = [
  { key: 'id', label: 'ID' },
  { key: 'title', label: 'Title' },
  { key: 'status', label: 'Status' },
  { key: 'priority', label: 'Priority' },
  { key: 'task_type', label: 'Type' },
  { key: 'effort', label: 'Effort' },
  { key: 'reporter', label: 'Reporter' },
  { key: 'assignee', label: 'Assignee' },
  { key: 'tags', label: 'Tags' },
  { key: 'sprints', label: 'Sprints' },
  { key: 'due_date', label: 'Due' },
  { key: 'modified', label: 'Updated' },
]

export function useFieldVisibility(
  storagePrefix: string,
  projectRef: Ref<string>,
  defaultFields: FieldSettings,
  customFieldNames: Ref<string[]>,
) {
  const fields = ref<FieldSettings>({ ...defaultFields })

  function storageKey() {
    return projectRef.value ? `${storagePrefix}::${projectRef.value}` : storagePrefix
  }

  const fieldOptions = computed<FieldOption[]>(() => {
    const customs = (customFieldNames.value || [])
      .filter((name) => name !== '*' && !BUILTIN_FIELDS.some((b) => b.key === name))
      .map((name) => ({ key: name, label: name }))
    return [...BUILTIN_FIELDS, ...customs]
  })

  function load() {
    try {
      const raw = localStorage.getItem(storageKey())
      if (!raw) {
        fields.value = { ...defaultFields }
        return
      }
      const parsed = JSON.parse(raw)
      if (!parsed || typeof parsed !== 'object') {
        fields.value = { ...defaultFields }
        return
      }
      const next: FieldSettings = { ...defaultFields }

      // Migrate old key (kept for compatibility): due -> due_date
      const legacyDue = (parsed as any).due
      if (typeof legacyDue === 'boolean' && typeof parsed.due_date !== 'boolean') {
        next.due_date = legacyDue
      }

      for (const { key } of fieldOptions.value) {
        const v = parsed[key]
        if (typeof v === 'boolean') {
          next[key] = v
        }
      }
      fields.value = next
    } catch {
      fields.value = { ...defaultFields }
    }
  }

  function save() {
    try {
      localStorage.setItem(storageKey(), JSON.stringify(fields.value))
    } catch {}
  }

  function reset() {
    fields.value = { ...defaultFields }
  }

  function isVisible(key: string): boolean {
    return fields.value[key] !== false
  }

  function setVisible(key: string, ev: Event) {
    const checked = Boolean((ev.target as HTMLInputElement | null)?.checked)
    fields.value = { ...fields.value, [key]: checked }
  }

  watch(fields, () => save(), { deep: true })

  return { fields, fieldOptions, load, reset, isVisible, setVisible }
}
