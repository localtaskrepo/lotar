import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

function openPanel(wrapper: any) {
  const toggle = wrapper.find('[data-testid="filter-toggle"]')
  return toggle.trigger('click')
}

async function submitSearch(wrapper: any, text: string) {
  const search = wrapper.find('[data-testid="filter-search"]')
  await search.setValue(text)
  await search.trigger('keydown', { key: 'Enter' })
  await nextTick()
}

function lastValue(wrapper: any): Record<string, string> {
  const events = wrapper.emitted('update:value') || []
  return events[events.length - 1]?.[0] || {}
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

  it('shows an inline search error without a duplicate custom input', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    expect(findByPlaceholder(wrapper, 'Custom filters')).toBeUndefined()
    await submitSearch(wrapper, 'field:iteration=')
    expect(wrapper.find('[role="alert"]').text()).toContain('value')
    expect(wrapper.find('[data-testid="filter-search"]').attributes('aria-invalid')).toBe('true')
    wrapper.unmount()
  })

  it('appendCustomFilter exposes shortcut for presets', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    const vm: any = wrapper.vm
    vm.appendCustomFilter('field:iteration=')
    await nextTick()
    expect((wrapper.find('[data-testid="filter-search"]').element as HTMLInputElement).value).toContain('field:iteration=')
    wrapper.unmount()
  })

  it('maps field:priority to the native priority filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    await submitSearch(wrapper, 'field:priority=Medium')
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.priority).toBe('Medium')
  })

  it('maps field:task_type to the native type filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    await submitSearch(wrapper, 'field:task_type=Bug')
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.type).toBe('Bug')
  })

  it('maps field:state to the native status filter', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    await submitSearch(wrapper, 'field:STATE=Backlog')
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.status).toBe('Backlog')
  })

  it('emits custom fields and bare assignments through the main search', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
    await submitSearch(wrapper, 'field:iteration=beta owner=ops')
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.['field:iteration']).toBe('beta')
    expect(last?.owner).toBe('ops')
  })

  describe('deletion visibility (DEV-92)', () => {
    it('defaults to active and omits the deletion key from emissions', async () => {
      const wrapper = mount(FilterBar, { props: { value: {} } })
      await openPanel(wrapper)
      const select = wrapper.find('[data-testid="task-deletion-filter"]')
      expect(select.exists()).toBe(true)
      expect((select.element as HTMLSelectElement).value).toBe('active')

      await select.setValue('active')
      await nextTick()
      const events = wrapper.emitted('update:value') || []
      const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
      expect(last?.deletion).toBeUndefined()
    })

    it('emits deletion=deleted when the trash view is selected', async () => {
      const wrapper = mount(FilterBar, { props: { value: {} } })
      await openPanel(wrapper)
      await wrapper.find('[data-testid="task-deletion-filter"]').setValue('deleted')
      await nextTick()
      const events = wrapper.emitted('update:value') || []
      const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
      expect(last?.deletion).toBe('deleted')
    })

    it('emits deletion=all and round-trips an incoming value without duplicating it into custom filters', async () => {
      const wrapper = mount(FilterBar, { props: { value: { deletion: 'all' } } })
      await openPanel(wrapper)
      const select = wrapper.find('[data-testid="task-deletion-filter"]')
      expect((select.element as HTMLSelectElement).value).toBe('all')

      // Change away and back so the watcher fires with the incoming value.
      await select.setValue('active')
      await select.setValue('all')
      await nextTick()
      const events = wrapper.emitted('update:value') || []
      const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
      expect(last?.deletion).toBe('all')

      expect(wrapper.findAll('.filter-bar__chip').filter(chip => chip.text().includes('All tasks'))).toHaveLength(1)
      expect(findByPlaceholder(wrapper, 'Custom filters')).toBeUndefined()
    })
  })

  it('renders incoming custom filters as removable chips', async () => {
    const wrapper = mount(FilterBar, {
      props: { value: { q: 'abc', 'field:iteration': 'beta', scope: 'edge' } },
    })
    await openPanel(wrapper)
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('iteration')
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('beta')
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('scope')
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('edge')
  })

  it('hides the status select when showStatus is false', () => {
    const wrapper = mount(FilterBar, { props: { value: {}, statuses: ['Todo'], showStatus: false } })
    expect(wrapper.find('[data-testid="filter-status"]').exists()).toBe(false)
  })

  it('renders project as static text when only one project exists', async () => {
    projectState.projectsRef!.value = [{ name: 'api-service', prefix: 'AS' }]
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await openPanel(wrapper)
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

    await openPanel(wrapper)
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

  it('Enter picks the highlighted suggestion for a bare partial match', async () => {
    const wrapper = mount(FilterBar, { props: { value: {}, statuses: ['Todo', 'Done'] } })
    const search = findByPlaceholder(wrapper, 'Search')
    await search!.setValue('todo')
    await search!.trigger('keydown', { key: 'ArrowDown' })
    await search!.trigger('keydown', { key: 'Enter' })
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.status).toBe('Todo')
    expect(last?.q).toBeUndefined()
  })

  it('inserts a picked key prefix without a trailing space and offers its values immediately', async () => {
    const wrapper = mount(FilterBar, { props: { value: {}, statuses: ['Todo', 'Done'] } })
    const search = findByPlaceholder(wrapper, 'Search')
    await search!.setValue('sta')
    await search!.trigger('keydown', { key: 'Enter' })

    const input = search!.element as HTMLInputElement
    expect(input.value).toBe('status:')

    await nextTick()
    const labels = wrapper.findAll('.filter-bar__suggestion').map((b) => b.text())
    expect(labels.some((t) => t.includes('Todo'))).toBe(true)
    expect(labels.some((t) => t.includes('Done'))).toBe(true)
  })

  it('Enter falls back to free text when no suggestion matches', async () => {
    const wrapper = mount(FilterBar, { props: { value: {}, statuses: ['Todo'] } })
    const search = findByPlaceholder(wrapper, 'Search')
    await search!.setValue('roadmap zzz')
    await search!.trigger('keydown', { key: 'Enter' })
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.q).toBe('roadmap zzz')
  })

  it('drops a persisted project unknown to this server', async () => {
    projectState.projectsRef!.value = []
    const wrapper = mount(FilterBar, { props: { value: { project: 'ZZ' } } })
    await nextTick()
    projectState.projectsRef!.value = [
      { name: 'api-service', prefix: 'AS' },
      { name: 'frontend-app', prefix: 'FA' },
    ]
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const last = events[events.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.project ?? '').toBe('')
  })
})

