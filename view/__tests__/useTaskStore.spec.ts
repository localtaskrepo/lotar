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
  restoreTask: vi.fn(),
}

vi.mock('../api/client', () => ({
  api: mockClient,
}))

// Mock useSse so we can simulate events and capture reconnect hooks
const mockSseHandlers = new Map<string, (ev: MessageEvent) => void>()
const mockSseClose = vi.fn()
const mockSseOptions: Array<{ onReconnect?: () => void }> = []
vi.mock('../composables/useSse', () => ({
  useSse: vi.fn(
    (_path: string, _params: Record<string, unknown>, opts?: { onReconnect?: () => void }) => {
      mockSseOptions.push(opts ?? {})
      return {
        es: {},
        on(event: string, handler: (e: MessageEvent) => void) {
          mockSseHandlers.set(event, handler)
        },
        off(event: string, _handler: (e: MessageEvent) => void) {
          mockSseHandlers.delete(event)
        },
        close: mockSseClose,
      }
    },
  ),
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
    mockSseOptions.length = 0
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
      expect(store.items.value[0]!.id).toBe('NEW-1')
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
  // Order ledger (DEV-57)
  // =========================================================================

  describe('order ledger', () => {
    it('records the server response order across a multi-page full hydrate', async () => {
      const t3 = makeTask('P-3')
      const t1 = makeTask('P-1')
      const t2 = makeTask('P-2')
      mockClient.listTasks
        .mockResolvedValueOnce({ total: 3, limit: 2, offset: 0, tasks: [t3, t1] })
        .mockResolvedValueOnce({ total: 3, limit: 2, offset: 2, tasks: [t2] })

      await store.hydrateAll({}, { pageSize: 2 })

      // Response order, NOT id or map order.
      expect([...store.orderIndex.value.entries()]).toEqual([
        ['P-3', 0],
        ['P-1', 1],
        ['P-2', 2],
      ])
    })

    it('rebuilds the ledger for a refiltered hydrate, dropping stale ranks', async () => {
      mockClient.listTasks.mockResolvedValueOnce({
        total: 2, limit: 200, offset: 0,
        tasks: [makeTask('A-1'), makeTask('A-2')],
      })
      await store.hydrateAll({ project: 'A' })

      mockClient.listTasks.mockResolvedValueOnce({
        total: 1, limit: 200, offset: 0,
        tasks: [makeTask('B-9')],
      })
      await store.hydrateAll({ project: 'B' })

      // Only the new hydration's tasks carry ranks.
      expect(store.orderIndex.value.has('A-1')).toBe(false)
      expect(store.orderIndex.value.has('A-2')).toBe(false)
      expect(store.orderIndex.value.get('B-9')).toBe(0)
    })

    it('keeps existing ranks stable when SSE updates or deletes tasks', async () => {
      mockClient.listTasks.mockResolvedValueOnce({
        total: 2, limit: 200, offset: 0,
        tasks: [makeTask('P-1'), makeTask('P-2')],
      })
      await store.hydrateAll()

      // An SSE update must not move the task in the authority order.
      const updated = makeTask('P-1', { title: 'renamed' })
      store.upsert(updated)
      expect(store.orderIndex.value.get('P-1')).toBe(0)
      expect(store.items.value.map((t) => t.id)).toEqual(['P-1', 'P-2'])

      // Deleting removes the map entry; remaining ranks stay untouched.
      mockClient.deleteTask.mockResolvedValueOnce({ ok: true } as any)
      await store.remove('P-2')
      expect(store.orderIndex.value.get('P-2')).toBe(1) // historical rank kept
      expect(store.items.value.map((t) => t.id)).toEqual(['P-1'])
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

    it('evicts + tombstones the task when the fetch fails with 404', async () => {
      store.upsert(makeTask('P-99'))
      const notFound = new Error('GET /api/tasks/get failed: Not found') as Error & { status?: number }
      notFound.status = 404
      mockClient.getTask.mockRejectedValue(notFound)

      const result = await store.fetchOne('P-99')

      expect(result).toBeNull()
      expect(store._map.value.has('P-99')).toBe(false)
      expect(store.fetchOneError.value).toBeNull()
    })

    it('retains the task and surfaces the error on transient fetch failures (503)', async () => {
      store.upsert(makeTask('P-99'))
      const unavailable = new Error('GET /api/tasks/get failed: Service unavailable') as Error & { status?: number }
      unavailable.status = 503
      mockClient.getTask.mockRejectedValue(unavailable)

      const result = await store.fetchOne('P-99')

      expect(result).toBeNull()
      expect(store._map.value.has('P-99')).toBe(true)
      expect(store.fetchOneError.value).toContain('Service unavailable')
    })
  })

  // =========================================================================
  // forceRefresh
  // =========================================================================

  describe('forceRefresh', () => {
    it('replaces the query membership without clearing unrelated entities', async () => {
      store.upsert(makeTask('OLD-1'))
      mockClient.listTasks.mockResolvedValueOnce({
        total: 1, limit: 200, offset: 0,
        tasks: [makeTask('FRESH-1')],
      })

      await store.forceRefresh()

      expect(store.count.value).toBe(1)
      expect(store.items.value.map((t) => t.id)).toEqual(['FRESH-1'])
      // DEV-65: entities other queries may still present are retained.
      expect(store._map.value.has('OLD-1')).toBe(true)
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

    it('remove() soft-deletes via API: entity stays as trash, active view drops it', async () => {
      store.upsert(makeTask('P-1'))
      mockClient.deleteTask.mockResolvedValue({ deleted: true, hard: false, warnings: [] })

      const response = await store.remove('P-1')

      expect(response.deleted).toBe(true)
      // DEV-92: soft delete keeps the entity (with tombstone stamp) so the
      // Deleted view can render it; the active flat view hides it.
      expect(store._map.value.has('P-1')).toBe(true)
      expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy()
      expect(store.items.value.map((t) => t.id)).toEqual([])
    })

    it('remove({hard:true}) deletes via API and evicts from store', async () => {
      store.upsert(makeTask('P-1'))
      mockClient.deleteTask.mockResolvedValue({ deleted: true, hard: true, warnings: ['kept blob'] })

      const response = await store.remove('P-1', { hard: true })

      expect(response.warnings).toEqual(['kept blob'])
      expect(store._map.value.has('P-1')).toBe(false)
    })

    it('restore() restores via API and re-activates the entity', async () => {
      store.upsert(makeTask('P-1'))
      mockClient.deleteTask.mockResolvedValue({ deleted: true, hard: false, warnings: [] })
      await store.remove('P-1')

      const restored = makeTask('P-1')
      mockClient.restoreTask.mockResolvedValue(restored)
      const result = await store.restore('P-1')

      expect(result.id).toBe('P-1')
      expect(store._map.value.get('P-1')?.deleted_at).toBeUndefined()
      expect(store.items.value.map((t) => t.id)).toEqual(['P-1'])
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
      await vi.waitFor(() =>
        expect(mockClient.getTask).toHaveBeenCalledWith('P-1', undefined, { includeDeleted: false }),
      )
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

    it('task_deleted optimistically tombstones in-place and reconciles via the trash endpoint', async () => {
      store.upsert(makeTask('P-1'))
      fireEvent('task_deleted', { id: 'P-1' })
      await nextTick()

      // DEV-92: metadata-only event (soft AND hard deletes look the same):
      // the row leaves active queries immediately as an in-place tombstone…
      expect(store._map.value.has('P-1')).toBe(true)
      expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy()
      expect(store.items.value.map((t) => t.id)).toEqual([])

      // …then the debounced reconciliation fetch asks the trash endpoint.
      vi.advanceTimersByTime(200)
      await vi.waitFor(() =>
        expect(mockClient.getTask).toHaveBeenCalledWith('P-1', undefined, { includeDeleted: true }),
      )
    })

    it('task_deleted reconciliation confirms the server stamp for soft deletes', async () => {
      store.upsert(makeTask('P-1'))
      const serverStamp = '2026-10-05T12:00:00Z'
      mockClient.getTask.mockResolvedValue(makeTask('P-1', { deleted_at: serverStamp }))

      fireEvent('task_deleted', { id: 'P-1' })
      vi.advanceTimersByTime(200)
      await vi.waitFor(() => expect(store._map.value.get('P-1')?.deleted_at).toBe(serverStamp))
    })

    it('task_deleted reconciliation evicts when the trash endpoint also 404s (hard delete)', async () => {
      store.upsert(makeTask('P-1'))
      const notFound = new Error('GET /api/tasks/get failed: Not found') as Error & { status?: number }
      notFound.status = 404
      mockClient.getTask.mockRejectedValue(notFound)

      fireEvent('task_deleted', { id: 'P-1' })
      vi.advanceTimersByTime(200)
      await vi.waitFor(() => expect(store._map.value.has('P-1')).toBe(false))
    })

    it('task_deleted replaces the pending strict fetch with a trash reconciliation', async () => {
      mockClient.getTask.mockResolvedValue(makeTask('P-1', { deleted_at: '2026-10-05T12:00:00Z' }))

      fireEvent('task_updated', { id: 'P-1' })
      vi.advanceTimersByTime(50)
      fireEvent('task_deleted', { id: 'P-1' })
      vi.advanceTimersByTime(200)

      await vi.waitFor(() => {
        // Exactly one fetch, and it is the trash-aware reconciliation.
        expect(mockClient.getTask).toHaveBeenCalledTimes(1)
        expect(mockClient.getTask).toHaveBeenCalledWith('P-1', undefined, { includeDeleted: true })
      })
      await vi.waitFor(() => expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy())
    })

    it('task_updated full DTO carrying deleted_at marks the entity deleted (fs soft edit)', async () => {
      store.upsert(makeTask('P-1'))
      fireEvent('task_updated', makeTask('P-1', { deleted_at: '2026-10-05T13:00:00Z' }))
      await nextTick()

      expect(store._map.value.get('P-1')?.deleted_at).toBe('2026-10-05T13:00:00Z')
      expect(store.items.value.map((t) => t.id)).toEqual([])
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
  // DEV-65: entity/query separation and race guards
  // =========================================================================

  function deferred<T>() {
    let resolve!: (value: T) => void
    let reject!: (reason?: unknown) => void
    const promise = new Promise<T>((res, rej) => {
      resolve = res
      reject = rej
    })
    return { promise, resolve, reject }
  }

  function listResponse(tasks: TaskDTO[], total = tasks.length) {
    return { total, limit: 200, offset: 0, tasks }
  }

  describe('DEV-65 entity/query separation', () => {
    beforeEach(() => {
      store.connectSse()
    })

    afterEach(() => {
      store.disconnectSse()
    })

    it('a scoped (clear) hydrate replaces only its own query; other queries and entities survive', async () => {
      // FRESH query result (e.g. the row an SSE create made visible)
      const fresh = store.getQuery({ project: 'FRESH' })
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('FRESH-1')]))
      await fresh.refresh()

      // A BASE-scoped hydrateAll(clear: true) completing afterwards must not
      // evict the FRESH entity or membership (the original smoke race).
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('BASE-1')]))
      await store.hydrateAll({ project: 'BASE' }, { clear: true })

      expect(fresh.tasks.value.map((t) => t.id)).toEqual(['FRESH-1'])
      expect(store._map.value.has('FRESH-1')).toBe(true)
      expect(store.items.value.map((t) => t.id)).toEqual(['BASE-1'])
      expect(store.count.value).toBe(1)
    })

    it('an SSE-inserted foreign entity never joins query membership or inflates counts', async () => {
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('A-1')]))
      await store.hydrateAll({ project: 'A' }, { clear: true })

      fireEvent('task_created', makeTask('FOREIGN-1'))

      expect(store._map.value.has('FOREIGN-1')).toBe(true)
      expect(store.items.value.map((t) => t.id)).toEqual(['A-1'])
      expect(store.count.value).toBe(1)
      expect(store.serverTotal.value).toBe(1)
    })

    it('fetchOne/upsert/add never add query membership', async () => {
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('A-1')]))
      await store.hydrateAll({}, { clear: true })

      mockClient.getTask.mockResolvedValueOnce(makeTask('Z-9'))
      await store.fetchOne('Z-9')
      store.upsert(makeTask('PANEL-1'))
      mockClient.addTask.mockResolvedValueOnce(makeTask('ADDED-1'))
      await store.add({ title: 'x' } as any)

      expect(store._map.value.has('Z-9')).toBe(true)
      expect(store._map.value.has('PANEL-1')).toBe(true)
      expect(store._map.value.has('ADDED-1')).toBe(true)
      expect(store.items.value.map((t) => t.id)).toEqual(['A-1'])
    })

    it('a slow older query cannot publish over a newer one (late success)', async () => {
      const slowA = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => slowA.promise)
      const pendingA = store.hydrateAll({ project: 'A' }, { clear: true })

      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('B-1')]))
      await store.hydrateAll({ project: 'B' }, { clear: true })
      expect(store.items.value.map((t) => t.id)).toEqual(['B-1'])
      expect(store.status.value).toBe('ready')

      slowA.resolve(listResponse([makeTask('A-1')]))
      await pendingA

      // The legacy active view stays on B; A publishes only to its own entry.
      expect(store.items.value.map((t) => t.id)).toEqual(['B-1'])
      expect(store.status.value).toBe('ready')
      expect(store.error.value).toBeNull()
      const queryA = store.getQuery({ project: 'A' })
      expect(queryA.tasks.value.map((t) => t.id)).toEqual(['A-1'])
    })

    it('a slow older query failure cannot publish an error over a newer ready query', async () => {
      const slowA = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => slowA.promise)
      const pendingA = store.hydrateAll({ project: 'A' }, { clear: true })

      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('B-1')]))
      await store.hydrateAll({ project: 'B' }, { clear: true })

      slowA.reject(new Error('A failed late'))
      await pendingA

      expect(store.status.value).toBe('ready')
      expect(store.error.value).toBeNull()
      expect(store.items.value.map((t) => t.id)).toEqual(['B-1'])
    })

    it('a stale same-key refresh cannot overwrite a newer refresh (success and error)', async () => {
      const first = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => first.promise)
      const pendingFirst = store.hydrateAll({}, { clear: true })

      const second = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => second.promise)
      const pendingSecond = store.hydrateAll({}, { clear: true })

      second.resolve(listResponse([makeTask('NEW-1')]))
      await pendingSecond
      expect(store.items.value.map((t) => t.id)).toEqual(['NEW-1'])

      first.resolve(listResponse([makeTask('OLD-1')]))
      await pendingFirst
      expect(store.items.value.map((t) => t.id)).toEqual(['NEW-1'])

      const third = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => third.promise)
      const pendingThird = store.hydrateAll({}, { clear: true })
      const fourth = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => fourth.promise)
      const pendingFourth = store.hydrateAll({}, { clear: true })
      fourth.resolve(listResponse([makeTask('FINAL-1')]))
      await pendingFourth
      third.reject(new Error('stale failure'))
      await pendingThird

      expect(store.status.value).toBe('ready')
      expect(store.error.value).toBeNull()
      expect(store.items.value.map((t) => t.id)).toEqual(['FINAL-1'])
    })

    it('a failed NEW query shows no rows from the previous query and surfaces its error', async () => {
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('A-1')]))
      await store.hydrateAll({ project: 'A' }, { clear: true })
      expect(store.items.value.map((t) => t.id)).toEqual(['A-1'])

      mockClient.listTasks.mockRejectedValueOnce(new Error('B exploded'))
      await store.hydrateAll({ project: 'B' }, { clear: true })

      expect(store.items.value).toEqual([])
      expect(store.count.value).toBe(0)
      expect(store.status.value).toBe('error')
      expect(store.error.value).toBe('B exploded')

      // Retry succeeds and replaces membership.
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('B-1')]))
      await store.hydrateAll({ project: 'B' }, { clear: true })
      expect(store.items.value.map((t) => t.id)).toEqual(['B-1'])
      expect(store.status.value).toBe('ready')
    })

    it('a stale hydrate response cannot overwrite a newer SSE entity', async () => {
      mockClient.listTasks.mockResolvedValueOnce(
        listResponse([makeTask('T-1', { title: 'original', modified: '2025-01-01T00:00:00Z' })]),
      )
      await store.hydrateAll({}, { clear: true })

      const response = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => response.promise)
      const pending = store.hydrateAll({}, { clear: true })
      // A newer SSE DTO lands while the response is in flight.
      fireEvent('task_updated', makeTask('T-1', { title: 'SSE newer', modified: '2025-01-02T00:00:00Z' }))

      response.resolve(
        listResponse([makeTask('T-1', { title: 'server older', modified: '2025-01-01T00:00:00Z' })]),
      )
      await pending
      expect(store._map.value.get('T-1')?.title).toBe('SSE newer')
    })

    it('a stale fetchOne response cannot overwrite a newer SSE entity', async () => {
      store.upsert(makeTask('T-2', { title: 'old', modified: '2025-01-01T00:00:00Z' }))
      const response = deferred<TaskDTO>()
      mockClient.getTask.mockImplementationOnce(() => response.promise)
      const pending = store.fetchOne('T-2')
      fireEvent('task_updated', makeTask('T-2', { title: 'SSE newer', modified: '2025-01-02T00:00:00Z' }))

      response.resolve(makeTask('T-2', { title: 'server older', modified: '2025-01-01T00:00:00Z' }))
      await pending
      expect(store._map.value.get('T-2')?.title).toBe('SSE newer')
    })

    it('a task deleted during an in-flight hydrate is not resurrected by the stale response', async () => {
      const response = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => response.promise)
      const pending = store.hydrateAll({ project: 'A' }, { clear: true })

      fireEvent('task_deleted', { id: 'T-1' })

      response.resolve(listResponse([makeTask('T-1')]))
      await pending
      // DEV-92: soft-delete semantics — the entity stays as a trash stub
      // (deletion guard rejected the stale active DTO) and renders nowhere.
      expect(store._map.value.get('T-1')?.deleted_at).toBeTruthy()
      expect(store.items.value).toEqual([])
    })

    it('a task deleted during an in-flight fetchOne is not resurrected', async () => {
      const response = deferred<TaskDTO>()
      mockClient.getTask.mockImplementationOnce(() => response.promise)
      const pending = store.fetchOne('T-3')

      fireEvent('task_deleted', { id: 'T-3' })

      response.resolve(makeTask('T-3'))
      await pending
      expect(store._map.value.get('T-3')?.deleted_at).toBeTruthy()
    })

    it('retained queries converge membership via authoritative refresh after SSE moves, preserving server order', async () => {
      const query = store.getQuery({ status: ['Todo'] } as TaskListFilter)
      query.retain()
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('T-2'), makeTask('T-1')]))
      await query.refresh()
      expect(query.tasks.value.map((t) => t.id)).toEqual(['T-2', 'T-1'])
      expect([...query.ranks.value.entries()]).toEqual([
        ['T-2', 0],
        ['T-1', 1],
      ])

      // An existing task moves out of the filter; a new matching task appears.
      fireEvent('task_updated', makeTask('T-1', { status: 'Done' as any }))
      fireEvent('task_created', makeTask('T-3'))

      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('T-3'), makeTask('T-2')]))
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => {
        expect(query.tasks.value.map((t) => t.id)).toEqual(['T-3', 'T-2'])
      })
      expect([...query.ranks.value.entries()]).toEqual([
        ['T-3', 0],
        ['T-2', 1],
      ])
      // The moved-out entity is retained for other consumers but is no member.
      expect(store._map.value.has('T-1')).toBe(true)
      expect(query.ids.value).not.toContain('T-1')
      // The authoritative refresh re-issued the query filter verbatim.
      expect(mockClient.listTasks).toHaveBeenLastCalledWith(
        expect.objectContaining({ status: ['Todo'], limit: 200, offset: 0 }),
      )
      query.release()
    })

    it('released queries are not refreshed by SSE events', async () => {
      const query = store.getQuery({ project: 'LIVE' })
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('L-1')]))
      await query.refresh()
      query.retain()
      query.release()

      fireEvent('task_created', makeTask('L-2'))
      await vi.advanceTimersByTimeAsync(300)
      expect(mockClient.listTasks).toHaveBeenCalledTimes(1)
    })
    it('local mutations (upsert/add/evict) invalidate retained queries without SSE', async () => {
      const query = store.getQuery({ project: 'LOCAL' })
      query.retain()
      mockClient.listTasks.mockResolvedValue(listResponse([]))
      await query.refresh()
      expect(mockClient.listTasks).toHaveBeenCalledTimes(1)

      store.upsert(makeTask('P-1'))
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => expect(mockClient.listTasks).toHaveBeenCalledTimes(2))

      mockClient.addTask.mockResolvedValueOnce(makeTask('P-2'))
      await store.add({ title: 'x' } as any)
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => expect(mockClient.listTasks).toHaveBeenCalledTimes(3))

      store.evict('P-1')
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => expect(mockClient.listTasks).toHaveBeenCalledTimes(4))
      query.release()
    })

    it('invalidation immediately supersedes an in-flight refresh before the debounce fires', async () => {
      const query = store.getQuery({ project: 'A' })
      query.retain()
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('A-1')]))
      await query.refresh()
      expect(query.tasks.value.map((t) => t.id)).toEqual(['A-1'])

      const inFlight = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => inFlight.promise)
      const pending = query.refresh()

      // An SSE event invalidates while the manual refresh is in flight: it
      // must be superseded NOW, not after the 250ms debounce successor.
      fireEvent('task_updated', makeTask('A-1', { title: 'moved' }))
      expect(query.status.value).toBe('loading')

      inFlight.resolve(listResponse([makeTask('STALE-1')]))
      await pending
      expect(query.ids.value).toEqual(['A-1'])
      expect(query.status.value).toBe('loading')

      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('FRESH-1')]))
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => expect(query.tasks.value.map((t) => t.id)).toEqual(['FRESH-1']))
      expect(query.status.value).toBe('ready')
      query.release()
    })

    it('releasing the last owner stops pending and in-flight refreshes', async () => {
      const query = store.getQuery({ project: 'SOLO' })
      query.retain()
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('S-1')]))
      await query.refresh()

      const inFlight = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => inFlight.promise)
      const pending = query.refresh()
      query.release()

      inFlight.resolve(listResponse([makeTask('S-2')]))
      await pending
      expect(query.ids.value).toEqual(['S-1'])
      await vi.advanceTimersByTimeAsync(300)
      expect(mockClient.listTasks).toHaveBeenCalledTimes(2)
    })

    it('an SSE reconnect refreshes retained queries to cover the missed gap', async () => {
      const query = store.getQuery({ project: 'GAP' })
      query.retain()
      mockClient.listTasks.mockResolvedValue(listResponse([]))
      await query.refresh()
      const callsBefore = mockClient.listTasks.mock.calls.length

      const opts = mockSseOptions[mockSseOptions.length - 1]
      expect(typeof opts?.onReconnect).toBe('function')
      opts!.onReconnect!()
      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => {
        expect(mockClient.listTasks.mock.calls.length).toBeGreaterThan(callsBefore)
      })
      query.release()
    })
  })

  // =========================================================================
  // DEV-92: soft deletion / trash lifecycle
  // =========================================================================

  describe('DEV-92 soft deletion', () => {
    it('a trash DTO in the entity map never renders in active queries and vice versa', async () => {
      const stamp = '2026-10-05T10:00:00Z'
      const active = store.getQuery({ project: 'A' })
      const trash = store.getQuery({ project: 'A', deletion: 'deleted' })
      const all = store.getQuery({ project: 'A', deletion: 'all' })

      // Server sends a mixed batch: the deletion-state filter must hold even
      // before/without membership correction.
      mockClient.listTasks.mockResolvedValueOnce(
        listResponse([makeTask('A-1'), makeTask('A-2', { deleted_at: stamp })]),
      )
      await active.refresh()
      expect(active.tasks.value.map((t) => t.id)).toEqual(['A-1'])

      mockClient.listTasks.mockResolvedValueOnce(
        listResponse([makeTask('A-2', { deleted_at: stamp }), makeTask('A-3')]),
      )
      await trash.refresh()
      expect(trash.tasks.value.map((t) => t.id)).toEqual(['A-2'])

      mockClient.listTasks.mockResolvedValueOnce(
        listResponse([makeTask('A-1'), makeTask('A-2', { deleted_at: stamp })]),
      )
      await all.refresh()
      expect(all.tasks.value.map((t) => t.id)).toEqual(['A-1', 'A-2'])
    })

    it('the flat fallback view (no query yet) hides trash entities', () => {
      store.upsert(makeTask('LIVE-1'))
      store.upsert(makeTask('TRASH-1', { deleted_at: '2026-10-05T10:00:00Z' }))
      expect(store.items.value.map((t) => t.id)).toEqual(['LIVE-1'])
    })

    it('a stale active response cannot resurrect a soft delete even with a newer modified', async () => {
      const response = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => response.promise)
      const pending = store.hydrateAll({ project: 'A' }, { clear: true })

      // Soft-delete mark lands while the response is in flight (deleted_at
      // set, modified UNCHANGED per the backend contract).
      store.upsert(
        makeTask('T-1', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }),
      )

      // Stale active snapshot with a NEWER modified still cannot clear the
      // tombstone — the deletion revision, not modified ordering, decides.
      response.resolve(
        listResponse([makeTask('T-1', { modified: '2026-02-02T00:00:00Z' })]),
      )
      await pending
      expect(store._map.value.get('T-1')?.deleted_at).toBeTruthy()
    })

    it('a stale trash response cannot re-delete a restored task even with a newer modified', async () => {
      // Seed a deleted entity, then start a trash-list refresh.
      store.upsert(
        makeTask('T-1', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }),
      )
      const response = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => response.promise)
      const pending = store.hydrateAll({ deletion: 'deleted' }, { clear: true })

      // Restore lands while the response is in flight (same modified) — via
      // the authoritative lifecycle channel (plain snapshots cannot clear).
      store.upsert(makeTask('T-1', { modified: '2026-01-01T00:00:00Z' }), { restore: true })

      // Stale trash snapshot with a NEWER modified still cannot re-delete.
      response.resolve(
        listResponse([
          makeTask('T-1', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-03-03T00:00:00Z' }),
        ]),
      )
      await pending
      expect(store._map.value.get('T-1')?.deleted_at).toBeUndefined()
    })

    it('restore() refreshes retained queries so the task returns to active lists', async () => {
      const query = store.getQuery({ project: 'R' })
      query.retain()
      mockClient.listTasks.mockResolvedValueOnce(listResponse([]))
      await query.refresh()
      expect(mockClient.listTasks.mock.calls.length).toBe(1)

      mockClient.deleteTask.mockResolvedValueOnce({ deleted: true, hard: false, warnings: [] })
      await store.remove('R-1')
      mockClient.restoreTask.mockResolvedValueOnce(makeTask('R-1'))
      await store.restore('R-1')

      await vi.advanceTimersByTimeAsync(300)
      await vi.waitFor(() => expect(mockClient.listTasks.mock.calls.length).toBe(2))
      // The single debounced refresh (remove + restore coalesced) re-issued
      // the retained query after the restore.
      expect(mockClient.listTasks).toHaveBeenLastCalledWith(
        expect.objectContaining({ project: 'R', limit: 200, offset: 0 }),
      )
      expect(store._map.value.get('R-1')?.deleted_at).toBeUndefined()
      query.release()
    })

    it('fetchOne retries the trash endpoint before tombstoning on 404', async () => {
      store.upsert(makeTask('P-9', { deleted_at: '2026-10-05T10:00:00Z' }))
      const notFound = new Error('GET /api/tasks/get failed: Not found') as Error & { status?: number }
      notFound.status = 404
      mockClient.getTask.mockRejectedValueOnce(notFound)
      mockClient.getTask.mockResolvedValueOnce(makeTask('P-9', { deleted_at: '2026-10-05T10:00:00Z' }))

      const result = await store.fetchOne('P-9')

      expect(result?.deleted_at).toBeTruthy()
      expect(store._map.value.has('P-9')).toBe(true)
      expect(mockClient.getTask).toHaveBeenCalledTimes(2)
      expect(mockClient.getTask).toHaveBeenLastCalledWith('P-9', undefined, { includeDeleted: true })
    })

    // -- Causality gap (1): tombstone `>` vs `>=` ---------------------------

    it('a fresh query started after a hard delete hydrates a recreated same-ID task', async () => {
      store.upsert(makeTask('T-1', { title: 'original' }))
      mockClient.deleteTask.mockResolvedValueOnce({ deleted: true, hard: true, warnings: [] })
      await store.remove('T-1', { hard: true })
      expect(store._map.value.has('T-1')).toBe(false)

      // No other clock write in between: the new refresh captures EXACTLY
      // the tombstone clock. Its snapshot is post-delete (recreated task)
      // and must be applied — `>=` would block this hydration forever.
      mockClient.listTasks.mockResolvedValueOnce(listResponse([makeTask('T-1', { title: 'replacement' })]))
      await store.hydrateAll({}, { clear: true })

      expect(store._map.value.get('T-1')?.title).toBe('replacement')
      expect(store.items.value.map((t) => t.id)).toEqual(['T-1'])
    })

    it('a stale pre-delete response stays blocked after a hard delete', async () => {
      const response = deferred<{ total: number; tasks: TaskDTO[] }>()
      mockClient.listTasks.mockImplementationOnce(() => response.promise)
      const pending = store.hydrateAll({ project: 'A' }, { clear: true })

      // Hard tombstone while the response is in flight.
      store.upsert(makeTask('T-1'))
      store.evict('T-1')

      response.resolve(listResponse([makeTask('T-1')]))
      await pending
      expect(store._map.value.has('T-1')).toBe(false)
    })

    // -- Causality gap (2): late 404 vs newer restore/recreate --------------

    function notFoundError() {
      const err = new Error('GET /api/tasks/get failed: Not found') as Error & { status?: number }
      err.status = 404
      return err
    }

    it('a late double-404 cannot evict an entity restored while the fetch was in flight', async () => {
      store.upsert(makeTask('P-1', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }))
      const first = deferred<TaskDTO>()
      mockClient.getTask.mockImplementationOnce(() => first.promise)
      mockClient.getTask.mockRejectedValueOnce(notFoundError())
      const pending = store.fetchOne('P-1')

      // Newer restore (same modified) lands while the request is in flight.
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }), { restore: true })

      first.reject(notFoundError())
      await pending

      expect(store._map.value.has('P-1')).toBe(true)
      expect(store._map.value.get('P-1')?.deleted_at).toBeUndefined()
    })

    it('a late trash-endpoint 404 cannot evict a newer restored entity', async () => {
      store.upsert(makeTask('P-2', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }))
      const late = deferred<TaskDTO>()
      mockClient.getTask.mockImplementationOnce(() => late.promise)
      const pending = store.fetchOne('P-2', { includeDeleted: true })

      store.upsert(makeTask('P-2', { modified: '2026-01-01T00:00:00Z' }), { restore: true })

      late.reject(notFoundError())
      await pending

      expect(store._map.value.has('P-2')).toBe(true)
      expect(store._map.value.get('P-2')?.deleted_at).toBeUndefined()
    })

    // -- Causality gap (3): late mutation responses vs newer lifecycle ------

    it('a pending soft-delete response cannot re-delete a newer restore (same modified)', async () => {
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }))
      const response = deferred<{ deleted: boolean; hard: boolean; warnings: string[] }>()
      mockClient.deleteTask.mockImplementationOnce(() => response.promise)
      const pending = store.remove('P-1')

      // Another client deletes and restores while our request is in flight
      // (both preserve modified). The final live state is ACTIVE.
      store.upsert(makeTask('P-1', { deleted_at: '2026-10-05T11:00:00Z', modified: '2026-01-01T00:00:00Z' }))
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }), { restore: true })

      response.resolve({ deleted: true, hard: false, warnings: [] })
      await pending

      expect(store._map.value.get('P-1')?.deleted_at).toBeUndefined()
      expect(store.items.value.map((t) => t.id)).toEqual(['P-1'])
    })

    it('a pending restore response cannot resurrect a newer soft deletion (same modified)', async () => {
      store.upsert(makeTask('P-1', { deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }))
      const response = deferred<TaskDTO>()
      mockClient.restoreTask.mockImplementationOnce(() => response.promise)
      const pending = store.restore('P-1')

      // Restored and re-deleted by a newer live write while in flight.
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }), { restore: true })
      store.upsert(makeTask('P-1', { deleted_at: '2026-10-05T12:00:00Z', modified: '2026-01-01T00:00:00Z' }))

      response.resolve(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }))
      await pending

      expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy()
    })

    it('a late update response cannot resurrect a soft-deleted task (same modified)', async () => {
      store.upsert(makeTask('P-1', { title: 'Old', modified: '2026-01-01T00:00:00Z' }))
      const response = deferred<TaskDTO>()
      mockClient.updateTask.mockImplementationOnce(() => response.promise)
      const pending = store.update('P-1', { title: 'Edit' } as any)

      // Soft delete lands while the update is in flight (modified unchanged).
      store.upsert(
        makeTask('P-1', { title: 'Old', deleted_at: '2026-10-05T10:00:00Z', modified: '2026-01-01T00:00:00Z' }),
      )

      response.resolve(makeTask('P-1', { title: 'Edit', modified: '2026-01-01T00:00:00Z' }))
      await pending

      expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy()
      expect(store._map.value.get('P-1')?.title).toBe('Old')
    })

    it('plain upsert snapshots cannot clear tombstones or resurrect hard-deleted ids', async () => {
      const stamp = '2026-10-05T10:00:00Z'
      store.upsert(makeTask('P-1', { deleted_at: stamp, modified: '2026-01-01T00:00:00Z' }))

      // Stale active snapshot (mutation response from before the delete).
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }))
      expect(store._map.value.get('P-1')?.deleted_at).toBeTruthy()

      // Restore-flagged upsert is the authoritative clear.
      store.upsert(makeTask('P-1', { modified: '2026-01-01T00:00:00Z' }), { restore: true })
      expect(store._map.value.get('P-1')?.deleted_at).toBeUndefined()

      // Hard-deleted ids stay gone for plain snapshots (SSE recreation uses
      // the internal live path).
      store.upsert(makeTask('P-2'))
      store.evict('P-2')
      store.upsert(makeTask('P-2'))
      expect(store._map.value.has('P-2')).toBe(false)
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
