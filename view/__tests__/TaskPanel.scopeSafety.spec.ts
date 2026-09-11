import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'

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

function task(id: string, title = id) {
  return { id, title, status: 'Open', priority: 'Medium', task_type: 'Task',
    reporter: '', assignee: '', description: `${title} description`, tags: [], sprints: [],
    relationships: {}, comments: [], references: [], history: [], custom_fields: {},
  }
}

function config(project: string) {
  return { issue_states: ['Open', 'Done'], issue_priorities: ['Medium', 'High'], issue_types: ['Task'],
    default_project: project, default_status: 'Open', default_priority: 'Medium',
    default_reporter: `${project}-reporter`, default_assignee: `${project}-owner`, default_tags: [`${project}-tag`],
    tags: ['*'], custom_fields: ['product'],
  }
}

let wrappers: VueWrapper[] = []
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
  api.listProjects.mockResolvedValue({ projects: [{ prefix: 'A' }, { prefix: 'B' }], total: 2 })
  api.listTasks.mockResolvedValue({ tasks: [], total: 0 })
  api.sprintList.mockResolvedValue({ sprints: [], missing_sprints: [] })
  api.taskHistory.mockResolvedValue([])
  api.inspectConfig.mockResolvedValue({ effective: { remotes: {} } })
  api.suggestTasks.mockResolvedValue([])
  api.whoami.mockResolvedValue('')
  api.showConfig.mockImplementation(async project => config(project))
  api.getTask.mockImplementation(async id => task(id))
  api.updateTask.mockImplementation(async (id, patch) => ({ ...task(id), ...patch }))
  api.setStatus.mockImplementation(async (id, status) => ({ ...task(id), status }))
  api.addTask.mockImplementation(async payload => ({ ...task(`${payload.project}-2`), ...payload }))
})

afterEach(() => {
  wrappers.forEach(wrapper => wrapper.unmount())
  wrappers = []
  document.body.innerHTML = ''
})