describe('FilterBar sort snapshot ownership (DEV-57)', () => {
  beforeEach(() => {
    localStorage.clear()
    projectState.projectsRef!.value = [
      { name: 'api-service', prefix: 'AS' },
      { name: 'frontend-app', prefix: 'FA' },
    ]
  })

  it('does not restore sort_by/order from its saved filter snapshot', async () => {
    // A stale snapshot written before single-ownership must not resurrect a
    // conflicting sort on a plain reload: the page's sort storage owns it.
    localStorage.setItem(
      'lotar.tasks.filter',
      JSON.stringify({ status: 'todo', sort_by: 'status', order: 'asc' }),
    )
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await nextTick()
    const events = wrapper.emitted('update:value') || []
    const payloads = events.map((e) => e[0] as Record<string, string>)
    for (const payload of payloads) {
      expect(payload.sort_by).toBeUndefined()
      expect(payload.order).toBe('desc')
    }
    const restoring = payloads.find((p) => p.status === 'todo')
    expect(restoring).toBeTruthy()
    wrapper.unmount()
  })

  it('persists snapshots without the sort keys while emitting them live', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await nextTick()
    await wrapper.setProps({ value: { status: 'todo', sort_by: 'custom:Rank', order: 'asc' } })
    await nextTick()
    wrapper.unmount()

    const saved = JSON.parse(localStorage.getItem('lotar.tasks.filter') || '{}')
    expect(saved.status).toBe('todo')
    expect(saved.sort_by).toBeUndefined()
    expect(saved.order).toBeUndefined()
  })

  it('round-trips sort_by and order live without emitting extra changes', async () => {
    const wrapper = mount(FilterBar, { props: { value: {} } })
    await nextTick()
    ;(wrapper.emitted('update:value') || []).length = 0

    await wrapper.setProps({ value: { status: 'todo', sort_by: 'priority', order: 'asc' } })
    await nextTick()
    const events = (wrapper.emitted('update:value') || []).map((e) => e[0] as Record<string, string>)
    const last = events[events.length - 1]
    expect(last?.sort_by).toBe('priority')
    expect(last?.order).toBe('asc')
    expect(last?.status).toBe('todo')
    wrapper.unmount()
  })
})

