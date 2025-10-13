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
})