describe('TaskPanel with real config and persistence', () => {
  it('keeps B fields and B mutation ID when A config completes after B has loaded', async () => {
    const fetchA = deferred()
    const fetchB = deferred()
    const configA = deferred()
    api.getTask.mockReturnValueOnce(fetchA.promise).mockReturnValueOnce(fetchB.promise)
    api.showConfig.mockImplementation(project => project === 'A' ? configA.promise : Promise.resolve(config('B')))
    const wrapper = panel('A-1')
    await flushPromises()
    fetchA.resolve(task('A-1', 'A content'))
    await flushPromises()
    expect(api.showConfig).toHaveBeenCalledWith('A')
    await wrapper.setProps({ taskId: 'B-1' })
    fetchB.resolve(task('B-1', 'B content'))
    await flushPromises()
    expect(wrapper.get('input[placeholder="Title"]').element).toHaveProperty('value', 'B content')
    configA.resolve(config('A'))
    await flushPromises()
    const vm = wrapper.vm as any
    expect(vm.task.id).toBe('B-1')
    expect(vm.form.id).toBe('B-1')
    expect(vm.form.description).toBe('B content description')
    await wrapper.get('input[placeholder="Title"]').setValue('B edited')
    await wrapper.get('input[placeholder="Title"]').trigger('blur')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('B-1', { title: 'B edited' })
  })

  it.each(['resolve', 'reject'] as const)('ignores stale A fetch %s while B config is pending', async outcome => {
    const fetchA = deferred()
    const configB = deferred()
    api.getTask.mockReturnValueOnce(fetchA.promise)
    api.showConfig.mockImplementation(project => project === 'B' ? configB.promise : Promise.resolve(config('A')))
    const wrapper = panel('A-1')
    await flushPromises()
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()
    if (outcome === 'resolve') fetchA.resolve(task('A-1'))
    else fetchA.reject(new Error('obsolete A failure'))
    await flushPromises()
    expect(wrapper.find('.task-panel__loading').exists()).toBe(true)
    expect(toast).not.toHaveBeenCalled()
    expect(api.showConfig).not.toHaveBeenCalledWith('A')
    configB.resolve(config('B'))
    await flushPromises()
    expect((wrapper.vm as any).form.id).toBe('B-1')
  })

  it.each(['resolve', 'reject'] as const)('ignores stale autosave %s after switching to B', async outcome => {
    const wrapper = panel('A-1')
    await flushPromises()
    const save = deferred()
    api.updateTask.mockReturnValueOnce(save.promise)
    await wrapper.get('input[placeholder="Title"]').setValue('A edited')
    await wrapper.get('input[placeholder="Title"]').trigger('blur')
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()
    if (outcome === 'resolve') save.resolve(task('A-1', 'A edited'))
    else save.reject(new Error('obsolete save failure'))
    await flushPromises()
    expect((wrapper.vm as any).form.id).toBe('B-1')
    expect(wrapper.get('input[placeholder="Title"]').element).toHaveProperty('value', 'B-1')
    expect(wrapper.emitted('updated')).toBeUndefined()
    expect(toast).not.toHaveBeenCalled()
  })

  it('blocks actual create submission during pending/failed B config, preserves entered fields, and retries with B defaults', async () => {
    const wrapper = panel()
    await flushPromises()
    const vm = wrapper.vm as any
    await wrapper.get('input[placeholder="Title"]').setValue('Entered title')
    vm.form.status = 'Done'
    vm.form.reporter = 'entered reporter'
    vm.form.assignee = 'entered owner'
    vm.form.tags = ['entered tag']
    vm.form.sprints = [42]
    vm.updateCustomFieldValue('product', 'entered custom')
    const before = JSON.parse(JSON.stringify(vm.form))
    const failedB = deferred()
    api.showConfig.mockReturnValueOnce(failedB.promise)
    await wrapper.get('.task-panel__group select').setValue('B')
    expect(vm.canCreate).toBe(false)
    expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
    await wrapper.get('form').trigger('submit')
    await vm.handleSubmit()
    expect(api.addTask).not.toHaveBeenCalled()
    failedB.reject(new Error('B configuration unavailable'))
    await flushPromises()
    expect(vm.form).toMatchObject({ ...before, project: 'B' })
    expect(vm.customFields.product).toBe('entered custom')
    expect(wrapper.text()).toContain('B configuration unavailable')
    await wrapper.get('form').trigger('submit')
    await vm.handleSubmit()
    expect(api.addTask).not.toHaveBeenCalled()
    const retryB = deferred()
    api.showConfig.mockReturnValueOnce(retryB.promise)
    await wrapper.findAll('button').find(button => button.text() === 'Retry configuration')!.trigger('click')
    expect(vm.canCreate).toBe(false)
    retryB.resolve(config('B'))
    await flushPromises()
    expect(vm.canCreate).toBe(true)
    expect(vm.form.title).toBe('Entered title')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(api.addTask).toHaveBeenCalledTimes(1)
    expect(api.addTask.mock.calls[0]![0]).toMatchObject({
      project: 'B', title: 'Entered title', reporter: 'B-reporter', assignee: 'B-owner', tags: ['B-tag'],
      custom_fields: { product: '' },
    })
    expect(api.addTask.mock.calls[0]![0].sprints).toBeUndefined()
  })

  it('does not reopen or make a closed panel ready after an initial project-list await', async () => {
    const projects = deferred()
    api.listProjects.mockReturnValueOnce(projects.promise)
    const wrapper = panel('A-1')
    await wrapper.setProps({ open: false })
    projects.resolve({ projects: [{ prefix: 'A' }], total: 1 })
    await flushPromises()
    expect(api.getTask).not.toHaveBeenCalled()
    expect(wrapper.find('aside').exists()).toBe(false)
    expect((wrapper.vm as any).canCreate).toBe(false)
  })

  it.each(['resolve', 'reject'] as const)('ignores stale status-save %s without reverting B status', async outcome => {
    const wrapper = panel('A-1')
    await flushPromises()
    const save = deferred()
    api.updateTask.mockReturnValueOnce(save.promise)
    const pending = (wrapper.vm as any).updateStatus('Done')
    await flushPromises()
    expect(api.updateTask).toHaveBeenCalledExactlyOnceWith('A-1', { status: 'Done' })
    expect(api.setStatus).not.toHaveBeenCalled()
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()
    if (outcome === 'resolve') save.resolve({ ...task('A-1'), status: 'Done' })
    else save.reject(new Error('obsolete status failure'))
    await pending
    expect((wrapper.vm as any).form.status).toBe('Open')
    expect((wrapper.vm as any).task.id).toBe('B-1')
    expect(wrapper.emitted('updated')).toBeUndefined()
    expect(toast).not.toHaveBeenCalled()
  })

  it('keeps B history when the A history await completes last', async () => {
    const historyA = deferred()
    api.taskHistory.mockReturnValueOnce(historyA.promise)
    const wrapper = panel('A-1')
    await flushPromises()
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()
    historyA.resolve([{ commit: 'obsolete A' }])
    await flushPromises()
    expect((wrapper.vm as any).commitHistory).toEqual([])
    expect((wrapper.vm as any).form.id).toBe('B-1')
  })

  it('does not restore old task identity or allow autosave after the new edit config fails', async () => {
    const wrapper = panel('A-1')
    await flushPromises()
    api.showConfig.mockRejectedValueOnce(new Error('B unavailable'))
    await wrapper.setProps({ taskId: 'B-1' })
    await flushPromises()
    const vm = wrapper.vm as any
    expect(vm.task.id).toBeUndefined()
    vm.form.title = 'Must not save'
    await vm.onFieldBlurBase('title')
    await vm.updateStatus('Done')
    expect(api.updateTask).not.toHaveBeenCalled()
    expect(api.setStatus).not.toHaveBeenCalled()
  })

  it('finishes a submitted create using captured status without closing the new project form', async () => {
    const wrapper = panel()
    await flushPromises()
    const vm = wrapper.vm as any
    vm.form.title = 'A new task'
    vm.form.status = 'Done'
    const create = deferred()
    api.addTask.mockReturnValueOnce(create.promise)
    const pending = vm.handleSubmit()
    await wrapper.get('.task-panel__group select').setValue('B')
    await flushPromises()
    vm.form.title = 'Unsaved B'
    create.resolve(task('A-2'))
    await pending
    expect(api.addTask).toHaveBeenCalledTimes(1)
    expect(api.addTask.mock.calls[0]![0]).toMatchObject({ status: 'Done' })
    expect(api.setStatus).not.toHaveBeenCalled()
    expect(vm.form.title).toBe('Unsaved B')
    expect(vm.form.project).toBe('B')
    expect(wrapper.emitted('close')).toBeUndefined()
    expect(wrapper.emitted('created')).toBeUndefined()
  })
})
