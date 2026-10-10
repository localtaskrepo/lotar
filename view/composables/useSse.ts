export interface SseOptions {
  /**
   * Called when the underlying EventSource (re)opens AFTER the first
   * successful connection. Use it to refresh state that may have missed
   * events while the connection was down. Optional; existing callers that
   * pass only URL params are unaffected.
   */
  onReconnect?: () => void
}

const connections = new Map<string, { es: EventSource; consumers: number; opens: number }>()

export function useSse(path = '/api/events', params: Record<string, string | number | boolean> = {}, opts: SseOptions = {}) {
  const usp = new URLSearchParams()
  Object.entries(params).forEach(([k, v]) => {
    if (v === undefined || v === null) return
    usp.set(k, String(v))
  })
  // One unfiltered stream per scope avoids exhausting the browser's per-host
  // socket pool. Project/debounce/other server parameters still isolate streams.
  const sharedEvents = path === '/api/events'
  const kinds = sharedEvents ? usp.get('kinds') ?? usp.get('topic') : null
  const allowedKinds = kinds === null ? null : new Set(kinds.split(',').map((kind) => kind.trim().toLowerCase()).filter(Boolean))
  if (sharedEvents) {
    usp.delete('kinds')
    usp.delete('topic')
    usp.set('ready', 'true')
  }
  usp.sort()
  const url = `${path}${usp.toString() ? '?' + usp.toString() : ''}`
  let connection = connections.get(url)
  if (!connection || connection.es.readyState === EventSource.CLOSED) {
    connection = { es: new EventSource(url), consumers: 0, opens: 0 }
    const created = connection
    created.es.addEventListener('open', () => { created.opens += 1 })
    connections.set(url, created)
  }
  const shared = connection
  const es = shared.es
  shared.consumers += 1
  let closed = false
  let hasOpened = es.readyState === EventSource.OPEN
  const reconnect = () => {
    if (hasOpened) opts.onReconnect?.()
    hasOpened = true
  }
  es.addEventListener('open', reconnect)
  const listeners = new Map<string, Map<(e: MessageEvent) => void, EventListener>>()

  function off(event: string, handler: (e: MessageEvent) => void) {
    const bindings = listeners.get(event)
    const wrapped = bindings?.get(handler)
    if (wrapped) es.removeEventListener(event, wrapped)
    bindings?.delete(handler)
  }

  return {
    es,
    on(event: string, handler: (e: MessageEvent) => void) {
      if (closed) return
      if (allowedKinds && !['open', 'error', 'ready'].includes(event) && !allowedKinds.has(event.toLowerCase())) return
      let bindings = listeners.get(event)
      if (!bindings) {
        bindings = new Map()
        listeners.set(event, bindings)
      }
      if (bindings.has(handler)) return
      const wrapped: EventListener = (ev) => handler(ev as MessageEvent)
      bindings.set(handler, wrapped)
      es.addEventListener(event, wrapped)
      // A page can join a stream opened by the app shell. Preserve its initial
      // open reconciliation without dispatching another open to existing owners.
      if (event === 'open' && es.readyState === EventSource.OPEN) {
        const opens = shared.opens
        queueMicrotask(() => {
          if (!closed && bindings.get(handler) === wrapped && shared.opens === opens && es.readyState === EventSource.OPEN) {
            wrapped(new Event('open'))
          }
        })
      }
    },
    off,
    close() {
      if (closed) return
      closed = true
      es.removeEventListener('open', reconnect)
      for (const [event, bindings] of listeners) {
        for (const wrapped of bindings.values()) es.removeEventListener(event, wrapped)
      }
      listeners.clear()
      shared.consumers -= 1
      if (shared.consumers === 0) {
        es.close()
        if (connections.get(url) === shared) connections.delete(url)
      }
    },
  }
}
