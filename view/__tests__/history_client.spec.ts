import { describe, expect, it, vi } from 'vitest'
import { api } from '../api/client'

describe('api client history endpoints', () => {
  it('calls /api/tasks/history with id and limit and unwraps the envelope', async () => {
    const entries = [
      { commit: 'abc123', author: 'Alice', email: 'alice@example.com', date: '2026-01-02T03:04:05Z', message: 'do a thing' },
      { commit: 'def456', author: 'Bob', email: 'bob@example.com', date: '2026-01-03T03:04:05Z', message: 'do another thing' },
    ]
    const mockRes = new Response(JSON.stringify({ data: entries }), { headers: { 'Content-Type': 'application/json' } })
    const spy = vi.spyOn(globalThis, 'fetch' as any).mockResolvedValue(mockRes as any)

    const items = await api.taskHistory('ABC-1', 10)

    expect(spy).toHaveBeenCalledTimes(1)
    const url = (spy.mock.calls[0]![0] as string)
    expect(url).toBe('/api/tasks/history?id=ABC-1&limit=10')
    const init = (spy.mock.calls[0]![1] as RequestInit)
    expect(init.method).toBeUndefined()
    expect((init.headers as Record<string, string>)['Accept']).toBe('application/json')
    expect(items).toEqual(entries)
    spy.mockRestore()
  })
})
