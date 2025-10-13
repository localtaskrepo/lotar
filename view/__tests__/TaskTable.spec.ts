import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import TaskTable from '../components/TaskTable.vue'

const tasks = [
  { id: 'PRJ-2', title: 'Bravo', status: 'open', priority: 'med', task_type: 'task', assignee: 'bob', created: '2025-08-19T10:00:00Z', modified: '2025-08-20T10:00:00Z', tags: ['b'], relationships: {}, comments: [], custom_fields: {} },
  { id: 'PRJ-1', title: 'Alpha', status: 'in-progress', priority: 'low', task_type: 'task', assignee: 'alice', created: '2025-08-20T10:00:00Z', modified: '2025-08-21T10:00:00Z', tags: ['a'], relationships: {}, comments: [], custom_fields: {} },
]

describe('TaskTable', () => {
  beforeEach(() => { localStorage.clear() })

  it('sorts by title asc then desc when clicking header', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, statuses: ['open', 'in-progress', 'done'] } })
    const headers = wrapper.findAll('th')
    // First non-selectable header is ID; second is Title
    const titleHeader = headers.find(h => h.text().toLowerCase().includes('title'))!
    await titleHeader.trigger('click') // asc
    const firstRow = wrapper.findAll('tbody tr')[0]
    expect(firstRow.text()).toContain('Alpha')
    await titleHeader.trigger('click') // desc
    const firstRowDesc = wrapper.findAll('tbody tr')[0]
    expect(firstRowDesc.text()).toContain('Bravo')
  })

  it('persists column toggles in localStorage', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, projectKey: 'PRJ' } })
    // open popover
    await wrapper.find('button.btn').trigger('click')
    const checks = wrapper.findAll('input[type="checkbox"]')
    // uncheck Tags column
    const tagsCheckbox = checks.find(c => c.element.nextSibling && (c.element.nextSibling as any).textContent?.includes('Tags'))!
    await tagsCheckbox.setValue(false)
    // close
    const buttons = wrapper.findAll('button.btn')
    await buttons[1].trigger('click')
    const saved = JSON.parse(localStorage.getItem('lotar.taskTable.columns::PRJ') || '[]')
    expect(saved).toBeInstanceOf(Array)
    expect(saved).not.toContain('tags')
  })

  it('uses per-project sort keys', async () => {
    localStorage.clear()
    const w1 = mount(TaskTable, { props: { tasks, projectKey: 'P1' } })
    const headers1 = w1.findAll('th')
    const titleHeader1 = headers1.find(h => h.text().toLowerCase().includes('title'))!
    await titleHeader1.trigger('click') // set P1 sort
    const w2 = mount(TaskTable, { props: { tasks, projectKey: 'P2' } })
    const headers2 = w2.findAll('th')
    const statusHeader2 = headers2.find(h => h.text().toLowerCase().includes('status'))!
    await statusHeader2.trigger('click') // set P2 sort
    const s1 = JSON.parse(localStorage.getItem('lotar.taskTable.sort::P1') || '{}')
    const s2 = JSON.parse(localStorage.getItem('lotar.taskTable.sort::P2') || '{}')
    expect(s1.key).toBe('title')
    expect(s2.key).toBe('status')
  })
})
