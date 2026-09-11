import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'

// DEV-54 UI regression coverage: atomic create status, per-task serialized and
// coalesced autosaves, failed-save reconciliation, nullable clears, and custom
// field JSON preservation.

const api = vi.hoisted(() => ({
  getTask: vi.fn(), showConfig: vi.fn(), updateTask: vi.fn(), setStatus: vi.fn(), addTask: vi.fn(),
  listProjects: vi.fn(), listTasks: vi.fn(), sprintList: vi.fn(), taskHistory: vi.fn(), inspectConfig: vi.fn(),
  suggestTasks: vi.fn(), whoami: vi.fn(),
}))
const toast = vi.hoisted(() => vi.fn())
vi.mock('../api/client', () => ({ api }))
vi.mock('../components/toast', () => ({ showToast: toast }))

function deferred<T = any>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

function task(id: string, title = id, extra: Record<string, unknown> = {}) {
  return {
    id, title, status: 'Open', priority: 'Medium', task_type: 'Task',
    reporter: '', assignee: '', description: `${title} description`, tags: [], sprints: [],
    relationships: {}, comments: [], references: [], history: [], custom_fields: {},
    ...extra,
  }
}

function config(project: string) {
  return {
    issue_states: ['Open', 'Queued', 'Done'], issue_priorities: ['Medium', 'High'], issue_types: ['Task'],
    default_project: project, default_status: 'Open', default_priority: 'Medium',
    default_reporter: '', default_assignee: '', default_tags: [],
    tags: ['*'], custom_fields: ['product', 'points', 'meta', 'active'],
  }
}

let wrappers: VueWrapper[] = []
let currentTask: Record<string, any> = {}
function panel(taskId = 'new') {
  const wrapper = mount(TaskPanel, {
    props: { open: true, taskId, initialProject: 'A' },
    global: { stubs: { Teleport: true } }, attachTo: document.body,
  })
  wrappers.push(wrapper)
  return wrapper
}

beforeEach(() => {
  vi.resetAllMocks()
  localStorage.clear()
  api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }], total: 1 })
  api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
  api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
  api.taskHistory.mockResolvedValue([])
  api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
  api.suggestTasks.mockResolvedValue([])
  api.whoami.mockResolvedValue('')
  api.showConfig.mockImplementation(async () => config('A'))
  api.getTask.mockImplementation(async () => JSON.parse(JSON.stringify(currentTask)))
  api.updateTask.mockImplementation(async (_id: string, patch: any) => {
    currentTask = { ...JSON.parse(JSON.stringify(currentTask)), ...JSON.parse(JSON.stringify(patch)) }
    return JSON.parse(JSON.stringify(currentTask))
  })
  api.setStatus.mockImplementation(async (_id: string, status: string) => ({ ...task('A-1'), status }))
  api.addTask.mockImplementation(async (payload: any) => ({ ...task(`${payload.project}-2`), ...payload }))
})

afterEach(() => {
  wrappers.forEach(wrapper => wrapper.unmount())
  wrappers = []
  document.body.innerHTML = ''
})

describe('TaskPanel atomic create status', () => {
  it('sends the project-scoped custom status in the single addTask call', async () => {
    const wrapper = panel()
    await flushPromises()
    const vm = wrapper.vm as any
    vm.form.title = 'Atomic status task'
    vm.form.status = 'Queued'
    await vm.handleSubmit()
    await flushPromises()
    expect(api.addTask).toHaveBeenCalledTimes(1)
    expect(api.addTask.mock.calls[0]![0]).toMatchObject({ project: 'A', title: 'Atomic status task', status: 'Queued' })
    expect(api.setStatus).not.toHaveBeenCalled()
  })
})

