import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import TaskPanel from '../components/TaskPanel.vue'
import TaskPanelHost from '../components/TaskPanelHost.vue'
import TaskDetails from '../pages/TaskDetails.vue'

const addCommentMock = vi.hoisted(() =>
  vi.fn(async (_id: string, text: string) => ({
    id: 'PRJ-1',
    title: 'A',
    status: 'open',
    priority: 'low',
    task_type: 'task',
    created: '',
    modified: '',
    tags: [],
    relationships: {},
    comments: [{ date: new Date().toISOString(), text }],
    custom_fields: {},
  })),
)

vi.mock('../api/client', () => {
  return {
    api: {
      whoami: vi.fn(async () => 'tester'),
      getTask: vi.fn(async () => ({ id: 'PRJ-1', title: 'A', status: 'open', priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {} })),
      deleteTask: vi.fn(async () => ({ deleted: true })),
      setStatus: vi.fn(async (_id: string, _s: string) => ({ id: 'PRJ-1', title: 'A', status: _s, priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {} })),
      updateTask: vi.fn(async (_id: string, patch: any) => ({ id: 'PRJ-1', title: 'A', status: 'open', priority: 'low', task_type: 'task', created: '', modified: '', tags: [], relationships: {}, comments: [], custom_fields: {}, ...patch })),
      taskHistory: vi.fn(async () => []),
      taskCommitDiff: vi.fn(async () => ''),
      addComment: addCommentMock,
      referenceSnippet: vi.fn(async () => ({
        path: 'src/example.rs',
        start_line: 1,
        end_line: 1,
        highlight_start: 1,
        highlight_end: 1,
        lines: [],
        has_more_before: false,
        has_more_after: false,
        total_lines: 1,
      })),
      listTasks: vi.fn(async () => []),
      listProjects: vi.fn(async () => []),
    }
  }
})

vi.mock('vue-router', async () => {
  return {
    useRoute: () => ({ params: { id: 'PRJ-1' }, query: {} }),
    useRouter: () => ({ push: vi.fn(), replace: vi.fn() })
  }
})

vi.mock('../composables/useConfig', () => ({
  useConfig: () => ({
    statuses: { value: ['open', 'done'] },
    priorities: { value: ['low'] },
    types: { value: ['task'] },
    customFields: { value: ['product'] },
    tags: { value: [] },
    members: { value: [] as string[] },
    defaults: { value: { project: '', status: 'open', priority: 'low', type: 'task', reporter: '', assignee: '', tags: [], customFields: { product: '' } } },
    refresh: vi.fn(async () => { }),
  }),
}))


describe('TaskDetails comments', () => {
  it('posts a new comment', async () => {
    const wrapper = mount({
      components: { TaskDetails, TaskPanelHost },
      template: '<TaskDetails /><TaskPanelHost />',
    }, {
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    await wrapper.vm.$nextTick()
    const panel = wrapper.findComponent(TaskPanel)
    const initialComments = (panel.vm as any).task.comments
    expect(Array.isArray(initialComments)).toBe(true)
    expect(initialComments).toHaveLength(0)
    const commentBox = wrapper.find('textarea[placeholder="Add a comment…"]')
    await commentBox.setValue('hello **world**')
    const postButton = wrapper.find('button[aria-label="Add comment"]')
    expect(postButton.exists()).toBe(true)
    await postButton.trigger('click')
    await flushPromises()
    await wrapper.vm.$nextTick()
    await flushPromises()
    const comments = (panel.vm as any).task.comments || []
    expect(comments).toHaveLength(1)
    expect(comments[0].text).toBe('hello **world**')
    expect(addCommentMock).toHaveBeenCalledWith('PRJ-1', 'hello **world**')

    const renderedStrong = wrapper.find('strong')
    expect(renderedStrong.exists()).toBe(true)
    expect(renderedStrong.text()).toBe('world')
  })
})
