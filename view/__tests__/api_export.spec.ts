import { describe, expect, it, vi } from 'vitest'
import { api } from '../api/client'

describe('api.exportTasks', () => {
  it('calls /api/tasks/export with query params and returns Response', async () => {
    const mockRes = new Response('id,title\n', { headers: { 'Content-Type': 'text/csv' } })
    const spy = vi.spyOn(globalThis, 'fetch' as any).mockResolvedValue(mockRes as any)
    const res = await api.exportTasks({ project: 'ABC', status: ['TODO', 'DONE'], tags: ['ui', 'backend'] } as any)
    expect(spy).toHaveBeenCalled()
    const url = (spy.mock.calls[0]![0] as string)
    expect(url).toContain('/api/tasks/export')
    expect(url).toContain('project=ABC')
    expect(url).toContain('status=TODO%2CDONE')
    expect(url).toContain('tags=ui%2Cbackend')
    const text = await res.text()
    expect(text).toContain('id,title')
    spy.mockRestore()
  })

  it('forwards the full filter set: sprints, assignee, custom fields, smart filters, order, and sort_by', async () => {
    const mockRes = new Response('id,title\n', { headers: { 'Content-Type': 'text/csv' } })
    const spy = vi.spyOn(globalThis, 'fetch' as any).mockResolvedValue(mockRes as any)
    await api.exportTasks({
      project: 'ABC',
      sprints: [3, 7],
      assignee: '__none__',
      custom_fields: { Risk: 'high', Team: ['a', 'b'] },
      due: 'today',
      recent: '7d',
      needs: 'effort,due',
      order: 'asc',
      sort_by: 'custom:Risk',
      component: 'ui',
    } as any)
    const url = (spy.mock.calls[0]![0] as string)
    expect(url).toContain('sprints=3%2C7')
    expect(url).toContain('assignee=__none__')
    expect(url).toContain('field%3ARisk=high')
    expect(url).toContain('field%3ATeam=a%2Cb')
    expect(url).toContain('due=today')
    expect(url).toContain('recent=7d')
    expect(url).toContain('needs=effort%2Cdue')
    expect(url).toContain('order=asc')
    expect(url).toContain('sort_by=custom%3ARisk')
    expect(url).toContain('component=ui')
    spy.mockRestore()
  })

  it('maps task_type and text_query spellings onto the wire keys', async () => {
    const mockRes = new Response('id,title\n', { headers: { 'Content-Type': 'text/csv' } })
    const spy = vi.spyOn(globalThis, 'fetch' as any).mockResolvedValue(mockRes as any)
    await api.exportTasks({ task_type: ['bug'], text_query: 'x' } as any)
    const url = (spy.mock.calls[0]![0] as string)
    expect(url).toContain('type=bug')
    expect(url).toContain('q=x')
    expect(url).not.toContain('task_type')
    expect(url).not.toContain('text_query')
    spy.mockRestore()
  })
})
