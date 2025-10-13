import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import TaskRow from '../components/TaskRow.vue'

const baseTask = {
  id: 'PRJ-1', title: 'Alpha', status: 'open', priority: 'low', task_type: 'task',
  created: '', modified: '', tags: ['one'], relationships: {}, comments: [], custom_fields: {}
}

describe('TaskRow inline edits', () => {
  it('emits update-title on save', async () => {
    const wrapper = mount(TaskRow, { props: { task: baseTask } })
    await wrapper.find('button[title="Edit title"]').trigger('click')
    const input = wrapper.find('input.input')
    await input.setValue('New Title')
    await input.trigger('keyup.enter')
    const evt = wrapper.emitted('update-title') as any[]
    expect(evt?.[0]?.[0]).toEqual({ id: 'PRJ-1', title: 'New Title' })
  })

  it('emits update-tags when toggling save', async () => {
    const wrapper = mount(TaskRow, { props: { task: baseTask } })
    const btns = wrapper.findAll('button')
    const editTagsBtn = btns.filter(b => b.text().includes('Edit tags'))[0]
    await editTagsBtn.trigger('click')
    const input = wrapper.find('input.input')
    await input.setValue('a, b, c')
    await editTagsBtn.trigger('click')
    const evt = wrapper.emitted('update-tags') as any[]
    expect(evt?.[0]?.[0]).toEqual({ id: 'PRJ-1', tags: ['a', 'b', 'c'] })
  })

  it('emits set-status on quick toggle', async () => {
    const wrapper = mount(TaskRow, { props: { task: baseTask, statuses: ['open', 'in-progress', 'done'] } })
    const statusBtn = wrapper.find('button.status')
    await statusBtn.trigger('click')
    const evt = wrapper.emitted('set-status') as any[]
    expect(evt?.[0]?.[0]).toEqual({ id: 'PRJ-1', status: 'in-progress' })
  })
})
