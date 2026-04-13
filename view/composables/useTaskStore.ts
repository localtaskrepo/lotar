/**
 * Centralised task store — singleton shared by all pages.
 *
 * Data lives in a `Map<id, TaskDTO>` wrapped in a `shallowRef`.
 * Views derive filtered/sorted arrays via `computed()`.
 * The store subscribes to SSE events so filesystem + API mutations
 * are reflected everywhere without per-page refresh loops.
 */
import { computed, shallowRef, triggerRef, type ComputedRef, type ShallowRef } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { TaskCreate, TaskDTO, TaskListFilter, TaskUpdate } from '../api/types'
import { useSse } from './useSse'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type StoreStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface TaskStoreState {
  /** Internal map keyed by task ID.  Exposed as readonly for tests. */
  readonly _map: ShallowRef<Map<string, TaskDTO>>
  /** Monotonically increasing counter bumped on every mutation. */
  readonly version: ShallowRef<number>
  /** Flat array view (derived from the map, recalculated on version bump). */
  readonly items: ComputedRef<TaskDTO[]>
  readonly count: ComputedRef<number>
  /** Total the server reported during the last full sync. */
  readonly serverTotal: ShallowRef<number>
  readonly status: ShallowRef<StoreStatus>
  readonly error: ShallowRef<string | null>
  readonly lastSyncAt: ShallowRef<number>

  // -- Hydration / refresh --------------------------------------------------
  /** Paginated fetch of ALL tasks matching `filter` into the store. */
  hydrateAll(filter?: TaskListFilter, opts?: HydrateOptions): Promise<void>
  /** Fetch a single page (limit/offset) — does NOT clear the store. */
  hydratePage(filter?: TaskListFilter): Promise<{ total: number }>
  /** Fetch or re-fetch a single task by ID and upsert it into the store. */
  fetchOne(id: string): Promise<TaskDTO | null>
  /** Force a full reload (clears store first). */
  forceRefresh(filter?: TaskListFilter): Promise<void>
  /** Returns true if the store has data (regardless of freshness). */
  readonly hasData: ComputedRef<boolean>

  // -- Mutations (API + store) -----------------------------------------------
  add(payload: TaskCreate): Promise<TaskDTO>
  update(id: string, patch: TaskUpdate): Promise<TaskDTO>
  remove(id: string): Promise<void>
  /** Optimistic upsert without an API call (used by TaskPanelHost, SSE). */
  upsert(task: TaskDTO): void
  /** Remove from local store without API call (SSE delete). */
  evict(id: string): void

  // -- SSE lifecycle ---------------------------------------------------------
  connectSse(): void
  disconnectSse(): void
  readonly sseConnected: ShallowRef<boolean>

  /** Register a callback for task_error SSE events (file parse failures). */
  onTaskError(cb: (payload: { id: string; message: string }) => void): () => void
}

