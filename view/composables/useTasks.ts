import { computed, type Ref } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { TaskCreate, TaskDTO, TaskListFilter, TaskUpdate } from '../api/types'
import { createResource } from './useResource'

export function createUseTasks(client: ApiClient) {
  const listResource = createResource<TaskDTO[], [TaskListFilter?]>(
    async (filter: TaskListFilter = {}) => client.listTasks(filter),
    { initialValue: [] as TaskDTO[] },
  )

  const items = listResource.data as Ref<TaskDTO[]>
  const loading = listResource.loading
  const error = computed(() => listResource.error.value?.message ?? null)
  const count = computed(() => items.value?.length ?? 0)

  async function refresh(filter: TaskListFilter = {}) {
    await listResource.refresh(filter)
  }

  async function add(payload: TaskCreate) {
    const created = await client.addTask(payload)
    listResource.mutate((current) => [created, ...(current ?? [])])
    return created
  }

  async function update(id: string, patch: TaskUpdate) {
    const updated = await client.updateTask(id, patch)
    listResource.mutate((current) => {
      const next = [...(current ?? [])]
      const index = next.findIndex((item) => item.id === id)
      if (index >= 0) next[index] = updated
      return next
    })
    return updated
  }

  async function remove(id: string) {
    await client.deleteTask(id)
    listResource.mutate((current) => (current ?? []).filter((item) => item.id !== id))
  }

  return {
    items,
    loading,
    error,
    count,
    status: listResource.status,
    ready: listResource.ready,
    refresh,
    add,
    update,
    remove,
  }
}

export const useTasks = () => createUseTasks(api)
