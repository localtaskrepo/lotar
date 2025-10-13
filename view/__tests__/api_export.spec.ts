import { describe, expect, it, vi } from 'vitest'
import { api } from '../api/client'

describe('api.exportTasks', () => {
  it('calls /api/tasks/export with query params and returns Response', async () => {
    const mockRes = new Response('id,title\n', { headers: { 'Content-Type': 'text/csv' } })
    const spy = vi.spyOn(globalThis, 'fetch' as any).mockResolvedValue(mockRes as any)
    const res = await api.exportTasks({ project: 'ABC', status: ['TODO', 'DONE'], tags: ['ui', 'backend'] } as any)
    expect(spy).toHaveBeenCalled()
    const url = (spy.mock.calls[0][0] as string)
    expect(url).toContain('/api/tasks/export')
    expect(url).toContain('project=ABC')
    expect(url).toContain('status=TODO%2CDONE')
    expect(url).toContain('tags=ui%2Cbackend')
    const text = await res.text()
    expect(text).toContain('id,title')
    spy.mockRestore()
  })
})
