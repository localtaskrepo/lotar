import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSse } from '../composables/useSse'

class FakeEventSource extends EventTarget {
  static instances: FakeEventSource[] = []
  static OPEN = 1
  static CLOSED = 2
  readyState = 0
  close = vi.fn(() => { this.readyState = 2 })

  constructor(public url: string) {
    super()
    FakeEventSource.instances.push(this)
  }

  open() {
    this.readyState = 1
    this.dispatchEvent(new Event('open'))
  }
}

describe('shared SSE subscriptions', () => {
  const handles: ReturnType<typeof useSse>[] = []
  function subscribe(...args: Parameters<typeof useSse>) {
    const handle = useSse(...args)
    handles.push(handle)
    return handle
  }

  beforeEach(() => {
    FakeEventSource.instances = []
    vi.stubGlobal('EventSource', FakeEventSource)
  })

  afterEach(() => {
    handles.splice(0).forEach((handle) => handle.close())
    vi.unstubAllGlobals()
  })

  it('shares task, config and sync kinds on one connection and routes only subscribed kinds', () => {
    const task = subscribe('/api/events', { kinds: 'task_updated', ready: true })
    const config = subscribe('/api/events', { kinds: 'config_updated' })
    const sync = subscribe('/api/events', { kinds: 'sync_progress' })
    expect(FakeEventSource.instances).toHaveLength(1)
    const source = FakeEventSource.instances[0]!
    expect(new URL(source.url, 'http://localhost').searchParams.has('kinds')).toBe(false)
    const taskHandler = vi.fn()
    const configHandler = vi.fn()
    const unwantedHandler = vi.fn()
    task.on('task_updated', taskHandler)
    config.on('config_updated', configHandler)
    sync.on('task_updated', unwantedHandler)
    source.dispatchEvent(new MessageEvent('task_updated', { data: '{"id":"DEV-1"}' }))
    expect(taskHandler).toHaveBeenCalledOnce()
    expect(taskHandler.mock.calls[0]![0].data).toBe('{"id":"DEV-1"}')
    expect(configHandler).not.toHaveBeenCalled()
    expect(unwantedHandler).not.toHaveBeenCalled()
    source.dispatchEvent(new MessageEvent('config_updated', { data: '{}' }))
    expect(configHandler).toHaveBeenCalledOnce()
  })

  it('releases only its own handlers, even when another owner registered the same function', () => {
    const first = subscribe()
    const second = subscribe()
    const handler = vi.fn()
    first.on('task_updated', handler)
    second.on('task_updated', handler)
    first.close()
    first.close()
    const source = FakeEventSource.instances[0]!
    expect(source.close).not.toHaveBeenCalled()
    source.dispatchEvent(new MessageEvent('task_updated'))
    expect(handler).toHaveBeenCalledOnce()
    second.off('task_updated', handler)
    source.dispatchEvent(new MessageEvent('task_updated'))
    expect(handler).toHaveBeenCalledOnce()
    second.close()
    expect(source.close).toHaveBeenCalledOnce()
    const replacement = subscribe()
    expect(FakeEventSource.instances).toHaveLength(2)
    expect(replacement.es).not.toBe(source)
  })

  it('reconciles a late subscriber on open and reconnects only active consumers', async () => {
    const earlyReconnect = vi.fn()
    const early = subscribe('/api/events', {}, { onReconnect: earlyReconnect })
    const source = FakeEventSource.instances[0]!
    source.open()
    expect(earlyReconnect).not.toHaveBeenCalled()
    const lateReconnect = vi.fn()
    const late = subscribe('/api/events', {}, { onReconnect: lateReconnect })
    const opened = vi.fn()
    late.on('open', opened)
    await Promise.resolve()
    expect(opened).toHaveBeenCalledOnce()
    expect(lateReconnect).not.toHaveBeenCalled()
    source.open()
    expect(opened).toHaveBeenCalledTimes(2)
    expect(earlyReconnect).toHaveBeenCalledOnce()
    expect(lateReconnect).toHaveBeenCalledOnce()
    early.close()
    source.open()
    expect(earlyReconnect).toHaveBeenCalledOnce()
    expect(lateReconnect).toHaveBeenCalledTimes(2)
  })

  it('does not deliver queued opens to removed or closed listeners', async () => {
    subscribe()
    FakeEventSource.instances[0]!.open()
    const removed = subscribe()
    const removedHandler = vi.fn()
    removed.on('open', removedHandler)
    removed.off('open', removedHandler)
    const closed = subscribe()
    const closedHandler = vi.fn()
    closed.on('open', closedHandler)
    closed.close()
    await Promise.resolve()
    expect(removedHandler).not.toHaveBeenCalled()
    expect(closedHandler).not.toHaveBeenCalled()
  })

  it('preserves server-side project and debounce isolation and parameter-order independence', () => {
    subscribe('/api/events', { project: 'DEV', debounce_ms: 10, kinds: 'task_updated' })
    subscribe('/api/events', { kinds: 'config_updated', debounce_ms: 10, project: 'DEV' })
    subscribe('/api/events', { project: 'OTHER', debounce_ms: 10 })
    subscribe('/api/events', { project: 'DEV', debounce_ms: 20 })
    expect(FakeEventSource.instances).toHaveLength(3)
    const query = new URL(FakeEventSource.instances[0]!.url, 'http://localhost').searchParams
    expect(query.get('project')).toBe('DEV')
    expect(query.get('debounce_ms')).toBe('10')
  })

  it('replaces permanently closed sources without an old owner evicting the replacement', () => {
    const old = subscribe()
    const failed = FakeEventSource.instances[0]!
    failed.readyState = FakeEventSource.CLOSED
    failed.dispatchEvent(new Event('error'))
    const fresh = subscribe()
    expect(FakeEventSource.instances).toHaveLength(2)
    expect(fresh.es).not.toBe(failed)
    old.close()
    const another = subscribe()
    expect(another.es).toBe(fresh.es)
    expect(FakeEventSource.instances).toHaveLength(2)
  })
})
