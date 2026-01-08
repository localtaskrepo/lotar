import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import TaskTable from '../components/TaskTable.vue'

const tasks = [
  {
    id: 'PRJ-2',
    title: 'Bravo',
    status: 'open',
    priority: 'med',
    task_type: 'task',
    assignee: 'bob',
    created: '2025-08-19T10:00:00Z',
    modified: '2025-08-20T10:00:00Z',
    tags: ['b'],
    relationships: {},
    comments: [],
    custom_fields: {},
    sprints: [],
    references: [],
    history: [],
  },
  {
    id: 'PRJ-1',
    title: 'Alpha',
    status: 'in-progress',
    priority: 'low',
    task_type: 'task',
    assignee: 'alice',
    created: '2025-08-20T10:00:00Z',
    modified: '2025-08-21T10:00:00Z',
    tags: ['a'],
    relationships: {},
    comments: [],
    custom_fields: {},
    sprints: [],
    references: [],
    history: [],
  },
]

describe('TaskTable', () => {
  beforeEach(() => { localStorage.clear() })

  function makeDataTransfer() {
    const store: Record<string, string> = {}
    return {
      effectAllowed: 'move',
      setData: (type: string, value: string) => { store[type] = value },
      getData: (type: string) => store[type] || '',
    } as any
  }

  it('sorts by title asc then desc when clicking header', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, statuses: ['open', 'in-progress', 'done'] } })
    const headers = wrapper.findAll('th')
    const titleHeader = headers.find(h => h.text().toLowerCase().includes('title'))!
    const titleButton = titleHeader.find('button.header-button')
    await titleButton.trigger('click') // asc
    const firstRow = wrapper.findAll('tbody tr')[0]
    expect(firstRow.text()).toContain('Alpha')
    await titleButton.trigger('click') // desc
    const firstRowDesc = wrapper.findAll('tbody tr')[0]
    expect(firstRowDesc.text()).toContain('Bravo')
  })

  it('persists column toggles in localStorage', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, projectKey: 'PRJ' } })
    // open popover
    const columnsButton = wrapper.findAll('button.btn').find((b) => b.text().includes('Columns'))!
    await columnsButton.trigger('click')
    const checks = wrapper.findAll('input[type="checkbox"]')
    // uncheck Tags column
    const tagsCheckbox = checks.find(c => c.element.nextSibling && (c.element.nextSibling as any).textContent?.includes('Tags'))!
    await tagsCheckbox.setValue(false)
    // close
    const closeButton = wrapper.findAll('button.btn').find((b) => b.text().trim() === 'Close')!
    await closeButton.trigger('click')
    const saved = JSON.parse(localStorage.getItem('lotar.taskTable.columns::PRJ') || '[]')
    expect(saved).toBeInstanceOf(Array)
    expect(saved).not.toContain('tags')
  })

  it('reorders columns via drag and drop and persists', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, projectKey: 'PRJ' } })

    const labels = () =>
      wrapper
        .findAll('button.header-button .header-button__label')
        .map((n) => n.text())

    const before = labels()
    expect(before).toContain('Priority')
    expect(before).toContain('Status')

    const buttons = wrapper.findAll('button.header-button')
    const priorityButton = buttons.find((b) => b.text().includes('Priority'))!
    const statusButton = buttons.find((b) => b.text().includes('Status'))!
    const statusTh = wrapper.findAll('th').find((th) => th.text().includes('Status'))!

    const dt = makeDataTransfer()
    await priorityButton.trigger('dragstart', { dataTransfer: dt })
    await statusTh.trigger('dragover', { dataTransfer: dt, clientX: 0 })
    await statusTh.trigger('drop', { dataTransfer: dt, clientX: 0 })

    const after = labels()
    expect(after.indexOf('Priority')).toBeLessThan(after.indexOf('Status'))

    const savedOrder = JSON.parse(localStorage.getItem('lotar.taskTable.columnOrder::PRJ') || '[]')
    expect(savedOrder.indexOf('priority')).toBeGreaterThanOrEqual(0)
    expect(savedOrder.indexOf('status')).toBeGreaterThanOrEqual(0)
    expect(savedOrder.indexOf('priority')).toBeLessThan(savedOrder.indexOf('status'))
  })

  it('reorders columns via the columns menu drag and drop', async () => {
    const wrapper = mount(TaskTable, { props: { tasks, projectKey: 'PRJ' } })

    await wrapper.find('button.btn').trigger('click')
    const rows = wrapper.findAll('.columns-popover label')

    const priorityRow = rows.find((row) => row.text().includes('Priority'))!
    const statusRow = rows.find((row) => row.text().includes('Status'))!

    const dt = makeDataTransfer()
    await priorityRow.trigger('dragstart', { dataTransfer: dt })
    await statusRow.trigger('dragover', { dataTransfer: dt, clientX: 0 })
    await statusRow.trigger('drop', { dataTransfer: dt, clientX: 0 })

    const savedOrder = JSON.parse(localStorage.getItem('lotar.taskTable.columnOrder::PRJ') || '[]')
    expect(savedOrder.indexOf('priority')).toBeLessThan(savedOrder.indexOf('status'))
  })

  it('keeps column order stable across toggles', async () => {
    localStorage.setItem('lotar.taskTable.columnOrder::PRJ', JSON.stringify([
      'id',
      'title',
      'priority',
      'status',
      'assignee',
      'tags',
      'modified',
      'reporter',
      'sprints',
      'due_date',
      'task_type',
      'effort',
    ]))

    const wrapper = mount(TaskTable, { props: { tasks, projectKey: 'PRJ' } })

    const labels = () =>
      wrapper
        .findAll('button.header-button .header-button__label')
        .map((n) => n.text())

    const before = labels()
    expect(before.indexOf('Priority')).toBeLessThan(before.indexOf('Status'))

    await wrapper.find('button.btn').trigger('click')
    const checks = wrapper.findAll('input[type="checkbox"]')
    const priorityCheckbox = checks.find(c => (c.element.nextSibling as any)?.textContent?.includes('Priority'))!

    await priorityCheckbox.setValue(false)
    await priorityCheckbox.setValue(true)

    const after = labels()
    expect(after.indexOf('Priority')).toBeLessThan(after.indexOf('Status'))
  })

  it('uses per-project sort keys', async () => {
    localStorage.clear()
    const w1 = mount(TaskTable, { props: { tasks, projectKey: 'P1' } })
    const headers1 = w1.findAll('th')
    const titleHeader1 = headers1.find(h => h.text().toLowerCase().includes('title'))!
    await titleHeader1.find('button.header-button').trigger('click') // set P1 sort
    const w2 = mount(TaskTable, { props: { tasks, projectKey: 'P2' } })
    const headers2 = w2.findAll('th')
    const statusHeader2 = headers2.find(h => h.text().toLowerCase().includes('status'))!
    await statusHeader2.find('button.header-button').trigger('click') // set P2 sort
    const s1 = JSON.parse(localStorage.getItem('lotar.taskTable.sort::P1') || '{}')
    const s2 = JSON.parse(localStorage.getItem('lotar.taskTable.sort::P2') || '{}')
    expect(s1.key).toBe('title')
    expect(s2.key).toBe('status')
  })

  it('renders sprint chips with non-breaking labels', () => {
    const sprintTasks = [
      {
        ...tasks[0],
        sprints: [42],
      },
    ]
    const wrapper = mount(TaskTable, {
      props: {
        tasks: sprintTasks,
        sprintLookup: {
          42: { label: '#42 Ultra Long Sprint Name For Preview', state: 'active' },
        },
        statuses: ['open'],
      },
    })
    const chip = wrapper.find('.sprint-chip')
    expect(chip.exists()).toBe(true)
    expect(chip.text()).toContain('\u00a0')
    expect(chip.attributes('title')).toContain('#42 Ultra Long Sprint Name For Preview')
  })
})
