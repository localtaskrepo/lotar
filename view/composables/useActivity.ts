import type { ComputedRef, DeepReadonly } from 'vue'
import { computed, reactive, readonly } from 'vue'
import { api } from '../api/client'
import type { ActivityFeedItem } from '../api/types'

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

type ActivityState = {
  items: ActivityItem[]
  touches: Record<string, TaskTouch>
  feed: ActivityFeedItem[]
  feedLoading: boolean
  feedError: string | null
}

let state: ActivityState | null = null

function ensure() {
  if (!state) {
    state = reactive<ActivityState>({ items: [], touches: {}, feed: [], feedLoading: false, feedError: null })
  }
  return state!
}

function genId() {
  return Math.random().toString(36).slice(2) + Date.now().toString(36)
}

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
  async function refreshFeed(params: { since?: string; until?: string; project?: string; limit?: number } = {}) {
    if (s.feedLoading) return
    s.feedLoading = true
    s.feedError = null
    try {
      const items = await api.activityFeed(params)
      s.feed.splice(0, s.feed.length, ...items)
    } catch (err: any) {
      s.feedError = err?.message || 'Failed to load activity'
      s.feed.splice(0, s.feed.length)
    } finally {
      s.feedLoading = false
    }
  }
  const items = readonly(s.items) as DeepReadonly<ActivityItem[]>
  const touches = readonly(s.touches) as DeepReadonly<Record<string, TaskTouch>>
  const feed = computed(() => s.feed) as ComputedRef<ActivityFeedItem[]>
  const feedLoading = computed(() => s.feedLoading) as ComputedRef<boolean>
  const feedError = computed(() => s.feedError) as ComputedRef<string | null>

  return {
    items,
    touches,
    feed,
    feedLoading,
    feedError,
    add,
    clear,
    markTaskTouch,
    removeTaskTouch,
    clearTouches,
    refreshFeed,
  }
}
