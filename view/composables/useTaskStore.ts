/**
 * Centralised task store — singleton shared by all pages.
 *
 * ## DEV-65 state contract
 *
 * The store is split into two layers:
 *
 * 1. **Entity layer** — `_map` is a normalized `Map<id, TaskDTO>` shared by
 *    every consumer. Entities are written only through guarded helpers:
 *    *live* writes (SSE DTOs, panel upserts, API mutation responses) always
 *    apply, while *response* writes (hydrate/fetchOne completions) are checked
 *    against a monotonic write clock and delete tombstones so a late response
 *    can never overwrite a newer entity published by SSE/panel, nor resurrect
 *    a task deleted after that request started.
 *
 * 2. **Query layer** — every server filter maps to a keyed query entry owning
 *    the ordered result: membership IDs, a per-query rank ledger preserving
 *    the DEV-57 server response order, the server total, status/error, and a
 *    request generation counter. Only the server decides membership: entity
 *    upserts (SSE/panel/fetchOne) never mutate query membership, and a query
 *    refresh REPLACES membership with the authoritative response. Completions
 *    from an older generation are rejected — including their status/error —
 *    so out-of-order responses cannot publish old query data.
 *
 * Consumers obtain a `TaskQueryHandle` via `getQuery(filter)`. Handles can be
 * `retain()`ed while their consumer is mounted; task SSE events then schedule
 * a debounced authoritative refresh of every retained query, so tasks moving
 * in or out of a filter converge without any client-side predicate engine
 * (the server stays the only filter authority).
 *
 * The legacy flat surface (`items`/`count`/`orderIndex`/`serverTotal`/
 * `status`/`error`/`lastSyncAt`/`hydrateAll`/`hydratePage`/`forceRefresh`)
 * remains a view over the *active* (most recently hydrated) query, falling
 * back to the raw entity map before any query exists.
 */
import { computed, shallowRef, triggerRef, type ComputedRef, type ShallowRef } from 'vue'
import type { ApiClient } from '../api/client'
import { api } from '../api/client'
import type { TaskCreate, TaskDTO, TaskListFilter, TaskUpdate } from '../api/types'
import { registerTaskQueryInvalidator } from './useCompletionPolicy'
import { useSse } from './useSse'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type StoreStatus = 'idle' | 'loading' | 'ready' | 'error'

export interface TaskQueryHandle {
  /** Canonical key of the filter this query was created from. */
  readonly key: string
  /**
   * True once this query has published a good server snapshot. While a newly
   * adopted query is still pending (no snapshot), consumers can keep
   * rendering the previous query's rows instead of a false empty state.
   */
  readonly hasSnapshot: ShallowRef<boolean>
  /** Ordered membership IDs from the last authoritative server response. */
  readonly ids: ShallowRef<readonly string[]>
  /**
   * Authority order ledger (DEV-57): insertion rank of every task in the
   * server's global response order for THIS query. SSE/entity writes never
   * touch ranks; only a completed refresh rewrites them.
   */
  readonly ranks: ShallowRef<Map<string, number>>
  /** Total the server reported for this query. */
  readonly total: ShallowRef<number>
  readonly status: ShallowRef<StoreStatus>
  readonly error: ShallowRef<string | null>
  readonly lastSyncAt: ShallowRef<number>
  /** Members resolved against the entity map, in `ids` order. */
  readonly tasks: ComputedRef<TaskDTO[]>
  /**
   * Authoritative refresh: replaces membership with the full server response
   * (all pages). Stale completions of earlier refreshes are rejected.
   */
  refresh(opts?: QueryRefreshOptions): Promise<void>
  /**
   * Live subscription. Retained queries are refreshed (debounced) after task
   * SSE events so membership moves converge on every mounted consumer.
   */
  retain(): void
  release(): void
}

export interface QueryRefreshOptions {
  /** Page size for the pagination loop (default 200). */
  pageSize?: number
}

