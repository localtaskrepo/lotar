import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import type { TaskDTO, TaskListFilter } from '../api/types'

// ---------------------------------------------------------------------------
// Mock the API client
// ---------------------------------------------------------------------------
const mockClient = {
  listTasks: vi.fn(),
  getTask: vi.fn(),
  addTask: vi.fn(),
  updateTask: vi.fn(),
  deleteTask: vi.fn(),
}

vi.mock('../api/client', () => ({
  api: mockClient,
}))

// Mock useSse so we can simulate events
const mockSseHandlers = new Map<string, (ev: MessageEvent) => void>()
const mockSseClose = vi.fn()
vi.mock('../composables/useSse', () => ({
  useSse: vi.fn(() => ({
    es: {},
    on(event: string, handler: (e: MessageEvent) => void) {
      mockSseHandlers.set(event, handler)
    },
    off(event: string, _handler: (e: MessageEvent) => void) {
      mockSseHandlers.delete(event)
    },
    close: mockSseClose,
  })),
}))

function fireEvent(kind: string, data: Record<string, unknown> | TaskDTO) {
  const handler = mockSseHandlers.get(kind)
  if (handler) {
    handler(new MessageEvent(kind, { data: JSON.stringify(data) }))
  }
}

function makeTask(id: string, overrides: Partial<TaskDTO> = {}): TaskDTO {
  return {
    id,
    title: `Task ${id}`,
    status: 'Todo' as any,
    priority: 'Medium' as any,
    task_type: 'Task' as any,
    created: '2025-01-01T00:00:00Z',
    modified: '2025-01-01T00:00:00Z',
    tags: [],
    relationships: {} as any,
    comments: [],
    references: [],
    sprints: [],
    history: [],
    custom_fields: {},
    ...overrides,
  }
}

