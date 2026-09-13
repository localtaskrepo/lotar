import type { DeepReadonly, ShallowRef } from 'vue'
import { reactive, readonly, shallowRef } from 'vue'
import { api } from '../api/client'
import type { ActivityFeedItem } from '../api/types'
import { MS_PER_DAY, startOfLocalDay } from '../utils/date'

export type ActivityItem = {
  id: string
  time: string
  kind: 'info' | 'update' | 'create' | 'delete' | 'error'
  message: string
}

export type TaskTouch = {
  id: string
  kind: 'created' | 'updated' | 'deleted'
  time: string
  actor?: string
  title?: string
}

/**
 * Semantic scope of an activity-feed query (DEV-66). `since`/`until` are
 * deliberately NOT part of the identity: they are recomputed from
 * `windowDays` on every refresh so each refresh genuinely refetches the
 * moving window, while the key stays stable (no unbounded key growth).
 */
export interface ActivityFeedQuery {
  /** Project prefix, or undefined for the global (all-projects) feed. */
  project?: string
  /** Window length in days; the request covers the last N local days. */
  windowDays: number
  /** Server-side cap on returned feed items. */
  limit: number
}

export interface ActivityFeedHandle {
  /** Canonical key of the query scope this handle is bound to. */
  readonly key: string
  /** True once a good server snapshot has been published for this scope. */
  readonly hasSnapshot: ShallowRef<boolean>
  /** Items from the last authoritative response for this scope. */
  readonly items: ShallowRef<readonly ActivityFeedItem[]>
  readonly loading: ShallowRef<boolean>
  readonly error: ShallowRef<string | null>
  readonly lastSyncAt: ShallowRef<number>
  /**
   * Fetch the latest window. Overlapping refreshes are allowed: every call
   * bumps the entry generation and only the latest request may publish
   * items, errors, or the settled loading state.
   */
  refresh(): Promise<void>
  /**
   * Drop this handle's ownership of the keyed entry. When the last owner
   * leaves, in-flight refreshes are invalidated so nothing publishes later.
   */
  release(): void
}

type ActivityState = {
  items: ActivityItem[]
  touches: Record<string, TaskTouch>
}

let state: ActivityState | null = null

function ensure() {
  if (!state) {
    state = reactive<ActivityState>({ items: [], touches: {} })
  }
  return state!
}

function genId() {
  return Math.random().toString(36).slice(2) + Date.now().toString(36)
}

// ---------------------------------------------------------------------------
// Keyed feed queries (DEV-66)
// ---------------------------------------------------------------------------

interface FeedEntry {
  key: string
  query: ActivityFeedQuery
  /** Latest-wins counter: completions from older generations publish nothing. */
  generation: number
  inFlight: number
  handles: number
  hasSnapshot: ShallowRef<boolean>
  items: ShallowRef<readonly ActivityFeedItem[]>
  loading: ShallowRef<boolean>
  error: ShallowRef<string | null>
  lastSyncAt: ShallowRef<number>
}

const feedEntries = new Map<string, FeedEntry>()

/**
 * Normalize the project filter once so the key and the outgoing request
 * always agree. An absent/blank project means the global feed; a literal
 * `*` stays an EXPLICIT project filter (never silently broadened to global).
 */
function normalizeFeedProject(project: string | undefined): string | undefined {
  const trimmed = (project ?? '').trim()
  return trimmed.length ? trimmed : undefined
}

/**
 * Unambiguous tuple key: JSON of [project|null, windowDays, limit]. No
 * sentinel string that a literal project value could collide with.
 */
function feedQueryKey(query: ActivityFeedQuery): string {
  return JSON.stringify([normalizeFeedProject(query.project) ?? null, query.windowDays, query.limit])
}

function ensureFeedEntry(query: ActivityFeedQuery): FeedEntry {
  const key = feedQueryKey(query)
  let entry = feedEntries.get(key)
  if (!entry) {
    entry = {
      key,
      query: { project: normalizeFeedProject(query.project), windowDays: query.windowDays, limit: query.limit },
      generation: 0,
      inFlight: 0,
      handles: 0,
      hasSnapshot: shallowRef(false),
      items: shallowRef<readonly ActivityFeedItem[]>([]),
      loading: shallowRef(false),
      error: shallowRef<string | null>(null),
      lastSyncAt: shallowRef(0),
    }
    feedEntries.set(key, entry)
  }
  return entry
}

