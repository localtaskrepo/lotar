export interface SseOptions {
  /**
   * Called when the underlying EventSource (re)opens AFTER the first
   * successful connection. Use it to refresh state that may have missed
   * events while the connection was down. Optional; existing callers that
   * pass only URL params are unaffected.
   */
  onReconnect?: () => void
}

export function useSse(path = '/api/events', params: Record<string, string | number | boolean> = {}, opts: SseOptions = {}) {
  const usp = new URLSearchParams()
  Object.entries(params).forEach(([k, v]) => {
    if (v === undefined || v === null) return
    usp.set(k, String(v))
  })
  const url = `${path}${usp.toString() ? '?' + usp.toString() : ''}`
  const es = new EventSource(url)
  let hasOpened = false
  if (opts.onReconnect) {
    es.addEventListener('open', () => {
      if (!hasOpened) {
        hasOpened = true
        return
      }
      opts.onReconnect!()
    })
  }
  return {
    es,
    on(event: string, handler: (e: MessageEvent) => void) { es.addEventListener(event, handler) },
    off(event: string, handler: (e: MessageEvent) => void) { es.removeEventListener(event, handler) },
    close() { es.close() },
  }
}