describe('FilterBar unified search and panel cleanup (DEV-98)', () => {
  const wrappers: ReturnType<typeof mount>[] = []
  beforeEach(() => {
    localStorage.clear()
    projectState.projectsRef!.value = [
      { name: 'api-service', prefix: 'AS' }, { name: 'frontend-app', prefix: 'FA' },
    ]
  })
  afterEach(() => {
    for (const wrapper of wrappers.splice(0)) wrapper.unmount()
    document.body.replaceChildren()
  })
  function render(props: Record<string, unknown> = {}, attached = false) {
    const wrapper = mount(FilterBar, { props: { value: {}, ...props }, ...(attached ? { attachTo: document.body } : {}) })
    wrappers.push(wrapper)
    return wrapper
  }

  it('replaces equality assignments while colon selection stays additive', async () => {
    const wrapper = render({ value: { priority: 'High', status: 'Todo', type: 'Feature' } })
    await submitSearch(wrapper, 'field:priority=Medium field:STATE=Done field:task_type=Bug')
    expect(lastValue(wrapper)).toMatchObject({ priority: 'Medium', status: 'Done', type: 'Bug' })
    await submitSearch(wrapper, 'status:Todo')
    expect(lastValue(wrapper).status).toBe('Done,Todo')
  })

  it('retains committed filters and results while a custom draft is incomplete or invalid', async () => {
    const wrapper = render({ value: { q: 'roadmap', status: 'Todo', 'field:iteration': 'beta' } })
    await flushPromises()
    const count = (wrapper.emitted('update:value') || []).length
    await submitSearch(wrapper, 'status:Done field:iteration=')
    expect(wrapper.find('[role="alert"]').exists()).toBe(true)
    expect((wrapper.emitted('update:value') || []).length).toBe(count)
    expect(wrapper.props('value')).toMatchObject({ q: 'roadmap', status: 'Todo', 'field:iteration': 'beta' })
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('Todo')
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('beta')
    await submitSearch(wrapper, 'roadmap field:iteration=gamma')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(lastValue(wrapper)).toMatchObject({ q: 'roadmap', status: 'Todo', 'field:iteration': 'gamma' })
  })

  it('does not search operator-prefix characters as free text while a field expression is typed', async () => {
    const wrapper = render({ value: { q: 'roadmap', 'field:iteration': 'beta' } })
    await flushPromises()
    const count = (wrapper.emitted('update:value') || []).length
    const search = wrapper.find('[data-testid="filter-search"]')
    for (const text of ['f', 'fi', 'fie', 'fiel', 'field', 'field:', 'field:iteration=']) await search.setValue(text)
    expect((wrapper.emitted('update:value') || []).length).toBe(count)
    await submitSearch(wrapper, 'field:iteration=gamma')
    expect(lastValue(wrapper)['field:iteration']).toBe('gamma')
    expect(lastValue(wrapper).q).toBeUndefined()
  })

  it('restores legacy object snapshots without losing commas, quotes, spaces or equals', async () => {
    const value = 'beta, "release" = ready'
    localStorage.setItem('lotar.tasks.filter', JSON.stringify({ q: 'release', 'field:iteration': value, scope: 'edge', sort_by: 'status', order: 'asc' }))
    const wrapper = render()
    await flushPromises()
    expect(lastValue(wrapper)).toMatchObject({ q: 'release', 'field:iteration': value, scope: 'edge', order: 'desc' })
    expect(lastValue(wrapper).sort_by).toBeUndefined()
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain(value)
    const remove = wrapper.findAll('button').find(button => button.attributes('aria-label')?.startsWith('Remove filter iteration '))!
    await remove.trigger('click')
    expect(lastValue(wrapper)['field:iteration']).toBeUndefined()
    expect(lastValue(wrapper).scope).toBe('edge')
    expect(JSON.parse(localStorage.getItem('lotar.tasks.filter') || '{}').scope).toBe('edge')
  })

  it('queues and focuses an editable quoted-name preset without applying a partial filter', async () => {
    const wrapper = render({ value: { q: 'roadmap' }, customPresets: [{ label: 'Release Stage', expression: 'field:Release Stage=' }] }, true)
    await flushPromises()
    const count = (wrapper.emitted('update:value') || []).length
    ; (wrapper.vm as any).appendCustomFilter('field:Release Stage=')
    await nextTick()
    const search = wrapper.find('[data-testid="filter-search"]')
    expect((search.element as HTMLInputElement).value).toBe('roadmap field:"Release Stage"=')
    expect(document.activeElement).toBe(search.element)
    expect((wrapper.emitted('update:value') || []).length).toBe(count)
    await submitSearch(wrapper, 'roadmap field:"Release Stage"="beta team"')
    expect(lastValue(wrapper)).toMatchObject({ q: 'roadmap', 'field:release stage': 'beta team' })
  })

  it('exposes labelled fields and closes the region with focus restored to Filters', async () => {
    const wrapper = render({ statuses: ['Todo'], priorities: ['High'], types: ['Bug'], sprintOptions: [{ id: 1, label: 'Sprint one' }] }, true)
    await openPanel(wrapper)
    expect(wrapper.find('[data-testid="filter-panel"]').attributes('role')).toBe('region')
    expect(wrapper.find('[data-testid="filter-panel"]').text()).toContain('Quick picks')
    expect(wrapper.find('[role="group"][aria-label="Filter fields"]').exists()).toBe(true)
    expect(wrapper.findAll('h4').some(heading => heading.text() === 'Filter fields')).toBe(false)
    expect(wrapper.find('[aria-label="Custom filters"]').exists()).toBe(false)
    expect(wrapper.find('[aria-label="Close filters"]').exists()).toBe(false)
    await wrapper.find('[data-testid="filter-status"]').trigger('click')
    const checkbox = wrapper.find('input[type="checkbox"]')
    ; (checkbox.element as HTMLInputElement).focus()
    await checkbox.trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('.filter-bar__menu-popover').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.find('[data-testid="filter-status"]').element)
    await wrapper.find('[data-testid="filter-status"]').trigger('keydown', { key: 'Escape' })
    await nextTick()
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.find('[data-testid="filter-toggle"]').element)
    await openPanel(wrapper)
    await wrapper.find('[data-testid="filter-toggle"]').trigger('click')
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(false)
  })

  it('lists every supported filter syntax in help without changing applied filters', async () => {
    const wrapper = render({ value: { project: 'AS', status: 'Todo', tags: 'ui' } }, true)
    await nextTick()
    const before = (wrapper.emitted('update:value') || []).length
    const toggle = wrapper.find('[data-testid="filter-help-toggle"]')
    expect(toggle.classes()).toContain('ghost')
    expect(toggle.classes()).toContain('icon-only')
    expect(toggle.attributes('aria-expanded')).toBe('false')
    await toggle.trigger('click')
    const help = wrapper.find('[data-testid="filter-help"]')
    expect(toggle.attributes('aria-expanded')).toBe('true')
    expect(help.attributes('role')).toBe('region')
    expect(toggle.attributes('aria-controls')).toBe(help.attributes('id'))
    for (const key of ['status:', 'priority:', 'type:', 'sprints:', 'project:', 'assignee:', 'tags:', 'due:', 'recent:', 'needs:', 'mine:', 'field:name=', 'q=', 'deletion=', 'order=', 'sort_by=']) {
      expect(help.text()).toContain(key)
    }
    expect(help.text()).toContain('today, soon, later or overdue')
    expect(help.text()).toContain('7d')
    expect((wrapper.emitted('update:value') || []).length).toBe(before)
    await toggle.trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(false)
    expect(document.activeElement).toBe(toggle.element)
    await toggle.trigger('click')
    await wrapper.find('[data-testid="filter-search"]').trigger('click')
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(false)
    expect((wrapper.emitted('update:value') || []).length).toBe(before)
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('Todo')
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('ui')
  })

  it('keeps help and filter disclosure states independent while using filter controls', async () => {
    const wrapper = render({ statuses: ['Todo', 'Done'], value: { project: 'AS', status: 'Todo' } }, true)
    await nextTick()
    const before = (wrapper.emitted('update:value') || []).length
    const help = wrapper.find('[data-testid="filter-help-toggle"]')
    const filters = wrapper.find('[data-testid="filter-toggle"]')
    await help.trigger('click')
    await filters.trigger('click')
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(true)
    expect(help.attributes('aria-expanded')).toBe('true')
    await wrapper.find('[data-testid="filter-status"]').trigger('click')
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(true)
    await wrapper.find('[data-testid="filter-status"]').trigger('click')
    await filters.trigger('click')
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(true)
    await filters.trigger('click')
    await help.trigger('click')
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(false)
    expect(help.attributes('aria-expanded')).toBe('false')
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(true)
    await help.trigger('click')
    await wrapper.find('[data-testid="filter-panel"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(true)
    expect(document.activeElement).toBe(filters.element)
    await filters.trigger('click')
    await wrapper.find('[data-testid="filter-search"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[data-testid="filter-panel"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="filter-help"]').exists()).toBe(true)
    expect((wrapper.emitted('update:value') || []).length).toBe(before)
  })

  it('offers existing tags through a key pick and applies a quoted value without free-text leakage', async () => {
    const wrapper = render({ tagOptions: ['ui', 'release candidate'], value: { project: 'AS' } }, true)
    const search = wrapper.find('[data-testid="filter-search"]')
    await search.setValue('tag')
    await search.trigger('keydown', { key: 'Enter' })
    expect((search.element as HTMLInputElement).value).toBe('tags:')
    expect(wrapper.find('[role="listbox"]').text()).toContain('release candidate')
    await search.setValue('tags:release')
    await search.trigger('keydown', { key: 'Enter' })
    expect(lastValue(wrapper)).toMatchObject({ project: 'AS', tags: 'release candidate' })
    expect(lastValue(wrapper).q).toBeUndefined()
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('release candidate')
  })

  it('updates tag options without carrying old project suggestions into the new scope', async () => {
    const wrapper = render({ tagOptions: ['old-scope'], value: { project: 'AS' } }, true)
    const search = wrapper.find('[data-testid="filter-search"]')
    await search.setValue('tags:')
    expect(wrapper.find('[role="listbox"]').text()).toContain('old-scope')
    await wrapper.setProps({ tagOptions: [], value: { project: 'FA' } })
    await search.setValue('tags:')
    expect(wrapper.find('[role="listbox"]').exists()).toBe(false)
    await wrapper.setProps({ tagOptions: ['new-scope'] })
    expect(wrapper.find('[role="listbox"]').text()).toContain('new-scope')
    expect(wrapper.find('[role="listbox"]').text()).not.toContain('old-scope')
  })

  it('offers configured fields beyond quick picks and observed values for literal reserved names', async () => {
    const wrapper = render({ customFieldOptions: ['*', 'sprint', 'release stage'], customFieldValues: { sprint: ['inc-2'], 'release stage': ['beta team'] } }, true)
    const search = wrapper.find('[data-testid="filter-search"]')
    await search.setValue('field:')
    expect(wrapper.find('[role="listbox"]').text()).toContain('release stage')
    expect(wrapper.find('[role="listbox"]').text()).not.toContain('*')
    await search.setValue('field:"sprint"=inc')
    await search.trigger('keydown', { key: 'Enter' })
    expect(lastValue(wrapper)['field:sprint']).toBe('inc-2')
    expect(lastValue(wrapper).sprints).toBeUndefined()
  })

  it('keeps plain search words as text even when they match an existing tag', async () => {
    const wrapper = render({ tagOptions: ['roadmap'], value: {} }, true)
    await submitSearch(wrapper, 'roadmap')
    expect(lastValue(wrapper).q).toBe('roadmap')
    expect(lastValue(wrapper).tags).toBeUndefined()
  })

  it('clears conditions but preserves the current project and live sort owner', async () => {
    const wrapper = render({ value: { project: 'AS', q: 'roadmap', status: 'Todo', tags: 'ui', deletion: 'all', 'field:iteration': 'beta', sort_by: 'priority', order: 'asc' } })
    await openPanel(wrapper)
    const clear = wrapper.findAll('button').find(button => button.text() === 'Clear conditions')!
    await clear.trigger('click')
    expect(lastValue(wrapper)).toEqual({ project: 'AS', sort_by: 'priority', order: 'asc' })
    const snapshot = JSON.parse(localStorage.getItem('lotar.tasks.filter') || '{}')
    expect(snapshot.sort_by).toBeUndefined()
    expect(snapshot.order).toBeUndefined()
    expect(snapshot['field:iteration']).toBeUndefined()
  })

  it('shows tags and deletion visibility as removable applied chips', async () => {
    const wrapper = render({ value: { tags: 'ui,api', deletion: 'deleted' } })
    await nextTick()
    expect(wrapper.find('[data-testid="filter-chips"]').text()).toContain('Deleted tasks')
    expect(wrapper.findAll('.filter-bar__chip')).toHaveLength(3)
    const remove = wrapper.findAll('button').find(button => button.attributes('aria-label')?.startsWith('Remove filter Visibility'))!
    await remove.trigger('click')
    expect(lastValue(wrapper).deletion).toBeUndefined()
    expect(lastValue(wrapper).tags).toBe('ui,api')
  })

  it('commits a manually typed filter on Enter after trailing whitespace', async () => {
    const wrapper = render({ statuses: ['Todo', 'Done'] })
    await submitSearch(wrapper, 'status:Todo ')
    expect(lastValue(wrapper).status).toBe('Todo')
    expect((wrapper.find('[data-testid="filter-search"]').element as HTMLInputElement).value).toBe('')
  })

  it('queues a literal custom-field preset even when its name is a builtin alias', async () => {
    const wrapper = render({ customPresets: [{ label: 'sprint', expression: 'field:sprint=' }] })
    ; (wrapper.vm as any).appendCustomFilter('field:sprint=')
    await nextTick()
    expect((wrapper.find('[data-testid="filter-search"]').element as HTMLInputElement).value).toBe('field:"sprint"=')
    await submitSearch(wrapper, 'field:"sprint"=inc-2')
    expect(lastValue(wrapper)['field:sprint']).toBe('inc-2')
    expect(lastValue(wrapper).sprints).toBeUndefined()
  })

  it('round-trips prototype-named custom fields as ordinary own string properties', async () => {
    const wrapper = render()
    await submitSearch(wrapper, '__proto__=fixture field:constructor=value')
    const payload = lastValue(wrapper)
    expect(Object.prototype.hasOwnProperty.call(payload, '__proto__')).toBe(true)
    expect(payload['__proto__']).toBe('fixture')
    expect(payload['field:constructor']).toBe('value')
    const snapshot = JSON.parse(localStorage.getItem('lotar.tasks.filter') || '{}')
    expect(snapshot['__proto__']).toBe('fixture')
  })

  it('uses search and chips instead of separate Tags or Custom filters text inputs', async () => {
    const wrapper = render({ value: { tags: 'ui,api' } })
    await openPanel(wrapper)
    expect(wrapper.find('input[placeholder="Tags"]').exists()).toBe(false)
    expect(wrapper.find('[aria-label="Custom filters"]').exists()).toBe(false)
    expect(wrapper.findAll('.filter-bar__chip-value').map(chip => chip.text())).toEqual(expect.arrayContaining(['ui', 'api']))
    await submitSearch(wrapper, 'tags=backend')
    expect(lastValue(wrapper).tags).toBe('backend')
  })

  it('shows the end-of-input clear button only for written search content', async () => {
    const wrapper = render({ value: { project: 'AS', status: 'Todo' } }, true)
    expect(wrapper.find('[data-testid="filter-search-clear"]').exists()).toBe(false)
    await wrapper.find('[data-testid="filter-search"]').setValue('roadmap')
    const clear = wrapper.find('[data-testid="filter-search-clear"]')
    expect(clear.attributes('aria-label')).toBe('Clear search')
    await clear.trigger('click')
    await nextTick()
    expect((wrapper.find('[data-testid="filter-search"]').element as HTMLInputElement).value).toBe('')
    expect(wrapper.find('[data-testid="filter-search-clear"]').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.find('[data-testid="filter-search"]').element)
    expect(lastValue(wrapper).q).toBeUndefined()
    expect(lastValue(wrapper)).toMatchObject({ project: 'AS', status: 'Todo' })
  })

  it('clears an invalid draft and its errors without removing applied filters or sort', async () => {
    const wrapper = render({ value: { q: 'roadmap', tags: 'ui', 'field:iteration': 'beta', order: 'asc', sort_by: 'priority' } }, true)
    await submitSearch(wrapper, 'field:iteration=')
    expect(wrapper.find('[role="alert"]').exists()).toBe(true)
    await wrapper.find('[data-testid="filter-search-clear"]').trigger('click')
    await nextTick()
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="filter-search"]').attributes('aria-invalid')).toBe('false')
    expect(lastValue(wrapper)).toMatchObject({ tags: 'ui', 'field:iteration': 'beta', order: 'asc', sort_by: 'priority' })
    expect(lastValue(wrapper).q).toBeUndefined()
    expect(document.activeElement).toBe(wrapper.find('[data-testid="filter-search"]').element)
  })

  it('expresses every panel/smart condition through the top search', async () => {
    const wrapper = render()
    await submitSearch(wrapper, 'project:AS status:Todo priority:High type:Bug sprints:1 tags:ui assignee:alice due:soon recent:7d needs:effort deletion=all sort_by=priority order=asc')
    expect(lastValue(wrapper)).toMatchObject({
      project: 'AS', status: 'Todo', priority: 'High', type: 'Bug', sprints: '1', tags: 'ui',
      assignee: 'alice', due: 'soon', recent: '7d', needs: 'effort', deletion: 'all', sort_by: 'priority', order: 'asc',
    })
  })
})