describe('TaskPanel autosave queue', () => {
  it('serializes autosaves and coalesces fields queued while a request is in flight', async () => {
    currentTask = task('A-1')
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    const first = deferred()
    api.updateTask.mockReturnValueOnce(first.promise)

    vm.form.title = 'A edited'
    void vm.onFieldBlurBase('title')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledTimes(1)
    expect(api.updateTask.mock.calls[0]![1]).toEqual({ title: 'A edited' })

    // While the first response is pending, queue more edits: same field twice
    // (last write wins) plus a new field. Nothing may be sent yet.
    vm.form.description = 'Queued description'
    void vm.onFieldBlurBase('description')
    vm.form.title = 'A edited more'
    void vm.onFieldBlurBase('title')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledTimes(1)

    first.resolve({ ...task('A-1', 'A edited') })
    await flushPromises()

    expect(api.updateTask).toHaveBeenCalledTimes(2)
    expect(api.updateTask.mock.calls[1]![0]).toBe('A-1')
    expect(api.updateTask.mock.calls[1]![1]).toEqual({
      title: 'A edited more',
      description: 'Queued description',
    })
    expect(vm.form.title).toBe('A edited more')
    expect(vm.form.description).toBe('Queued description')
  })

  it('resolves repeated same-field status saves with their own request outcomes', async () => {
    currentTask = task('A-1')
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    const first = deferred()
    const second = deferred()
    api.updateTask.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

    let firstSettled = false
    let secondSettled = false
    const p1 = vm.updateStatus('Done').then(() => { firstSettled = true })
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('A-1', { status: 'Done' })
    expect(firstSettled).toBe(false)

    // Same field queued while the first request is in flight: must not be
    // sent yet and must not inherit the first request's outcome.
    const p2 = vm.updateStatus('Blocked').then(() => { secondSettled = true })
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledTimes(1)
    expect(secondSettled).toBe(false)

    first.resolve({ ...task('A-1'), status: 'Done' })
    await p1
    await flushPromises()
    expect(firstSettled).toBe(true)
    // The queued save fires only after the first response lands…
    expect(api.updateTask).toHaveBeenCalledTimes(2)
    expect(api.updateTask.mock.calls[1]![1]).toEqual({ status: 'Blocked' })
    // …so exactly one success toast exists and the second waiter is pending.
    expect(secondSettled).toBe(false)
    expect(toast).toHaveBeenCalledTimes(1)
    expect(toast).toHaveBeenCalledWith('Status updated')

    // The second request fails: its waiter resolves with its own failure.
    api.getTask.mockResolvedValueOnce({ ...task('A-1'), status: 'Done' })
    second.reject(new Error('second status failed'))
    await p2
    await flushPromises()
    expect(secondSettled).toBe(true)
    expect(toast).toHaveBeenCalledTimes(2)
    expect(toast).toHaveBeenLastCalledWith('second status failed')
    // Reconciliation shows the persisted server status, not the failed one.
    expect(vm.form.status).toBe('Done')
    expect(vm.task.status).toBe('Done')
  })

  it('reconciles a failed save to server state but keeps newer pending intent', async () => {
    currentTask = task('A-1')
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    const failing = deferred()
    api.updateTask.mockReturnValueOnce(failing.promise)

    vm.form.title = 'doomed edit'
    void vm.onFieldBlurBase('title')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledTimes(1)

    // Newer intent queued while the doomed request is in flight.
    vm.form.description = 'newer intent'
    void vm.onFieldBlurBase('description')

    api.getTask.mockClear()
    failing.reject(new Error('save failed'))
    await flushPromises()

    expect(toast).toHaveBeenCalledWith('save failed')
    expect(api.getTask).toHaveBeenCalledWith('A-1')
    // Failed field with no newer intent is reverted to the server value.
    expect(vm.form.title).toBe('A-1')
    expect(vm.task.title).toBe('A-1')
    // Newer pending intent survives reconciliation and is then saved.
    expect(vm.form.description).toBe('newer intent')
    expect(api.updateTask).toHaveBeenCalledTimes(2)
    expect(api.updateTask.mock.calls[1]![1]).toEqual({ description: 'newer intent' })
    expect(currentTask.description).toBe('newer intent')
  })

  it('does not overwrite uncommitted edits when reconciling a failed save', async () => {
    currentTask = task('A-1')
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    const failing = deferred()
    api.updateTask.mockReturnValueOnce(failing.promise)
    vm.form.title = 'doomed edit'
    void vm.onFieldBlurBase('title')
    await flushPromises()

    // User re-types the field before the failure arrives (not yet committed).
    vm.form.title = 'retyped newer intent'
    failing.reject(new Error('save failed'))
    await flushPromises()

    expect(vm.form.title).toBe('retyped newer intent')
    expect(vm.task.title).toBe('A-1')
  })

  it('keeps queued edits per task after switching tasks without cross-task bleed', async () => {
    currentTask = task('A-1')
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    const first = deferred()
    api.updateTask.mockReturnValueOnce(first.promise)
    vm.form.title = 'A edited'
    void vm.onFieldBlurBase('title')
    await flushPromises()

    currentTask = task('B-1')
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()

    first.resolve({ ...task('A-1', 'A edited') })
    await flushPromises()
    expect(wrapper.emitted('updated')).toBeUndefined()
    expect(toast).not.toHaveBeenCalled()
    expect(vm.form.id).toBe('B-1')
    expect(vm.form.title).toBe('B-1')

    // Editing B must go to B's own queue only.
    api.updateTask.mockClear()
    vm.form.title = 'B edited'
    void vm.onFieldBlurBase('title')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('B-1', { title: 'B edited' })
  })
})