export interface HydrateOptions {
  /** Page size for pagination loop (default 200). */
  pageSize?: number
  /** If true, clear existing data before hydrating. */
  clear?: boolean
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

function createTaskStore(client: ApiClient): TaskStoreState {
  const _map = shallowRef<Map<string, TaskDTO>>(new Map())
  const version = shallowRef(0)
  const serverTotal = shallowRef(0)
  const status = shallowRef<StoreStatus>('idle')
  const error = shallowRef<string | null>(null)
  const lastSyncAt = shallowRef(0)
  const sseConnected = shallowRef(false)

  // SSE connection handle
  let sseHandle: ReturnType<typeof useSse> | null = null
  let sseCleaners: Array<() => void> = []

  // Debounce tracker for fswatcher task_updated (ID-only events).
  let pendingFetches = new Map<string, ReturnType<typeof setTimeout>>()

  // Error listeners for task_error SSE events
  const errorListeners = new Set<(payload: { id: string; message: string }) => void>()
  // ---- helpers -----------------------------------------------------------

  function bump() {
    version.value += 1
    triggerRef(_map)
  }

  // ---- derived state -----------------------------------------------------

  const items = computed<TaskDTO[]>(() => {
    // Touch version to re-evaluate when it changes.
    void version.value
    return Array.from(_map.value.values())
  })

  const count = computed(() => items.value.length)
  const hasData = computed(() => _map.value.size > 0)

  // ---- hydration ---------------------------------------------------------

  async function hydrateAll(filter: TaskListFilter = {}, opts: HydrateOptions = {}) {
    const pageSize = opts.pageSize ?? 200
    if (opts.clear) {
      _map.value.clear()
      bump()
    }
    status.value = 'loading'
    error.value = null

    try {
      let currentOffset = 0
      let expectedTotal = 0
      let pages = 0

      while (pages < 10_000) {
        pages += 1
        const response = await client.listTasks({
          ...filter,
          limit: pageSize,
          offset: currentOffset,
        } as any)
        const batch = Array.isArray(response.tasks) ? response.tasks : []
        expectedTotal = response.total ?? expectedTotal
        if (batch.length === 0) break

        for (const task of batch) {
          _map.value.set(task.id, task)
        }
        currentOffset += batch.length
        if (expectedTotal && currentOffset >= expectedTotal) break
      }

      serverTotal.value = expectedTotal || _map.value.size
      lastSyncAt.value = Date.now()
      status.value = 'ready'
      bump()
    } catch (err: unknown) {
      status.value = 'error'
      error.value = err instanceof Error ? err.message : String(err)
    }
  }

  async function hydratePage(filter: TaskListFilter = {}) {
    status.value = 'loading'
    error.value = null

    try {
      const response = await client.listTasks(filter)
      const batch = Array.isArray(response.tasks) ? response.tasks : []
      for (const task of batch) {
        _map.value.set(task.id, task)
      }
      serverTotal.value = response.total ?? serverTotal.value
      status.value = 'ready'
      bump()
      return { total: response.total ?? 0 }
    } catch (err: unknown) {
      status.value = 'error'
      error.value = err instanceof Error ? err.message : String(err)
      return { total: 0 }
    }
  }

  async function fetchOne(id: string): Promise<TaskDTO | null> {
    try {
      const task = await client.getTask(id)
      if (task && task.id) {
        _map.value.set(task.id, task)
        bump()
        return task
      }
      return null
    } catch {
      // Task may have been deleted between event and fetch — evict it.
      if (_map.value.has(id)) {
        _map.value.delete(id)
        bump()
      }
      return null
    }
  }

  async function forceRefresh(filter: TaskListFilter = {}) {
    _map.value = new Map()
    bump()
    await hydrateAll(filter, { clear: false })
  }

  // ---- mutations ----------------------------------------------------------

  async function add(payload: TaskCreate): Promise<TaskDTO> {
    const created = await client.addTask(payload)
    _map.value.set(created.id, created)
    bump()
    return created
  }

  async function update(id: string, patch: TaskUpdate): Promise<TaskDTO> {
    const updated = await client.updateTask(id, patch)
    _map.value.set(updated.id, updated)
    bump()
    return updated
  }

  async function remove(id: string): Promise<void> {
    await client.deleteTask(id)
    _map.value.delete(id)
    bump()
  }

  function upsert(task: TaskDTO) {
    _map.value.set(task.id, task)
    bump()
  }

  function evict(id: string) {
    if (_map.value.has(id)) {
      _map.value.delete(id)
      bump()
    }
  }

  // ---- SSE ----------------------------------------------------------------

  function handleSseEvent(kind: string, payload: any) {
    if (!payload) return
    const id: string | undefined = payload.id

    switch (kind) {
      case 'task_created': {
        // API-triggered: payload is full TaskDTO
        if (id && payload.title) {
          upsert(payload as TaskDTO)
        }
        break
      }
      case 'task_updated': {
        if (!id) break
        // API-triggered events include the full DTO (has title).
        // Filesystem-watcher events only include { id }.
        if (payload.title) {
          upsert(payload as TaskDTO)
        } else {
          // Debounce per-ID so rapid writes don't flood single-task fetches.
          const existing = pendingFetches.get(id)
          if (existing) clearTimeout(existing)
          pendingFetches.set(
            id,
            setTimeout(() => {
              pendingFetches.delete(id)
              void fetchOne(id)
            }, 150),
          )
        }
        break
      }
      case 'task_deleted': {
        if (id) {
          // Cancel any pending fetch for this task.
          const pending = pendingFetches.get(id)
          if (pending) {
            clearTimeout(pending)
            pendingFetches.delete(id)
          }
          evict(id)
        }
        break
      }
      // config_updated / project_changed: views can listen separately if needed
    }
  }

  function connectSse() {
    if (sseHandle) return
    sseHandle = useSse('/api/events', {
      kinds: 'task_created,task_updated,task_deleted,task_error',
      ready: true,
    })
    sseConnected.value = true

    const kinds = ['task_created', 'task_updated', 'task_deleted'] as const
    for (const kind of kinds) {
      const handler = (ev: MessageEvent) => {
        if (!ev.data) return
        try {
          const data = JSON.parse(ev.data)
          handleSseEvent(kind, data)
        } catch { /* ignore malformed */ }
      }
      sseHandle.on(kind, handler)
      sseCleaners.push(() => sseHandle?.off(kind, handler))
    }

    // Forward task_error events to registered listeners
    const errorHandler = (ev: MessageEvent) => {
      if (!ev.data) return
      try {
        const data = JSON.parse(ev.data)
        if (data?.id && data?.message) {
          for (const cb of errorListeners) cb(data)
        }
      } catch { /* ignore malformed */ }
    }
    sseHandle.on('task_error', errorHandler)
    sseCleaners.push(() => sseHandle?.off('task_error', errorHandler))
  }

  function onTaskError(cb: (payload: { id: string; message: string }) => void): () => void {
    errorListeners.add(cb)
    return () => { errorListeners.delete(cb) }
  }

  function disconnectSse() {
    sseCleaners.splice(0).forEach((fn) => fn())
    if (sseHandle) {
      sseHandle.close()
      sseHandle = null
    }
    sseConnected.value = false
    // Clear pending debounced fetches
    for (const timer of pendingFetches.values()) clearTimeout(timer)
    pendingFetches.clear()
  }

  return {
    _map,
    version,
    items,
    count,
    serverTotal,
    status,
    error,
    lastSyncAt,
    hasData,
    hydrateAll,
    hydratePage,
    fetchOne,
    forceRefresh,
    add,
    update,
    remove,
    upsert,
    evict,
    connectSse,
    disconnectSse,
    sseConnected,
    onTaskError,
  }
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

let _instance: TaskStoreState | null = null

export function useTaskStore(): TaskStoreState {
  if (!_instance) {
    _instance = createTaskStore(api)
  }
  return _instance
}

/** For tests: reset the singleton so each test gets a clean store. */
export function _resetTaskStore() {
  if (_instance) {
    _instance.disconnectSse()
  }
  _instance = null
}

/** For tests: create an isolated store with a custom client. */
export function _createTestTaskStore(client: ApiClient): TaskStoreState {
  return createTaskStore(client)
}
