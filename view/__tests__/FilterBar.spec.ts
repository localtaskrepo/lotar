import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { nextTick } from 'vue'
import FilterBar from '../components/FilterBar.vue'

function findByPlaceholder(wrapper: any, ph: string) {
  return wrapper.findAll('input').find((i: any) => i.attributes('placeholder')?.includes(ph))
}

describe('FilterBar', () => {
  it('emits assignee=@me when My tasks is checked', async () => {
    const wrapper = mount(FilterBar, {
      props: { statuses: ['TODO'], priorities: ['P1'], types: ['feature'], value: {} },
    })
    const my = wrapper.find('input[type="checkbox"]')
    await my.setValue(true)
    await nextTick()
    const evts = wrapper.emitted('update:value') || []
    const last = evts[evts.length - 1]?.[0] as Record<string, string> | undefined
    expect(last?.assignee).toBe('@me')
  })

  it('restores My tasks when value has assignee=@me', async () => {
    const wrapper = mount(FilterBar, { props: { value: { assignee: '@me' } } })
    const my = wrapper.find('input[type="checkbox"]')
    expect((my.element as HTMLInputElement).checked).toBe(true)
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
})
