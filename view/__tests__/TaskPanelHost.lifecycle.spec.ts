import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import TaskPanelHost from '../components/TaskPanelHost.vue'

const toastMock = vi.hoisted(() => vi.fn())

vi.mock('../api/client', () => ({
  api: {
    listTasks: vi.fn(async () => ({ total: 0, limit: 200, offset: 0, tasks: [] })),
    getTask: vi.fn(),
    addTask: vi.fn(),
    updateTask: vi.fn(),
    deleteTask: vi.fn(),
    restoreTask: vi.fn(),
  },
}))

vi.mock('../components/toast', () => ({
  showToast: toastMock,
}))

const { useTaskStore, _resetTaskStore } = await import('../composables/useTaskStore')
const { useTaskPanelController } = await import('../composables/useTaskPanelController')

const TaskPanelStub = {
  name: 'TaskPanel',
  props: ['open', 'taskId', 'lifecycleReload'],
  template: '<div data-testid="panel-stub" />',
}

function makeTask(id: string, overrides: Record<string, unknown> = {}) {
  return {
    id,
    title: `Task ${id}`,
    status: 'Open',
    priority: 'Medium',
    task_type: 'Task',
    created: '2026-01-01T00:00:00Z',
    modified: '2026-01-01T00:00:00Z',
    tags: [],
    relationships: {},
    comments: [],
    references: [],
    sprints: [],
    history: [],
    custom_fields: {},
    ...overrides,
  }
}

describe('TaskPanelHost external lifecycle reactivity (DEV-92)', () => {
  let store: ReturnType<typeof useTaskStore>
  let controller: ReturnType<typeof useTaskPanelController>
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let wrapper: any

  const stubProps = () => wrapper.findComponent({ name: 'TaskPanel' }).props()

  beforeEach(() => {
    vi.clearAllMocks()
    _resetTaskStore()
    store = useTaskStore()
    controller = useTaskPanelController()
    controller.closeTaskPanel()
    wrapper = mount(TaskPanelHost, {
      global: {
        stubs: { TaskPanel: TaskPanelStub },
      },
    })
  })

  afterEach(() => {
    controller.closeTaskPanel()
    wrapper.unmount()
  })

  it('an external soft deletion of the open task requests an authoritative reload and stays open', async () => {
    store.upsert(makeTask('A-1'))
    controller.openTaskPanel({ taskId: 'A-1' })
    await nextTick()
    expect(stubProps().open).toBe(true)
    expect(stubProps().lifecycleReload).toBe(0)

    // External soft delete reaches the shared store (e.g. via SSE).
    store.upsert(makeTask('A-1', { deleted_at: '2026-10-05T10:00:00Z' }))
    await nextTick()

    expect(stubProps().open).toBe(true)
    expect(stubProps().lifecycleReload).toBe(1)
    expect(toastMock).not.toHaveBeenCalledWith(expect.stringContaining('permanently deleted'))
  })

  it('an external hard deletion of the open task closes the panel with a toast', async () => {
    store.upsert(makeTask('A-1'))
    controller.openTaskPanel({ taskId: 'A-1' })
    await nextTick()

    store.evict('A-1')
    await nextTick()

    expect(controller.state.open).toBe(false)
    expect(stubProps().open).toBe(false)
    expect(toastMock).toHaveBeenCalledWith(expect.stringContaining('permanently deleted'))
  })

  it('opening an already-deleted task deliberately does not auto-close or reload', async () => {
    store.upsert(makeTask('A-1', { deleted_at: '2026-10-05T10:00:00Z' }))
    controller.openTaskPanel({ taskId: 'A-1' })
    await nextTick()

    expect(controller.state.open).toBe(true)
    expect(stubProps().lifecycleReload).toBe(0)
    expect(toastMock).not.toHaveBeenCalled()
  })

  it('an external restore of the open trash row requests a reconcile reload', async () => {
    store.upsert(makeTask('A-1', { deleted_at: '2026-10-05T10:00:00Z' }))
    controller.openTaskPanel({ taskId: 'A-1' })
    await nextTick()
    expect(stubProps().lifecycleReload).toBe(0)

    store.upsert(makeTask('A-1'), { restore: true })
    await nextTick()

    expect(controller.state.open).toBe(true)
    expect(stubProps().lifecycleReload).toBe(1)
  })

  it('lifecycle writes for unrelated tasks never reload or close the open panel', async () => {
    store.upsert(makeTask('A-1'))
    controller.openTaskPanel({ taskId: 'A-1' })
    await nextTick()

    store.upsert(makeTask('B-9', { deleted_at: '2026-10-05T10:00:00Z' }))
    store.upsert(makeTask('B-9'), { restore: true })
    store.evict('B-9')
    store.upsert(makeTask('B-8'))
    await nextTick()

    expect(controller.state.open).toBe(true)
    expect(stubProps().lifecycleReload).toBe(0)
    expect(toastMock).not.toHaveBeenCalled()
  })
})
