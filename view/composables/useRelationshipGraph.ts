import { computed, onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue'
import { api } from '../api/client'
import type { RelationshipGraphRequest, RelationshipGraphResponse } from '../api/types'
import { useSse } from './useSse'

export function useRelationshipGraph(query: Ref<RelationshipGraphRequest>) {
  const graph = ref<RelationshipGraphResponse | null>(null)
  const loading = ref(false)
  const error = ref('')
  const acceptedQuery = ref<RelationshipGraphRequest | null>(null)
  const stale = computed(() => !!graph.value && (!!error.value || JSON.stringify(acceptedQuery.value) !== JSON.stringify(query.value)))
  let generation = 0
  let alive = true
  let stream: ReturnType<typeof useSse> | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  async function refresh() {
    const request = ++generation
    loading.value = true
    error.value = ''
    try {
      const snapshot = { ...query.value, ...(query.value.filters ? { filters: { ...query.value.filters } } : {}) }
      const { depth, max_nodes } = snapshot
      if (depth !== undefined && (!Number.isInteger(depth) || depth < 0 || depth > 8)) throw new Error('Depth must be an integer between 0 and 8.')
      if (max_nodes !== undefined && (!Number.isInteger(max_nodes) || max_nodes < 1 || max_nodes > 500)) throw new Error('Node limit must be an integer between 1 and 500.')
      const result = await api.relationshipGraph(snapshot)
      if (alive && request === generation) { acceptedQuery.value = snapshot; graph.value = result }
    } catch (cause) {
      if (alive && request === generation) {
        error.value = cause instanceof Error ? cause.message : 'Could not load dependency view'
      }
    } finally { if (alive && request === generation) loading.value = false }
  }
  watch(query, () => { void refresh() }, { immediate: true, deep: true })
  function scheduleRefresh() {
    clearTimeout(timer)
    timer = setTimeout(() => { void refresh() }, 120)
  }
  onMounted(() => {
    if (typeof EventSource === 'undefined') return
    stream = useSse('/api/events', {}, { onReconnect: scheduleRefresh })
    // An outside task can introduce an incoming edge. Cached membership is not
    // a safe event filter; the bounded graph endpoint decides membership.
    for (const event of ['task_created', 'task_updated', 'task_deleted', 'task_restored', 'task_error']) stream.on(event, scheduleRefresh)
  })
  onBeforeUnmount(() => {
    alive = false
    generation += 1
    clearTimeout(timer)
    stream?.close()
  })
  return { graph, loading, error, refresh, acceptedQuery, stale }
}
