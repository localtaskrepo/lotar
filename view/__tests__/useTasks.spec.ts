import { describe, expect, it, vi } from 'vitest'
import { createUseTasks } from '../composables/useTasks'

describe('useTasks', () => {
  const fake = {
    listTasks: vi.fn(async () => ({
      total: 1,
      limit: 50,
      offset: 0,
      tasks: [{ id: 'PRJ-1', title: 'A', status: 'open', priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {} }],
    })),
    addTask: vi.fn(async (p) => ({ id: 'PRJ-2', title: p.title, status: 'open', priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {} })),
    updateTask: vi.fn(async (id, patch) => ({ id, title: patch.title || 'x', status: 'open', priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {} })),
    deleteTask: vi.fn(async () => ({ deleted: true })),
  } as any

  it('refresh, add, update, remove', async () => {
    const { items, refresh, add, update, remove, count, status } = createUseTasks(fake)
    await refresh()
    expect(status.value).toBe('ready')
    expect(items.value.length).toBe(1)
    await add({ title: 'B' })
    expect(items.value.length).toBe(2)
    await update('PRJ-2', { title: 'C' })
    expect(items.value.find(t => t.id === 'PRJ-2')?.title).toBe('C')
    await remove('PRJ-2')
    expect(items.value.find(t => t.id === 'PRJ-2')).toBeFalsy()
    expect(count.value).toBe(1)
  })
})