export interface TaskStoreState {
  /**
   * Normalized entity map keyed by task ID. Shared by all queries; query
   * refreshes merge entities in but never clear unrelated entities.
   * Exposed as readonly for tests.
   */
  readonly _map: ShallowRef<Map<string, TaskDTO>>
  /** Monotonically increasing counter bumped on every entity mutation. */
  readonly version: ShallowRef<number>
  /**
   * Flat array view over the ACTIVE query's membership (or, before any query
   * exists, over the raw entity map). Foreign SSE entities are NOT included
   * unless the server made them members.
   */
  readonly items: ComputedRef<TaskDTO[]>
  readonly count: ComputedRef<number>
  /**
   * Authority order ledger of the ACTIVE query (DEV-57 server order; see
   * `TaskQueryHandle.ranks` for the per-query contract).
   */
  readonly orderIndex: ComputedRef<Map<string, number>>
  /** Total the server reported for the ACTIVE query. */
  readonly serverTotal: ComputedRef<number>
  readonly status: ComputedRef<StoreStatus>
  readonly error: ComputedRef<string | null>
  readonly lastSyncAt: ComputedRef<number>

  // -- Keyed queries ----------------------------------------------------------
  /** Get (or create) the keyed query handle for `filter`. */
  getQuery(filter?: TaskListFilter): TaskQueryHandle

  // -- Hydration / refresh ----------------------------------------------------
  /**
   * Paginated fetch of ALL tasks matching `filter` into the keyed query.
   * `clear: true` (replace membership) is the scoped form used by pages;
   * the default merge keeps legacy entity-union semantics.
   */
  hydrateAll(filter?: TaskListFilter, opts?: HydrateOptions): Promise<void>
  /** Fetch a single page (limit/offset) — merges entities, never replaces membership. */
  hydratePage(filter?: TaskListFilter): Promise<{ total: number }>
  /**
   * Fetch or re-fetch a single task by ID and upsert it into the entity map.
   * Only 404/410 evicts + tombstones; transient failures keep the entity and
   * surface the message through `fetchOneError`.
   */
  fetchOne(id: string): Promise<TaskDTO | null>
  /** Last transient fetchOne failure message (entity was retained). Cleared on the next call. */
  readonly fetchOneError: ShallowRef<string | null>
  /** Force a full reload (clears entities first). */
  forceRefresh(filter?: TaskListFilter): Promise<void>
  /**
   * DEV-21: schedule a debounced authoritative refresh of every RETAINED
   * query (same primitive task SSE events use). Used when the server
   * calendar day rolls so embedded task_state metadata is refetched.
   */
  invalidateQueries(): void
  /** Returns true if the active query has data (regardless of freshness). */
  readonly hasData: ComputedRef<boolean>

