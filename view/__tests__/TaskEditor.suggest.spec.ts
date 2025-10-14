import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import TaskEditor from '../components/TaskEditor.vue'

vi.mock('../api/client', () => ({
  api: {
    whoami: () => Promise.resolve('me'),
    showConfig: () => Promise.resolve({ custom_fields: ['product'] }),
    listTasks: () => Promise.resolve([
      { id: 'PRJ-1', title: 'A', status: 'open', priority: 'low', task_type: 'task', assignee: 'alice', created: '', modified: '', tags: ['x', 'y'], relationships: {}, comments: [], custom_fields: { product: 'frontend' } },
      { id: 'PRJ-2', title: 'B', status: 'open', priority: 'low', task_type: 'task', assignee: 'bob', created: '', modified: '', tags: ['y', 'z'], relationships: {}, comments: [], custom_fields: { product: 'backend' } },
    ]),
    suggestTasks: () => Promise.resolve([]),
  }
}))

const propsBase = { mode: 'create' as any, modelValue: { title: '', project: 'PRJ', tags: [], custom_fields: {} }, projects: [] as any[], statuses: [] as string[], priorities: [] as string[], types: [] as string[] }

describe('TaskEditor suggestions', () => {
  it('suggests tags and picks on click', async () => {
    const wrapper = mount(TaskEditor as any, { props: propsBase as any })
    // focus to open suggestions
    const input = wrapper.find('input.tag-input')
    await input.trigger('focus')
    await input.setValue('y')
    // wait next tick
    await new Promise(r => setTimeout(r))
    const items = wrapper.findAll('ul.suggest li')
    // pick tag 'y'
    await items[0].trigger('mousedown')
    expect((wrapper.vm as any).form.tags).toContain('y')
  })

  it('navigates user suggestions with keyboard and picks on enter', async () => {
    const wrapper = mount(TaskEditor as any, { props: propsBase as any })
    const reporter = wrapper.findAll('input.input').find(i => i.attributes('placeholder')?.includes('Reporter'))!
    await reporter.trigger('focus')
    await reporter.setValue('a')
    await reporter.trigger('keydown', { key: 'ArrowDown' })
    await reporter.trigger('keydown', { key: 'Enter' })
    expect((wrapper.vm as any).form.reporter).toBeTruthy()
  })

  it('preserves product custom field without remapping on save', async () => {
    const wrapper = mount(TaskEditor as any, {
      props: {
        ...propsBase,
        modelValue: {
          ...propsBase.modelValue,
          custom_fields: { product: 'frontend', other: 'value' },
        },
      } as any,
    })
    await (wrapper.vm as any).emitSave()
    const emitted = wrapper.emitted()
    const payload = (emitted['save']?.[0] as any)?.[0]
    expect(payload?.product).toBeUndefined()
    expect(payload?.custom_fields?.product).toBe('frontend')
    expect(payload?.custom_fields?.other).toBe('value')
  })
})