describe('TaskPanel nullable clears', () => {
  it('clears due_date, effort, description, reporter, and assignee with explicit null', async () => {
    currentTask = task('A-1', 'A-1', {
      due_date: '2026-01-02', effort: '3h', description: 'has description', reporter: 'alice', assignee: 'bob',
    })
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    vm.form.due_date = ''
    void vm.onFieldBlurBase('due_date')
    await flushPromises()
    vm.form.effort = ''
    void vm.onFieldBlurBase('effort')
    await flushPromises()
    vm.form.description = ''
    void vm.onFieldBlurBase('description')
    await flushPromises()
    vm.form.reporter = ''
    void vm.onFieldBlurBase('reporter')
    await flushPromises()
    vm.form.assignee = ''
    void vm.onFieldBlurBase('assignee')
    await flushPromises()

    const merged: Record<string, unknown> = {}
    for (const call of api.updateTask.mock.calls) Object.assign(merged, call[1])
    expect(merged).toEqual({
      due_date: null,
      effort: null,
      description: null,
      reporter: null,
      assignee: null,
    })
    // Durable wire form: null must survive JSON serialization (never dropped
    // like undefined).
    expect(JSON.stringify(api.updateTask.mock.calls.map(call => call[1]))).toContain('"due_date":null')
  })
})

describe('TaskPanel custom field JSON preservation', () => {
  it('resends untouched non-string values verbatim and edits as strings', async () => {
    currentTask = task('A-1', 'A-1', {
      custom_fields: { product: 'Core', points: 42, meta: { a: 1 }, active: true },
    })
    const wrapper = panel('A-1')
    await flushPromises()
    const vm = wrapper.vm as any
    api.updateTask.mockClear()

    await vm.commitCustomFields()
    expect(api.updateTask).toHaveBeenCalledTimes(1)
    expect(api.updateTask.mock.calls[0]![1]!.custom_fields).toEqual({
      product: 'Core',
      points: 42,
      meta: { a: 1 },
      active: true,
    })

    api.updateTask.mockClear()
    vm.updateCustomFieldValue('product', 'Edited')
    await vm.commitCustomFields()
    expect(api.updateTask).toHaveBeenCalledTimes(1)
    const patch = api.updateTask.mock.calls[0]![1]
    expect(patch.custom_fields.product).toBe('Edited')
    expect(patch.custom_fields.points).toBe(42)
    expect(patch.custom_fields.meta).toEqual({ a: 1 })
    expect(patch.custom_fields.active).toBe(true)
  })
})
