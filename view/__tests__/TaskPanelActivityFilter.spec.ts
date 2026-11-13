import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it } from 'vitest'
import TaskPanelCommentsTab from '../components/task-panel/TaskPanelCommentsTab.vue'
import TaskPanelHistoryTab from '../components/task-panel/TaskPanelHistoryTab.vue'
import { COMMENT_FILTER_KEY, HISTORY_FILTER_KEY } from '../composables/task-panel/useActivityFilter'

const commentsProps = (comments: Array<{ date: string; author?: string; text: string }>) => ({
    mode: 'edit' as const,
    task: { comments },
    newComment: '',
    submitting: false,
    editingCommentIndex: null,
    editingCommentText: '',
    editingCommentSubmitting: false,
    formatDate: (input: string) => input,
    setEditingTextarea: () => {},
})

const mountComments = (comments: Array<{ date: string; author?: string; text: string }>) =>
    mount(TaskPanelCommentsTab, {
        props: commentsProps(comments),
        global: { stubs: { MarkdownContent: { props: ['source'], template: '<p>{{ source }}</p>' } } },
    })

const historyLog = [
    { at: '3', actor: 'automation', changes: [{ field: 'status', old: 'Todo', new: 'InProgress' }] },
    {
        at: '2',
        actor: 'claude',
        changes: [
            { field: 'tags', old: '', new: 'ui' },
            { field: 'comment_added', old: null, new: null },
        ],
    },
    { at: '1', actor: 'mallox', changes: [{ field: 'assignee', old: '', new: 'claude' }] },
]

const mountHistory = () =>
    mount(TaskPanelHistoryTab, {
        props: {
            mode: 'edit',
            changeLog: historyLog,
            formatDate: (input: string) => input,
            formatFieldName: (field: string) => field,
            formatChangeValue: (value?: string | null) => value ?? '',
        },
    })

const chip = (wrapper: ReturnType<typeof mount>, label: string) => {
    const found = wrapper.findAll('.task-panel__activity-filter-chip').find((item) => item.text() === label)
    if (!found) throw new Error(`no chip ${label}`)
    return found
}

describe('Comments and History tab filters (DEV-502)', () => {
    beforeEach(() => localStorage.clear())

    it('hides an author’s comments, says how many are hidden, and keeps the choice for the next task', async () => {
        const comments = [
            { date: '1', author: 'automation', text: 'Job started' },
            { date: '2', author: 'claude', text: 'Done' },
            { date: '3', text: 'old' },
        ]
        const wrapper = mountComments(comments)
        await chip(wrapper, 'automation').trigger('click')
        expect(chip(wrapper, 'automation').attributes('aria-pressed')).toBe('false')
        expect(wrapper.findAll('.task-panel__comment').map((item) => item.find('p').text())).toEqual(['Done', 'old'])
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 3 comments hidden.')
        expect(JSON.parse(localStorage.getItem(COMMENT_FILTER_KEY) ?? '{}').authors).toEqual(['automation'])

        const next = mountComments(comments)
        expect(next.findAll('.task-panel__comment')).toHaveLength(2)
        await next.find('.task-panel__activity-filter-show-all').trigger('click')
        expect(next.findAll('.task-panel__comment')).toHaveLength(3)
        expect(next.find('.task-panel__activity-filter-hidden').exists()).toBe(false)
    })

    it('offers Hide automation on a task whose only comments are automation’s', async () => {
        const wrapper = mountComments([{ date: '1', author: 'automation', text: 'Job started' }])
        await chip(wrapper, 'automation').trigger('click')
        expect(wrapper.findAll('.task-panel__comment')).toHaveLength(0)
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 1 comment hidden.')
    })

    it('offers the field and author chips on a history with one field and one author', async () => {
        const wrapper = mount(TaskPanelHistoryTab, {
            props: {
                mode: 'edit',
                changeLog: historyLog.slice(0, 1),
                formatDate: (input: string) => input,
                formatFieldName: (field: string) => field,
                formatChangeValue: (value?: string | null) => value ?? '',
            },
        })
        expect(chip(wrapper, 'Status').exists()).toBe(true)
        await chip(wrapper, 'automation').trigger('click')
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 1 change hidden.')
    })

    it('shows no filter on a task without comments', () => {
        const wrapper = mountComments([])
        expect(wrapper.find('.task-panel__activity-filter').exists()).toBe(false)
    })

    it('filters history by field group and by author', async () => {
        const wrapper = mountHistory()
        const fieldsShown = () => wrapper.findAll('.task-panel__history-change strong').map((item) => item.text())
        expect(fieldsShown()).toEqual(['status', 'tags', 'comment_added', 'assignee'])

        await chip(wrapper, 'Other').trigger('click')
        expect(fieldsShown()).toEqual(['status', 'tags', 'assignee'])
        // A change hidden within an update that stays shown still counts.
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 4 changes hidden.')
        expect(wrapper.find('.task-panel__activity-filter-show-all').exists()).toBe(true)

        await chip(wrapper, 'automation').trigger('click')
        expect(fieldsShown()).toEqual(['tags', 'assignee'])
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('2 of 4 changes hidden.')
        expect(JSON.parse(localStorage.getItem(HISTORY_FILTER_KEY) ?? '{}')).toEqual({
            authors: ['automation'],
            fields: ['other'],
        })

        await chip(wrapper, 'Tags').trigger('click')
        await chip(wrapper, 'Assignee').trigger('click')
        expect(wrapper.text()).toContain('All updates are hidden by the filter')
        await wrapper.find('.task-panel__activity-filter-show-all').trigger('click')
        expect(fieldsShown()).toHaveLength(4)
    })
})
