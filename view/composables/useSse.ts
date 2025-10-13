export function useSse(path = '/api/events', params: Record<string, string | number | boolean> = {}) {
  const usp = new URLSearchParams()
  Object.entries(params).forEach(([k, v]) => {
    if (v === undefined || v === null) return
    usp.set(k, String(v))
  })
  const url = `${path}${usp.toString() ? '?' + usp.toString() : ''}`
  const es = new EventSource(url)
  return {
    es,
    on(event: string, handler: (e: MessageEvent) => void) { es.addEventListener(event, handler) },
    off(event: string, handler: (e: MessageEvent) => void) { es.removeEventListener(event, handler) },
    close() { es.close() },
  }
}