  // -- Mutations (API + store) -----------------------------------------------
  add(payload: TaskCreate): Promise<TaskDTO>
  update(id: string, patch: TaskUpdate): Promise<TaskDTO>
  remove(id: string): Promise<void>
  /** Optimistic entity upsert without an API call (TaskPanelHost, SSE). Never joins a query. */
  upsert(task: TaskDTO): void
  /** Remove the entity without an API call (SSE delete). Tombstones the ID. */
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
  /**
   * If true, membership is REPLACED by the response (scoped hydrate).
   * If false/omitted, legacy merge semantics apply (union with existing
   * membership / entity keys).
   */
  clear?: boolean
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

interface QueryEntry {
  key: string
  /** Immutable filter snapshot this query is authoritative for. */
  filter: TaskListFilter
  /** Per-query request generation; older completions are rejected. */
  generation: number
  /** Number of completed refresh runs ( distinguishes the first merge seed ). */
  runs: number
  liveHandles: number
  /** Currently running refreshes for this entry. */
  inFlight: number
  /** True once a good server snapshot has been published. */
  hasSnapshot: ShallowRef<boolean>
  refreshTimer: ReturnType<typeof setTimeout> | null
  ids: ShallowRef<readonly string[]>
  ranks: ShallowRef<Map<string, number>>
  total: ShallowRef<number>
  status: ShallowRef<StoreStatus>
  error: ShallowRef<string | null>
  lastSyncAt: ShallowRef<number>
}

const DEFAULT_PAGE_SIZE = 200
const MAX_PAGES = 10_000
/** Debounce for SSE-triggered authoritative refreshes of retained queries. */
const LIVE_REFRESH_DEBOUNCE_MS = 250
/** Debounce for fswatcher task_updated (ID-only) single-task fetches. */
const FETCH_ONE_DEBOUNCE_MS = 150
/** Paging params are per-request only; they must not fork the query key. */
const PAGING_KEYS = new Set(['limit', 'offset', 'page'])

function canonicalQueryKey(filter: TaskListFilter = {}): string {
  const source = filter as Record<string, unknown>
  const parts: string[] = []
  for (const key of Object.keys(source).sort()) {
    if (PAGING_KEYS.has(key)) continue
    const value = source[key]
    if (value === undefined || value === null || value === '') continue
    if (Array.isArray(value)) {
      const items = [...value].map((item) => String(item)).sort()
      parts.push(`${key}=[${items.join(',')}]`)
    } else if (typeof value === 'object') {
      parts.push(`${key}=${JSON.stringify(value)}`)
    } else {
      parts.push(`${key}=${String(value)}`)
    }
  }
  return parts.length ? parts.join('&') : '*'
}

function createTaskStore(client: ApiClient): TaskStoreState {
  const _map = shallowRef<Map<string, TaskDTO>>(new Map())
  const version = shallowRef(0)
  const sseConnected = shallowRef(false)
  const fetchOneError = shallowRef<string | null>(null)

  // ---- entity guards (DEV-65) ---------------------------------------------
  // Monotonic write clock: every entity write/delete stamps an increasing
  // revision. In-flight requests capture the clock at start and their
  // responses may only apply if nothing newer happened to that entity.
  let writeClock = 0
  const entityRevisions = new Map<string, number>()
  const tombstones = new Map<string, number>()

  // ---- keyed queries -------------------------------------------------------
  const queries = new Map<string, QueryEntry>()
  const queriesVersion = shallowRef(0)
  const activeQueryKey = shallowRef<string | null>(null)

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

  /** Live entity write (SSE DTO, panel upsert, API mutation response). */
  function writeEntity(task: TaskDTO): void {
    if (!task?.id) return
    _map.value.set(task.id, task)
    entityRevisions.set(task.id, ++writeClock)
    tombstones.delete(task.id)
    bump()
  }

  /**
   * Delete + tombstone. The tombstone revision prevents a stale in-flight
   * response from resurrecting the task.
   */
  function tombstoneEntity(id: string): void {
    if (!_map.value.has(id)) {
      if (!tombstones.has(id)) tombstones.set(id, ++writeClock)
      return
    }
    _map.value.delete(id)
    entityRevisions.delete(id)
    tombstones.set(id, ++writeClock)
    bump()
  }

  function compareModified(next: string | undefined, current: string | undefined): number {
    const a = next ? Date.parse(next) : NaN
    const b = current ? Date.parse(current) : NaN
    if (Number.isNaN(a) || Number.isNaN(b)) return 0
    return a < b ? -1 : a > b ? 1 : 0
  }

  /**
   * Apply an entity from an asynchronous RESPONSE, guarded against newer
   * writes that happened while the request was in flight:
   * - a tombstone stamped after the request started wins (no resurrection);
   * - an entity written live after the request started (SSE/panel) wins on
   *   ties and older response snapshots;
   * - an older response snapshot than what we already hold is dropped.
   */
  function applyResponseEntity(task: TaskDTO, requestClock: number): boolean {
    const id = task?.id
    if (!id) return false
    const tomb = tombstones.get(id)
    if (tomb !== undefined && tomb >= requestClock) return false
    const existing = _map.value.get(id)
    if (existing) {
      const existingRev = entityRevisions.get(id) ?? 0
      const cmp = compareModified(task.modified, existing.modified)
      if (existingRev > requestClock ? cmp <= 0 : cmp < 0) return false
    }
    writeEntity(task)
    return true
  }

  function ensureQuery(filter: TaskListFilter = {}): QueryEntry {
    const key = canonicalQueryKey(filter)
    let entry = queries.get(key)
    if (!entry) {
      entry = {
        key,
        filter: { ...filter },
        generation: 0,
        runs: 0,
        liveHandles: 0,
        inFlight: 0,
        hasSnapshot: shallowRef(false),
        refreshTimer: null,
        ids: shallowRef<readonly string[]>([]),
        ranks: shallowRef<Map<string, number>>(new Map()),
        total: shallowRef(0),
        status: shallowRef<StoreStatus>('idle'),
        error: shallowRef<string | null>(null),
        lastSyncAt: shallowRef(0),
      }
      queries.set(key, entry)
      queriesVersion.value += 1
    }
    return entry
  }

  function setActiveQuery(entry: QueryEntry): void {
    if (activeQueryKey.value !== entry.key) activeQueryKey.value = entry.key
  }

  function clearEntryTimer(entry: QueryEntry): void {
    if (entry.refreshTimer !== null) {
      clearTimeout(entry.refreshTimer)
      entry.refreshTimer = null
    }
  }

  /**
   * Invalidate one retained query: schedule a debounced authoritative
   * refresh AND immediately supersede any in-flight refresh so its stale
   * membership cannot publish during the debounce window. The entry stays
   * `loading` until the successor runs, so it can never get stuck `ready`
   * with stale rows or starve (each timer fires 250ms after it is set).
   */
  function invalidateEntry(entry: QueryEntry): void {
    if (entry.liveHandles <= 0) return
    if (entry.refreshTimer === null) {
      entry.refreshTimer = setTimeout(() => {
        entry.refreshTimer = null
        if (entry.liveHandles <= 0) return
        void runQueryRefresh(entry)
      }, LIVE_REFRESH_DEBOUNCE_MS)
    }
    if (entry.inFlight > 0) entry.generation += 1
  }

  function invalidateRetainedQueries(): void {
    for (const entry of queries.values()) invalidateEntry(entry)
  }

  async function runQueryRefresh(entry: QueryEntry, opts: QueryRefreshOptions & { merge?: boolean } = {}): Promise<void> {
    const generation = ++entry.generation
    const requestClock = writeClock
    const pageSize = opts.pageSize ?? DEFAULT_PAGE_SIZE
    const merge = opts.merge === true
    entry.status.value = 'loading'
    entry.error.value = null
    entry.inFlight += 1

    // Merge mode keeps the legacy union semantics: existing members (or, on
    // the very first run, the raw entity keys) stay, response tasks append.
    const seed: string[] = merge
      ? entry.runs === 0 && entry.ids.value.length === 0
        ? Array.from(_map.value.keys())
        : [...entry.ids.value]
      : []
    const ids = new Set<string>(seed)
    const ranks = new Map<string, number>()

    try {
      let currentOffset = 0
      let expectedTotal = 0
      let pages = 0
      while (pages < MAX_PAGES) {
        pages += 1
        const response = await client.listTasks({
          ...entry.filter,
          limit: pageSize,
          offset: currentOffset,
        } as any)
        if (generation !== entry.generation) return
        const batch = Array.isArray(response?.tasks) ? response.tasks : []
        expectedTotal = response?.total ?? expectedTotal
        for (const task of batch) {
          if (!task?.id) continue
          applyResponseEntity(task, requestClock)
          ranks.set(task.id, ranks.size)
          ids.add(task.id)
        }
        if (batch.length === 0) break
        currentOffset += batch.length
        if (expectedTotal && currentOffset >= expectedTotal) break
      }

      if (generation !== entry.generation) return
      entry.runs += 1
      entry.ids.value = Array.from(ids)
      entry.ranks.value = ranks
      entry.total.value = expectedTotal || ids.size
      entry.lastSyncAt.value = Date.now()
      entry.hasSnapshot.value = true
      entry.status.value = 'ready'
      bump()
    } catch (err: unknown) {
      if (generation !== entry.generation) return
      entry.runs += 1
      entry.status.value = 'error'
      entry.error.value = err instanceof Error ? err.message : String(err)
    } finally {
      entry.inFlight -= 1
    }
  }

  function makeHandle(entry: QueryEntry): TaskQueryHandle {
    const tasks = computed<TaskDTO[]>(() => {
      void version.value
      const out: TaskDTO[] = []
      for (const id of entry.ids.value) {
        const task = _map.value.get(id)
        if (task) out.push(task)
      }
      return out
    })
    return {
      key: entry.key,
      hasSnapshot: entry.hasSnapshot,
      ids: entry.ids,
      ranks: entry.ranks,
      total: entry.total,
      status: entry.status,
      error: entry.error,
      lastSyncAt: entry.lastSyncAt,
      tasks,
      refresh: (opts?: QueryRefreshOptions) => runQueryRefresh(entry, opts),
      retain: () => {
        entry.liveHandles += 1
      },
      release: () => {
        entry.liveHandles = Math.max(0, entry.liveHandles - 1)
        if (entry.liveHandles === 0) {
          clearEntryTimer(entry)
          // Last owner left: an in-flight refresh must not publish either.
          if (entry.inFlight > 0) entry.generation += 1
        }
      },
    }
  }

  function getQuery(filter: TaskListFilter = {}): TaskQueryHandle {
    return makeHandle(ensureQuery(filter))
  }

  // ---- derived (legacy) state ---------------------------------------------

  const activeEntry = computed<QueryEntry | null>(() => {
    void queriesVersion.value
    const key = activeQueryKey.value
    return key ? queries.get(key) ?? null : null
  })

  const items = computed<TaskDTO[]>(() => {
    void version.value
    const entry = activeEntry.value
    if (!entry) return Array.from(_map.value.values())
    void entry.ids.value
    const out: TaskDTO[] = []
    for (const id of entry.ids.value) {
      const task = _map.value.get(id)
      if (task) out.push(task)
    }
    return out
  })

  const count = computed(() => items.value.length)
  const hasData = computed(() => items.value.length > 0)
  const orderIndex = computed<Map<string, number>>(
    () => activeEntry.value?.ranks.value ?? new Map<string, number>(),
  )
  const serverTotal = computed(() => activeEntry.value?.total.value ?? 0)
  const status = computed<StoreStatus>(() => activeEntry.value?.status.value ?? 'idle')
  const error = computed<string | null>(() => activeEntry.value?.error.value ?? null)
  const lastSyncAt = computed(() => activeEntry.value?.lastSyncAt.value ?? 0)

  // ---- hydration ---------------------------------------------------------

  async function hydrateAll(filter: TaskListFilter = {}, opts: HydrateOptions = {}) {
    const entry = ensureQuery(filter)
    // Switch the legacy view immediately so a slow previous query's rows (or
    // errors) cannot linger while the new query loads.
    setActiveQuery(entry)
    await runQueryRefresh(entry, {
      merge: opts.clear !== true,
      pageSize: opts.pageSize,
    })
  }

  async function hydratePage(filter: TaskListFilter = {}) {
    const entry = ensureQuery(filter)
    setActiveQuery(entry)
    const generation = ++entry.generation
    const requestClock = writeClock
    entry.status.value = 'loading'
    entry.error.value = null
    const seed =
      entry.runs === 0 && entry.ids.value.length === 0
        ? Array.from(_map.value.keys())
        : [...entry.ids.value]

    try {
      const response = await client.listTasks(filter)
      if (generation !== entry.generation) return { total: entry.total.value }
      const batch = Array.isArray(response?.tasks) ? response.tasks : []
      const ids = new Set<string>(seed)
      for (const task of batch) {
        if (!task?.id) continue
        applyResponseEntity(task, requestClock)
        ids.add(task.id)
      }
      entry.runs += 1
      entry.ids.value = Array.from(ids)
      if (typeof response?.total === 'number') entry.total.value = response.total
      entry.lastSyncAt.value = Date.now()
      entry.status.value = 'ready'
      bump()
      return { total: entry.total.value }
    } catch (err: unknown) {
      if (generation !== entry.generation) return { total: 0 }
      entry.runs += 1
      entry.status.value = 'error'
      entry.error.value = err instanceof Error ? err.message : String(err)
      return { total: 0 }
    }
  }

  async function fetchOne(id: string): Promise<TaskDTO | null> {
    const requestClock = writeClock
    fetchOneError.value = null
    try {
      const task = await client.getTask(id)
      if (task && task.id) {
        applyResponseEntity(task, requestClock)
        return task
      }
      return null
    } catch (err: unknown) {
      const status = (err as { status?: unknown } | null | undefined)?.status
      if (status === 404 || status === 410) {
        // The task is gone: evict + tombstone so stale responses cannot
        // resurrect it.
        tombstoneEntity(id)
      } else {
        // Transient failure (network/5xx): retain the entity and surface the
        // error instead of silently dropping data.
        fetchOneError.value = err instanceof Error ? err.message : String(err)
      }
      return null
    }
  }

  async function forceRefresh(filter: TaskListFilter = {}) {
    // DEV-65: force-refresh replaces THIS query's membership only. Clearing
    // the shared entity map would evict entities other keyed queries still
    // present; tombstones already protect against resurrection.
    const entry = ensureQuery(filter)
    setActiveQuery(entry)
    await runQueryRefresh(entry)
  }

  // ---- mutations ----------------------------------------------------------

  async function add(payload: TaskCreate): Promise<TaskDTO> {
    const created = await client.addTask(payload)
    writeEntity(created)
    invalidateRetainedQueries()
    return created
  }

  async function update(id: string, patch: TaskUpdate): Promise<TaskDTO> {
    const updated = await client.updateTask(id, patch)
    writeEntity(updated)
    invalidateRetainedQueries()
    return updated
  }

  async function remove(id: string): Promise<void> {
    await client.deleteTask(id)
    tombstoneEntity(id)
    invalidateRetainedQueries()
  }

  function upsert(task: TaskDTO) {
    writeEntity(task)
    // Local panel upserts must converge membership exactly like SSE events;
    // applying a query response never schedules (no recursion).
    invalidateRetainedQueries()
  }

  function evict(id: string) {
    tombstoneEntity(id)
    invalidateRetainedQueries()
  }

  // ---- SSE ----------------------------------------------------------------

  function handleSseEvent(kind: string, payload: any) {
    if (!payload) return
    const id: string | undefined = payload.id

    switch (kind) {
      case 'task_created': {
        // API-triggered: payload is full TaskDTO. Entity-only: membership is
        // decided by the authoritative refresh scheduled below.
        if (id && payload.title) {
          writeEntity(payload as TaskDTO)
        }
        invalidateRetainedQueries()
        break
      }
      case 'task_updated': {
        if (!id) break
        // API-triggered events include the full DTO (has title).
        // Filesystem-watcher events only include { id }.
        if (payload.title) {
          writeEntity(payload as TaskDTO)
          invalidateRetainedQueries()
        } else {
          // Debounce per-ID so rapid writes don't flood single-task fetches.
          const existing = pendingFetches.get(id)
          if (existing) clearTimeout(existing)
          pendingFetches.set(
            id,
            setTimeout(() => {
              pendingFetches.delete(id)
              void fetchOne(id)
            }, FETCH_ONE_DEBOUNCE_MS),
          )
          invalidateRetainedQueries()
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
          tombstoneEntity(id)
        }
        invalidateRetainedQueries()
        break
      }
      // config_updated / project_changed: views can listen separately if needed
    }
  }

  function connectSse() {
    if (sseHandle) return
    sseHandle = useSse(
      '/api/events',
      { kinds: 'task_created,task_updated,task_deleted,task_error', ready: true },
      {
        // A reconnect may have missed events: refresh every retained query
        // from the authoritative server before trusting membership again.
        onReconnect: () => invalidateRetainedQueries(),
      },
    )
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
    fetchOneError,
    items,
    count,
    orderIndex,
    serverTotal,
    status,
    error,
    lastSyncAt,
    hasData,
    getQuery,
    hydrateAll,
    hydratePage,
    fetchOne,
    forceRefresh,
    invalidateQueries: invalidateRetainedQueries,
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
    // DEV-21: the shared completion policy refreshes retained task queries
    // when the server calendar day rolls (stale task_state metadata).
    const store = _instance
    registerTaskQueryInvalidator(() => store.invalidateQueries())
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
