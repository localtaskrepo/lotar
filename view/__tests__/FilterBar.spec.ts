import { mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

const projectState = vi.hoisted(() => ({
  projectsRef: null as null | { value: Array<{ name: string; prefix: string }> },
}))

vi.mock('../composables/useProjects', async () => {
  const vue = await import('vue')
  projectState.projectsRef ??= vue.ref<Array<{ name: string; prefix: string }>>([])
  return {
    useProjects: () => ({
      projects: projectState.projectsRef!,
      refresh: async () => { },
    }),
  }
})

import FilterBar from '../components/FilterBar.vue';

function findByPlaceholder(wrapper: any, ph: string) {
  return wrapper.findAll('input').find((i: any) => i.attributes('placeholder')?.includes(ph))
}

describe('FilterBar', () => {
  beforeEach(() => {
    // Default to multi-project so the project control renders as a select in most tests.
    projectState.projectsRef!.value = [
      { name: 'api-service', prefix: 'AS' },
      { name: 'frontend-app', prefix: 'FA' },
    ]
  })

  it('preserves incoming assignee when emitting after edits', async () => {
    const wrapper = mount(FilterBar, { props: { value: { assignee: '@me' } } })
    const search = findByPlaceholder(wrapper, 'Search')
    await search!.setValue('roadmap')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.assignee).toBe('@me')
    expect(last?.q).toBe('roadmap')
  })

  it('shows helper text in tooltip when custom filters are invalid', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    expect(custom).toBeTruthy()
    await custom!.setValue('field:iteration')
    await nextTick()
    const hint = wrapper.find('[data-testid="custom-filter-hint"]')
    expect(hint.attributes('title')).toContain('missing "="')
    await hint.trigger('mouseenter')
    await nextTick()
    const popover = wrapper.find('[data-testid="custom-filter-hint-popover"]')
    expect(popover.exists()).toBe(true)
    expect(popover.text()).toContain('missing "="')
  })

  it('appendCustomFilter exposes shortcut for presets', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const vm: any = wrapper.vm
    vm.appendCustomFilter('field:iteration=')
    await nextTick()
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    expect((custom!.element as HTMLInputElement).value).toContain('field:iteration=')
  })

  it('maps field:priority to the native priority filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    await custom!.setValue('field:priority=Medium')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.priority).toBe('Medium')
  })

  it('maps field:task_type to the native type filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    await custom!.setValue('field:task_type=Bug')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.type).toBe('Bug')
  })

  it('maps field:state to the native status filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    await custom!.setValue('field:STATE=Backlog')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.status).toBe('Backlog')
  })

  it('emits custom filters when provided via input', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    expect(custom).toBeTruthy()
    await custom!.setValue('field:iteration=beta, owner=ops')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.['field:iteration']).toBe('beta')
    expect(last?.owner).toBe('ops')
  })

  it('hydrates custom filters from incoming value', async () => {
    const wrapper = mount(FilterBar, {
      props: { value: { q: 'abc', 'field:iteration': 'beta', scope: 'edge' } },
    })
    await nextTick()
    const custom = findByPlaceholder(wrapper, 'Custom filters')
    expect(custom?.element.value).toContain('field:iteration=beta')
    expect(custom?.element.value).toContain('scope=edge')
  })

  it('hides the status select when showStatus is false', () => {
    const wrapper = mount(FilterBar, { props: { value: {}, statuses: ['Todo'], showStatus: false } })
    expect(wrapper.find('[data-testid="filter-status"]').exists()).toBe(false)
  })

  it('renders project as static text when only one project exists', async () => {
    projectState.projectsRef!.value = [{ name: 'api-service', prefix: 'AS' }]
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await nextTick()

    expect(wrapper.find('select[data-testid="filter-project"]').exists()).toBe(false)
    const projectEl = wrapper.find('[data-testid="filter-project"]')
    expect(projectEl.exists()).toBe(true)
    expect(projectEl.text()).toContain('api-service')
    expect(projectEl.text()).toContain('AS')

    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.project).toBe('AS')
  })

  it('inverts status selection when requested', async () => {
    const wrapper = mount(FilterBar, {
      props: {
        statuses: ['Todo', 'Doing', 'Done'],
        value: { status: 'Todo,Done' },
      },
    })

    await nextTick()
    await wrapper.find('[data-testid="filter-status"]').trigger('click')
    await nextTick()

    const invert = wrapper.find('button.filter-bar__menu-action')
    // First action button might be Clear depending on initial selection; pick the one labeled Invert.
    const invertBtn = wrapper.findAll('button.filter-bar__menu-action').find((b) => b.text().includes('Invert'))
    expect(invertBtn).toBeTruthy()

    await invertBtn!.trigger('click')
    await nextTick()

    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.status).toBe('Doing')
  })

  it('emits project key when emitProjectKey is enabled even if empty', async () => {
    const wrapper = mount(FilterBar, { props: { value: {}, emitProjectKey: true } })
    const search = findByPlaceholder(wrapper, 'Search')
    await search!.setValue('foo')
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last).toBeTruthy()
    expect(Object.prototype.hasOwnProperty.call(last, 'project')).toBe(true)
    expect(last?.project).toBe('')
  })
})
