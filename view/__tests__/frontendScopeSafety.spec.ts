import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, effectScope, nextTick, reactive, ref } from 'vue'
import { createResource } from '../composables/useResource'
import { useConfig } from '../composables/useConfig'
import AutomationView from '../pages/AutomationView.vue'
import { useTaskPanelFormLifecycle } from '../composables/task-panel/useTaskPanelFormLifecycle'

const api = vi.hoisted(() => ({
  inspectAutomation: vi.fn(), inspectConfig: vi.fn(), setAutomation: vi.fn(),
  listProjects: vi.fn(), showConfig: vi.fn(),
}))
vi.mock('../api/client', () => ({ api }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))

function deferred<T = any>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

describe('frontend scope safety', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    api.listProjects.mockResolvedValue({ projects: [] })
    api.inspectConfig.mockResolvedValue(null)
  })

  it('only publishes the latest A/B/A resource request, including stale failures', async () => {
    const requests = [deferred(), deferred(), deferred()]
    let index = 0
    const onError = vi.fn()
    const resource = createResource(async (_scope: string) => requests[index++]!.promise, { onError })
    const pending = ['A', 'B', 'A'].map(scope => resource.refresh(scope))
    requests[2]!.resolve('new A')
    await pending[2]
    requests[0]!.resolve('old A')
    requests[1]!.reject(new Error('old B'))
    await Promise.all(pending)
    expect(resource.data.value).toBe('new A')
    expect(resource.status.value).toBe('ready')
    expect(resource.error.value).toBeNull()
    expect(onError).not.toHaveBeenCalled()
  })

  it('invalidates pending resource requests on reset and explicit set', async () => {
    const request = deferred()
    const resource = createResource(() => request.promise)
    const pending = resource.refresh()
    resource.reset()
    resource.set('local')
    request.resolve('remote')
    await pending
    expect(resource.data.value).toBe('local')
  })

  it('clears old config defaults while the new scope is pending and after failure', async () => {
    const scope = effectScope()
    const config = scope.run(() => useConfig())!
    api.showConfig.mockResolvedValueOnce({ default_status: 'A status' })
    await config.refresh('A')
    await nextTick()
    expect(config.defaults.value.status).toBe('A status')
    const request = deferred()
    api.showConfig.mockReturnValueOnce(request.promise)
    const pending = config.refresh('B')
    expect(config.defaults.value.status).toBe('')
    expect(config.cfg.value).toBeNull()
    request.reject(new Error('B failed'))
    await pending
    expect(config.defaults.value.project).toBe('B')
    expect(config.defaults.value.status).toBe('')
    scope.stop()
  })

  it('publishes only the latest A config defaults after out-of-order A/B/A loads', async () => {
    const scope = effectScope()
    const config = scope.run(() => useConfig())!
    const requests = [deferred(), deferred(), deferred()]
    requests.forEach(request => api.showConfig.mockReturnValueOnce(request.promise))
    const pending = ['A', 'B', 'A'].map(project => config.refresh(project))
    requests[2]!.resolve({ default_status: 'new A' })
    await pending[2]
    await nextTick()
    requests[0]!.resolve({ default_status: 'old A' })
    requests[1]!.resolve({ default_status: 'old B' })
    await Promise.all(pending)
    await nextTick()
    expect(config.defaults.value.project).toBe('A')
    expect(config.defaults.value.status).toBe('new A')
    scope.stop()
  })

  it('applies panel project defaults once for the latest A/B/A request', async () => {
    const scope = effectScope()
    const requests = [deferred<void>(), deferred<void>(), deferred<void>()]
    const refreshConfig = vi.fn()
    requests.forEach(request => refreshConfig.mockReturnValueOnce(request.promise))
    const resetSprintsState = vi.fn()
    const form = reactive({ project: 'A' })
    const lifecycle = scope.run(() => useTaskPanelFormLifecycle({
      mode: computed(() => 'create'), form, suppressWatch: ref(false), ready: ref(false), errors: {},
      defaults: ref({ status: 'latest', tags: [] }), statuses: ref([]), priorities: ref([]), types: ref([]),
      projects: ref([]), refreshConfig, resetSprintsState,
      mergeKnownTags: vi.fn(), resetCustomFields: vi.fn(), ensureConfiguredCustomFields: vi.fn(),
      applyDefaultsFromConfig: vi.fn(), syncOwnershipControls: vi.fn(), preloadPeople: vi.fn(),
    } as any))!
    lifecycle.onProjectChange()
    form.project = 'B'
    lifecycle.onProjectChange()
    form.project = 'A'
    lifecycle.onProjectChange()
    requests[2]!.resolve()
    await flushPromises()
    expect(resetSprintsState).toHaveBeenCalledTimes(1)
    requests[0]!.resolve()
    requests[1]!.resolve()
    await flushPromises()
    expect(form.project).toBe('A')
    expect(resetSprintsState).toHaveBeenCalledTimes(1)
    scope.stop()
  })

  function mountAutomation() {
    return mount(AutomationView, { global: { stubs: { AutomationRulesEditor: true } } })
  }

  it('keeps the latest automation A/B/A completion and blocks saving a failed scope', async () => {
    const requests = [deferred(), deferred(), deferred(), deferred()]
    requests.forEach(request => api.inspectAutomation.mockReturnValueOnce(request.promise))
    const wrapper = mountAutomation()
    const vm = wrapper.vm as any
    vm.project = 'A'
    vm.project = 'B'
    vm.project = 'A'
    requests[3]!.resolve({ scope_yaml: 'new A' })
    await flushPromises()
    requests[1]!.resolve({ scope_yaml: 'old A' })
    requests[2]!.resolve({ scope_yaml: 'old B' })
    requests[0]!.resolve({ scope_yaml: 'global' })
    await flushPromises()
    expect(vm.scopeYaml).toBe('new A')
    vm.scopeYaml = 'edited A'
    const failed = deferred()
    api.inspectAutomation.mockReturnValueOnce(failed.promise)
    vm.project = 'B'
    await vm.saveRules()
    expect(api.setAutomation).not.toHaveBeenCalled()
    failed.reject(new Error('B unavailable'))
    await flushPromises()
    await vm.saveRules()
    expect(api.setAutomation).not.toHaveBeenCalled()
    expect(vm.rulesDirty).toBe(false)
    wrapper.unmount()
  })

  it.each(['resolve', 'reject'] as const)('ignores an old automation save %s after moving to B', async outcome => {
    api.inspectAutomation.mockResolvedValueOnce({ scope_yaml: 'global' })
    const wrapper = mountAutomation()
    await flushPromises()
    const vm = wrapper.vm as any
    vm.scopeYaml = 'edited global'
    const save = deferred()
    api.setAutomation.mockReturnValueOnce(save.promise)
    const pending = vm.saveRules()
    api.inspectAutomation.mockResolvedValueOnce({ scope_yaml: 'B rules' })
    vm.project = 'B'
    await flushPromises()
    vm.scopeYaml = 'unsaved B'
    if (outcome === 'resolve') save.resolve({})
    else save.reject(new Error('old save failed'))
    await pending
    expect(vm.scopeYaml).toBe('unsaved B')
    expect(vm.baselineYaml).toBe('B rules')
    expect(vm.rulesSaveError).toBe('')
    expect(api.inspectAutomation).toHaveBeenCalledTimes(2)
    expect(api.setAutomation).toHaveBeenCalledWith({ yaml: 'edited global', project: undefined })
    wrapper.unmount()
  })
})