async function refreshFeedEntry(entry: FeedEntry): Promise<void> {
  const generation = ++entry.generation
  entry.inFlight += 1
  entry.loading.value = true
  entry.error.value = null
  // The window is computed per request on purpose: a manual refresh must
  // genuinely refetch, so timestamps never become part of the query key.
  const now = new Date()
  const since = startOfLocalDay(new Date(now.getTime() - (entry.query.windowDays - 1) * MS_PER_DAY))
  try {
    const params: { since: string; until: string; limit: number; project?: string } = {
      since: since.toISOString(),
      until: now.toISOString(),
      limit: entry.query.limit,
    }
    if (entry.query.project) params.project = entry.query.project
    const items = await api.activityFeed(params)
    if (generation !== entry.generation) return
    entry.items.value = items
    entry.hasSnapshot.value = true
    entry.lastSyncAt.value = Date.now()
  } catch (err: unknown) {
    if (generation !== entry.generation) return
    // Preserve the last good snapshot: a failed background refresh surfaces
    // the error next to the stale rows instead of blanking the scope.
    entry.error.value = (err as { message?: string } | null)?.message || 'Failed to load activity'
  } finally {
    entry.inFlight -= 1
    if (generation === entry.generation) entry.loading.value = false
  }
}

function makeFeedHandle(entry: FeedEntry): ActivityFeedHandle {
  entry.handles += 1
  return {
    key: entry.key,
    hasSnapshot: entry.hasSnapshot,
    items: entry.items,
    loading: entry.loading,
    error: entry.error,
    lastSyncAt: entry.lastSyncAt,
    refresh: () => refreshFeedEntry(entry),
    release: () => {
      entry.handles = Math.max(0, entry.handles - 1)
      if (entry.handles === 0 && entry.inFlight > 0) {
        // Last owner left: the in-flight request must not publish later.
        entry.generation += 1
        entry.loading.value = false
      }
    },
  }
}

// ---------------------------------------------------------------------------
// Public composable
// ---------------------------------------------------------------------------

export function useActivity() {
  const s = ensure()
  function add(item: Omit<ActivityItem, 'id' | 'time'> & { time?: string }) {
    const it: ActivityItem = { id: genId(), time: item.time || new Date().toISOString(), kind: item.kind as any, message: item.message }
    // prepend newest
    s.items.unshift(it)
    // cap log size
    if (s.items.length > 200) s.items.length = 200
  }
  function clear() { s.items.splice(0, s.items.length) }
  function markTaskTouch(touch: Omit<TaskTouch, 'time'> & { time?: string }) {
    const entry: TaskTouch = {
      ...touch,
      time: touch.time || new Date().toISOString(),
    }
    s.touches[entry.id] = entry
    // Auto-expire touches after a few minutes to keep highlights fresh
    if (typeof window !== 'undefined') {
      window.setTimeout(() => {
        const current = s.touches[entry.id]
        if (current && current.time === entry.time) {
          delete s.touches[entry.id]
        }
      }, 5 * 60 * 1000)
    }
  }
  function removeTaskTouch(id: string) {
    if (id in s.touches) {
      delete s.touches[id]
    }
  }
  function clearTouches() {
    Object.keys(s.touches).forEach((key) => delete s.touches[key])
  }
  /**
   * Get (or create) the keyed feed handle for `query`. Each call owns one
   * reference: call `release()` when the consumer unmounts or changes scope.
   * Handles with the same key share one entry, so concurrent consumers of
   * identical scope reuse the same data, while different scopes (e.g. the
   * activity drawer's global 30d/200 feed vs Insights' project/window feed)
   * stay fully isolated.
   */
  function getFeedQuery(query: ActivityFeedQuery): ActivityFeedHandle {
    return makeFeedHandle(ensureFeedEntry(query))
  }
  const items = readonly(s.items) as DeepReadonly<ActivityItem[]>
  const touches = readonly(s.touches) as DeepReadonly<Record<string, TaskTouch>>

  return {
    items,
    touches,
    add,
    clear,
    markTaskTouch,
    removeTaskTouch,
    clearTouches,
    getFeedQuery,
  }
}

/** For tests: reset the singleton local log and all keyed feed entries. */
export function _resetActivityStore(): void {
  state = null
  feedEntries.clear()
}