describe('TaskStore', () => {
  let store: Awaited<ReturnType<typeof freshStore>>

  async function freshStore() {
    const mod = await import('../composables/useTaskStore')
    mod._resetTaskStore()
    return mod._createTestTaskStore(mockClient as any)
  }

  beforeEach(async () => {
    vi.clearAllMocks()
    vi.useFakeTimers()
    mockSseHandlers.clear()
    mockSseClose.mockClear()
    store = await freshStore()
  })

  afterEach(() => {
    vi.useRealTimers()
    store.disconnectSse()
  })

  // =========================================================================
  // Hydration
  // =========================================================================

  describe('hydrateAll', () => {
    it('loads all pages into the map', async () => {
      const t1 = makeTask('P-1')
      const t2 = makeTask('P-2')
      const t3 = makeTask('P-3')
      mockClient.listTasks
        .mockResolvedValueOnce({ total: 3, limit: 2, offset: 0, tasks: [t1, t2] })
        .mockResolvedValueOnce({ total: 3, limit: 2, offset: 2, tasks: [t3] })

      await store.hydrateAll({}, { pageSize: 2 })

      expect(store.status.value).toBe('ready')
      expect(store.count.value).toBe(3)
      expect(store.serverTotal.value).toBe(3)
      expect(store.items.value.map((t) => t.id).sort()).toEqual(['P-1', 'P-2', 'P-3'])
      expect(store.lastSyncAt.value).toBeGreaterThan(0)
    })

    it('stops on empty batch', async () => {
      mockClient.listTasks.mockResolvedValueOnce({ total: 0, limit: 200, offset: 0, tasks: [] })

      await store.hydrateAll()

      expect(store.count.value).toBe(0)
      expect(store.status.value).toBe('ready')
      expect(mockClient.listTasks).toHaveBeenCalledTimes(1)
    })

    it('passes filter through to API', async () => {
      mockClient.listTasks.mockResolvedValue({ total: 0, limit: 200, offset: 0, tasks: [] })
      const filter: TaskListFilter = { project: 'ACME', status: ['Todo'] }

      await store.hydrateAll(filter)

      expect(mockClient.listTasks).toHaveBeenCalledWith(
        expect.objectContaining({ project: 'ACME', status: ['Todo'], limit: 200, offset: 0 }),
      )
    })

    it('clears store when clear option is set', async () => {
      store.upsert(makeTask('OLD-1'))
      expect(store.count.value).toBe(1)

      mockClient.listTasks.mockResolvedValueOnce({ total: 1, limit: 200, offset: 0, tasks: [makeTask('NEW-1')] })
      await store.hydrateAll({}, { clear: true })

      expect(store.count.value).toBe(1)
      expect(store.items.value[0].id).toBe('NEW-1')
    })

    it('sets error status on failure', async () => {
      mockClient.listTasks.mockRejectedValue(new Error('Network down'))

      await store.hydrateAll()

      expect(store.status.value).toBe('error')
      expect(store.error.value).toBe('Network down')
    })

    it('merges into existing data without clear', async () => {
      store.upsert(makeTask('KEEP-1'))
      mockClient.listTasks.mockResolvedValueOnce({
        total: 1, limit: 200, offset: 0,
        tasks: [makeTask('NEW-1')],
      })

      await store.hydrateAll()

      expect(store.count.value).toBe(2)
      expect(store._map.value.has('KEEP-1')).toBe(true)
      expect(store._map.value.has('NEW-1')).toBe(true)
    })
  })

  // =========================================================================
  // hydratePage
  // =========================================================================

  describe('hydratePage', () => {
    it('adds a single page of results to the store', async () => {
      mockClient.listTasks.mockResolvedValueOnce({
        total: 50, limit: 20, offset: 0,
        tasks: [makeTask('A-1'), makeTask('A-2')],
      })

      const result = await store.hydratePage({ limit: 20, offset: 0 } as any)

      expect(result.total).toBe(50)
      expect(store.count.value).toBe(2)
      expect(store.serverTotal.value).toBe(50)
    })
  })

  // =========================================================================
  // fetchOne
  // =========================================================================

  describe('fetchOne', () => {
    it('fetches and upserts a single task', async () => {
      const task = makeTask('P-42', { title: 'Fetched' })
      mockClient.getTask.mockResolvedValue(task)

      const result = await store.fetchOne('P-42')

      expect(result).toEqual(task)
      expect(store._map.value.get('P-42')?.title).toBe('Fetched')
    })

    it('evicts task if fetch fails (task deleted between event and fetch)', async () => {
      store.upsert(makeTask('P-99'))
      mockClient.getTask.mockRejectedValue(new Error('Not found'))

      const result = await store.fetchOne('P-99')

      expect(result).toBeNull()
      expect(store._map.value.has('P-99')).toBe(false)
    })
  })

  // =========================================================================
  // forceRefresh
  // =========================================================================

  describe('forceRefresh', () => {
    it('clears store and rehydrates', async () => {
      store.upsert(makeTask('OLD-1'))
      mockClient.listTasks.mockResolvedValueOnce({
        total: 1, limit: 200, offset: 0,
        tasks: [makeTask('FRESH-1')],
      })

      await store.forceRefresh()

      expect(store.count.value).toBe(1)
      expect(store._map.value.has('OLD-1')).toBe(false)
      expect(store._map.value.has('FRESH-1')).toBe(true)
    })
  })

  // =========================================================================
  // Mutations
  // =========================================================================

  describe('mutations', () => {
    it('add() creates via API and inserts into store', async () => {
      const task = makeTask('NEW-1')
      mockClient.addTask.mockResolvedValue(task)

      const result = await store.add({ title: 'New' } as any)

      expect(result).toEqual(task)
      expect(store._map.value.has('NEW-1')).toBe(true)
    })

    it('update() patches via API and updates store', async () => {
      store.upsert(makeTask('P-1', { title: 'Old' }))
      const updated = makeTask('P-1', { title: 'Updated' })
      mockClient.updateTask.mockResolvedValue(updated)

      const result = await store.update('P-1', { title: 'Updated' } as any)

      expect(result.title).toBe('Updated')
      expect(store._map.value.get('P-1')?.title).toBe('Updated')
    })

    it('remove() deletes via API and evicts from store', async () => {
      store.upsert(makeTask('P-1'))
      mockClient.deleteTask.mockResolvedValue(undefined)

      await store.remove('P-1')

      expect(store._map.value.has('P-1')).toBe(false)
    })

    it('upsert() adds without API call', () => {
      store.upsert(makeTask('LOCAL-1'))
      expect(store._map.value.has('LOCAL-1')).toBe(true)
    })

    it('evict() removes without API call', () => {
      store.upsert(makeTask('P-1'))
      store.evict('P-1')
      expect(store._map.value.has('P-1')).toBe(false)
    })

    it('evict() is a no-op for unknown IDs', () => {
      const v = store.version.value
      store.evict('UNKNOWN')
      expect(store.version.value).toBe(v)
    })
  })

  // =========================================================================
  // Version counter
  // =========================================================================

  describe('version counter', () => {
    it('increments on upsert', () => {
      const v = store.version.value
      store.upsert(makeTask('P-1'))
      expect(store.version.value).toBe(v + 1)
    })

    it('increments on evict', () => {
      store.upsert(makeTask('P-1'))
      const v = store.version.value
      store.evict('P-1')
      expect(store.version.value).toBe(v + 1)
    })

    it('items computed re-evaluates when version bumps', async () => {
      expect(store.items.value.length).toBe(0)
      store.upsert(makeTask('P-1'))
      await nextTick()
      expect(store.items.value.length).toBe(1)
    })
  })

  // =========================================================================
  // SSE Integration
  // =========================================================================

  describe('SSE events', () => {
    beforeEach(() => {
      store.connectSse()
    })

    afterEach(() => {
      store.disconnectSse()
    })

    it('registers handlers for task events on connect', () => {
      expect(mockSseHandlers.has('task_created')).toBe(true)
      expect(mockSseHandlers.has('task_updated')).toBe(true)
      expect(mockSseHandlers.has('task_deleted')).toBe(true)
    })

    it('task_created with full DTO inserts into store', async () => {
      const task = makeTask('SSE-1', { title: 'Via SSE' })
      fireEvent('task_created', task)
      await nextTick()

      expect(store._map.value.has('SSE-1')).toBe(true)
      expect(store._map.value.get('SSE-1')?.title).toBe('Via SSE')
    })

    it('task_updated with full DTO (API-triggered) upserts without fetch', async () => {
      store.upsert(makeTask('P-1', { title: 'Old' }))
      fireEvent('task_updated', makeTask('P-1', { title: 'New via SSE' }))
      await nextTick()

      expect(store._map.value.get('P-1')?.title).toBe('New via SSE')
      expect(mockClient.getTask).not.toHaveBeenCalled()
    })

    it('task_updated with only ID (fswatcher) fetches single task', async () => {
      const fetched = makeTask('P-1', { title: 'From server' })
      mockClient.getTask.mockResolvedValue(fetched)

      store.upsert(makeTask('P-1', { title: 'Stale' }))
      fireEvent('task_updated', { id: 'P-1' })

      // Debounce: 150ms
      expect(mockClient.getTask).not.toHaveBeenCalled()
      vi.advanceTimersByTime(200)
      await vi.waitFor(() => expect(mockClient.getTask).toHaveBeenCalledWith('P-1'))
      expect(store._map.value.get('P-1')?.title).toBe('From server')
    })

    it('task_updated debounces rapid fswatcher events for the same ID', async () => {
      mockClient.getTask.mockResolvedValue(makeTask('P-1'))

      fireEvent('task_updated', { id: 'P-1' })
      vi.advanceTimersByTime(50)
      fireEvent('task_updated', { id: 'P-1' })
      vi.advanceTimersByTime(50)
      fireEvent('task_updated', { id: 'P-1' })
      vi.advanceTimersByTime(200)

      await vi.waitFor(() => expect(mockClient.getTask).toHaveBeenCalledTimes(1))
    })

    it('task_deleted evicts from store', async () => {
      store.upsert(makeTask('P-1'))
      fireEvent('task_deleted', { id: 'P-1' })
      await nextTick()

      expect(store._map.value.has('P-1')).toBe(false)
    })

    it('task_deleted cancels pending fswatcher fetch for same ID', async () => {
      mockClient.getTask.mockResolvedValue(makeTask('P-1'))

      fireEvent('task_updated', { id: 'P-1' })
      vi.advanceTimersByTime(50)
      fireEvent('task_deleted', { id: 'P-1' })
      vi.advanceTimersByTime(200)

      expect(mockClient.getTask).not.toHaveBeenCalled()
      expect(store._map.value.has('P-1')).toBe(false)
    })

    it('disconnectSse cleans up handlers and closes connection', () => {
      store.disconnectSse()

      expect(mockSseClose).toHaveBeenCalled()
      expect(store.sseConnected.value).toBe(false)
      expect(mockSseHandlers.size).toBe(0)
    })

    it('registers handler for task_error events on connect', () => {
      expect(mockSseHandlers.has('task_error')).toBe(true)
    })

    it('task_error invokes registered onTaskError callbacks', () => {
      const spy = vi.fn()
      store.onTaskError(spy)

      fireEvent('task_error', { id: 'P-1', message: 'bad yaml' })

      expect(spy).toHaveBeenCalledWith({ id: 'P-1', message: 'bad yaml' })
    })

    it('onTaskError unsubscribe stops further callbacks', () => {
      const spy = vi.fn()
      const unsub = store.onTaskError(spy)

      fireEvent('task_error', { id: 'P-1', message: 'first' })
      expect(spy).toHaveBeenCalledTimes(1)

      unsub()
      fireEvent('task_error', { id: 'P-2', message: 'second' })
      expect(spy).toHaveBeenCalledTimes(1)
    })

    it('task_error supports multiple listeners', () => {
      const spy1 = vi.fn()
      const spy2 = vi.fn()
      store.onTaskError(spy1)
      store.onTaskError(spy2)

      fireEvent('task_error', { id: 'X-1', message: 'oops' })

      expect(spy1).toHaveBeenCalledWith({ id: 'X-1', message: 'oops' })
      expect(spy2).toHaveBeenCalledWith({ id: 'X-1', message: 'oops' })
    })

    it('task_error ignores malformed payloads', () => {
      const spy = vi.fn()
      store.onTaskError(spy)

      // Missing message field
      fireEvent('task_error', { id: 'P-1' })
      expect(spy).not.toHaveBeenCalled()

      // Missing id field
      fireEvent('task_error', { message: 'oops' } as any)
      expect(spy).not.toHaveBeenCalled()
    })
  })

  // =========================================================================
  // Singleton behavior
  // =========================================================================

  describe('singleton', () => {
    it('useTaskStore returns the same instance', async () => {
      const mod = await import('../composables/useTaskStore')
      mod._resetTaskStore()
      const a = mod.useTaskStore()
      const b = mod.useTaskStore()
      expect(a).toBe(b)
    })

    it('_resetTaskStore creates a fresh instance', async () => {
      const mod = await import('../composables/useTaskStore')
      const a = mod.useTaskStore()
      a.upsert(makeTask('P-1'))
      mod._resetTaskStore()
      const b = mod.useTaskStore()
      expect(b.count.value).toBe(0)
    })
  })
})
