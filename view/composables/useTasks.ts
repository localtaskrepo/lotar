import { computed, ref, type Ref } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { TaskCreate, TaskDTO, TaskListFilter, TaskUpdate } from '../api/types'
import { createResource } from './useResource'

export function createUseTasks(client: ApiClient) {
  const total = ref(0)
  const limit = ref(0)
  const offset = ref(0)

  const listResource = createResource<TaskDTO[], [TaskListFilter?]>(
    async (filter: TaskListFilter = {}) => {
      const response = await client.listTasks(filter)
      total.value = response.total ?? 0
      limit.value = response.limit ?? 0
      offset.value = response.offset ?? 0
      return Array.isArray(response.tasks) ? response.tasks : []
    },
    { initialValue: [] as TaskDTO[] },
  )

  const items = listResource.data as Ref<TaskDTO[]>
  const loading = listResource.loading
  const error = computed(() => listResource.error.value?.message ?? null)
  const count = computed(() => items.value?.length ?? 0)

  async function refresh(filter: TaskListFilter = {}) {
    await listResource.refresh(filter)
  }

  async function refreshAll(filter: TaskListFilter = {}, pageSize = 200) {
    const collected: TaskDTO[] = []
    let currentOffset = 0
    let expectedTotal = 0
    let pages = 0

    while (pages < 10_000) {
      pages += 1
      const response = await client.listTasks({ ...filter, limit: pageSize, offset: currentOffset } as any)
      const batch = Array.isArray(response.tasks) ? response.tasks : []
      expectedTotal = response.total ?? expectedTotal
      if (batch.length === 0) {
        break
      }
      collected.push(...batch)
      currentOffset += batch.length
      if (expectedTotal && collected.length >= expectedTotal) {
        break
      }
    }

    total.value = expectedTotal || collected.length
    limit.value = collected.length
    offset.value = 0
    listResource.set(collected)
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
    total,
    limit,
    offset,
    status: listResource.status,
    ready: listResource.ready,
    refresh,
    refreshAll,
    add,
    update,
    remove,
  }
}

export const useTasks = () => createUseTasks(api)
