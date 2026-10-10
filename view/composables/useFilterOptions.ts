import { computed, shallowRef, watch } from 'vue'
import type { TaskDTO } from '../api/types'

interface KnownOptions {
  tags: string[]
  assignees: string[]
  customFieldValues: Record<string, string[]>
}

export function useFilterOptions(
  tasks: () => TaskDTO[],
  project: () => string,
  config: { scope: () => string; tags: () => string[]; members: () => string[] },
) {
  const known = shallowRef(new Map<string, KnownOptions>())
  const unique = (values: string[]) => [...new Set(values.map(value => value.trim()).filter(value => value && value !== '*'))].sort((a, b) => a.localeCompare(b))

  // Retain values observed before a filter hid their rows, but never borrow
  // entities or option lists from another project scope.
  watch(() => ({ scope: project(), tasks: tasks().map(task => ({ id: task.id, tags: [...(task.tags || [])], assignee: task.assignee, fields: { ...task.custom_fields } })) }), ({ scope, tasks }) => {
    const previous = known.value.get(scope)
    const tags = [...(previous?.tags ?? [])]
    const assignees = [...(previous?.assignees ?? [])]
    const fields = new Map(Object.entries(previous?.customFieldValues ?? {}).map(([key, values]) => [key, [...values]]))
    for (const task of tasks) {
      if (scope && task.id.slice(0, task.id.lastIndexOf('-')) !== scope) continue
      tags.push(...task.tags)
      if (task.assignee) assignees.push(task.assignee)
      for (const [key, value] of Object.entries(task.fields)) {
        if (!['string', 'number', 'boolean'].includes(typeof value) || value === '') continue
        const values = fields.get(key) ?? []
        values.push(String(value))
        fields.set(key, values)
      }
    }
    known.value = new Map(known.value).set(scope, {
      tags: unique(tags), assignees: unique(assignees),
      customFieldValues: Object.fromEntries([...fields].map(([key, values]) => [key, [...new Set(values)].filter(value => value.length > 0).sort((a, b) => a.localeCompare(b))])),
    })
  }, { immediate: true, flush: 'sync' })

  return computed(() => {
    const scope = project()
    const current = known.value.get(scope)
    const configured = config.scope() === scope
    return {
      tags: unique([...(configured ? config.tags() : []), ...(current?.tags ?? [])]),
      assignees: unique([...(configured ? config.members() : []), ...(current?.assignees ?? [])]),
      customFieldValues: current?.customFieldValues ?? {},
    }
  })
}
