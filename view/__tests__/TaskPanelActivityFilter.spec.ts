import type { TaskComment, TaskTimelineEntry } from '../api/types'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import TaskPanelCommentsTab from '../components/task-panel/TaskPanelCommentsTab.vue'
import TaskPanelTimelineTab from '../components/task-panel/TaskPanelTimelineTab.vue'
import { COMMENT_FILTER_KEY, TIMELINE_FILTER_KEY } from '../composables/task-panel/useActivityFilter'

const timeline = vi.hoisted(() => ({ entries: [] as unknown[] }))
const sseHandlers = vi.hoisted(() => new Map<string, (event: MessageEvent) => void>())
const getAgentJobLogs = vi.hoisted(() => vi.fn())

vi.mock('../api/client', () => ({
    api: {
        taskTimeline: vi.fn(async (id: string) => ({ task_id: id, entries: timeline.entries })),
        getAgentJobLogs,
    },
}))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        on: (kind: string, handler: (event: MessageEvent) => void) => sseHandlers.set(kind, handler),
        off: vi.fn(),
        close: vi.fn(),
    }),
}))

const commentsProps = (comments: TaskComment[]) => ({
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

const markdownStub = { MarkdownContent: { props: ['source'], template: '<p>{{ source }}</p>' } }

const mountComments = (comments: TaskComment[]) =>
    mount(TaskPanelCommentsTab, {
        props: commentsProps(comments),
        global: { stubs: markdownStub },
    })

const job = {
    id: 'job-1',
    ticket_id: 'DEV-1',
    runner: 'claude',
    agent: 'fix',
    status: 'running',
    created_at: '2026-01-01T00:04:00Z',
    started_at: '2026-01-01T00:04:00Z',
    model: 'sonnet',
    summary: 'Working on it',
    usage: { cost_usd: 0.5 },
}

const timelineEntries: TaskTimelineEntry[] = [
    { kind: 'change', at: '2026-01-01T00:01:00Z', actor: 'mallox', changes: [{ field: 'assignee', new: 'claude' }] },
    {
        kind: 'change',
        at: '2026-01-01T00:02:00Z',
        actor: 'claude',
        changes: [
            { field: 'tags', old: '', new: 'ui' },
            { field: 'priority', old: 'Low', new: 'High' },
        ],
    },
    { kind: 'comment', at: '2026-01-01T00:03:00Z', index: 0, comment: { date: '', author: 'claude', text: 'Plan' } },
    { kind: 'job', at: '2026-01-01T00:04:00Z', job },
    {
        kind: 'commit',
        at: '2026-01-01T00:05:00Z',
        commit: 'abcdef123456',
        author: 'Ravi',
        email: 'r@example.com',
        message: 'DEV-1 Fix',
    },
]

const mountTimeline = async (entries: TaskTimelineEntry[] = timelineEntries) => {
    timeline.entries = entries
    const wrapper = mount(TaskPanelTimelineTab, {
        props: {
            mode: 'edit',
            taskId: 'DEV-1',
            formatDate: (input: string) => input,
            formatFieldName: (field: string) => field,
            formatChangeValue: (value?: string | null) => value ?? '',
            formatCommit: (value: string) => value.slice(0, 7),
        },
        global: { stubs: markdownStub },
    })
    await flushPromises()
    return wrapper
}

const chip = (wrapper: ReturnType<typeof mount>, label: string) => {
    const found = wrapper.findAll('.task-panel__activity-filter-chip').find((item) => item.text() === label)
    if (!found) throw new Error(`no chip ${label}`)
    return found
}

describe('Comments and Timeline tab filters (DEV-502, DEV-518)', () => {
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

    it('filters question and answer types independently, keeping edit indexes and nested answers correct', async () => {
        const comments: TaskComment[] = [
            { date: 'q', author: 'automation', text: 'Which?', question: { options: [{ key: 'yes', label: 'Yes' }] } },
            { date: 'a', author: 'mallox', text: 'Answer: yes', answer: { to: 'q', choice: ['yes'] } },
            { date: 'c', author: 'claude', text: 'Ordinary comment' },
        ]
        const wrapper = mountComments(comments)
        expect(wrapper.findAll('.task-panel__comment')).toHaveLength(2)
        await chip(wrapper, 'Answer').trigger('click')
        expect(wrapper.find('[data-testid="comment-answer"]').exists()).toBe(false)
        expect(wrapper.text()).toContain('Answer hidden by filter')
        expect(wrapper.find('[data-testid="question-prompt"]').exists()).toBe(false)
        await chip(wrapper, 'Question').trigger('click')
        expect(wrapper.findAll('.task-panel__comment')).toHaveLength(1)
        const next = mountComments(comments)
        expect(next.findAll('.task-panel__comment')).toHaveLength(1)
        expect(next.find('.task-panel__activity-filter-hidden').text()).toContain('2 of 3 comments hidden.')
        await next.get('.task-panel__comment-action').trigger('click')
        expect(next.emitted('startEdit')).toEqual([[2]])
        await chip(next, 'Answer').trigger('click')
        expect(next.text()).toContain('Answer: yes')
    })

    it('saves an optional override for open questions without exposing answered questions', async () => {
        const comments: TaskComment[] = [
            {
                date: 'open',
                author: 'automation',
                text: 'Open?',
                question: { options: [{ key: 'yes', label: 'Yes' }] },
            },
            {
                date: 'closed',
                author: 'automation',
                text: 'Closed?',
                question: { options: [{ key: 'yes', label: 'Yes' }] },
            },
            { date: 'answer', author: 'mallox', text: 'Yes', answer: { to: 'closed', choice: ['yes'] } },
        ]
        const wrapper = mountComments(comments)
        await chip(wrapper, 'automation').trigger('click')
        await chip(wrapper, 'Question').trigger('click')
        expect(wrapper.findAll('[data-testid="comment-question"]')).toHaveLength(0)
        await wrapper.get('input[aria-label="Always show open questions"]').setValue(true)
        expect(wrapper.findAll('[data-testid="comment-question"]')).toHaveLength(1)
        expect(wrapper.text()).toContain('Open?')
        expect(wrapper.text()).not.toContain('Closed?')
        expect(JSON.parse(localStorage.getItem(COMMENT_FILTER_KEY) ?? '{}').alwaysShowOpenQuestions).toBe(true)
        const next = mountComments(comments)
        expect(next.findAll('[data-testid="comment-question"]')).toHaveLength(1)
        await next.get('input[aria-label="Always show open questions"]').setValue(false)
        expect(next.findAll('[data-testid="comment-question"]')).toHaveLength(0)
    })

    it('offers Hide automation on a task whose only comments are automation’s', async () => {
        const wrapper = mountComments([{ date: '1', author: 'automation', text: 'Job started' }])
        await chip(wrapper, 'automation').trigger('click')
        expect(wrapper.findAll('.task-panel__comment')).toHaveLength(0)
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 1 comment hidden.')
    })

    it('offers the kind and author chips on a timeline with one entry', async () => {
        const wrapper = await mountTimeline(timelineEntries.slice(0, 1))
        expect(chip(wrapper, 'Changes').exists()).toBe(true)
        await chip(wrapper, 'mallox').trigger('click')
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('1 of 1 entry hidden.')
    })

    it('shows no filter on a task without comments', () => {
        const wrapper = mountComments([])
        expect(wrapper.find('.task-panel__activity-filter').exists()).toBe(false)
    })

    it('lists the timeline newest first and filters it by kind, field and author', async () => {
        const wrapper = await mountTimeline()
        const kinds = () => wrapper.findAll('.timeline__item').map((item) => item.attributes('data-kind'))
        expect(kinds()).toEqual(['commit', 'job', 'comment', 'change', 'change'])
        expect(wrapper.get('[data-testid="timeline-job"]').text()).toContain('claude · sonnet')
        expect(wrapper.get('[data-testid="timeline-job"]').text()).toContain('$0.50')

        await chip(wrapper, 'Commits').trigger('click')
        expect(kinds()).toEqual(['job', 'comment', 'change', 'change'])
        await chip(wrapper, 'Other').trigger('click')
        expect(wrapper.findAll('.task-panel__history-change strong').map((item) => item.text())).toEqual([
            'tags',
            'assignee',
        ])
        await chip(wrapper, 'claude').trigger('click')
        expect(kinds()).toEqual(['job', 'change'])
        expect(wrapper.find('.task-panel__activity-filter-hidden').text()).toContain('3 of 5 entries hidden.')
        expect(JSON.parse(localStorage.getItem(TIMELINE_FILTER_KEY) ?? '{}')).toEqual({
            authors: ['claude'],
            fields: ['kind:commit', 'other'],
        })

        await wrapper.find('.task-panel__activity-filter-show-all').trigger('click')
        expect(kinds()).toHaveLength(5)
    })

    it('loads an agent run’s thread when opened and follows it live', async () => {
        getAgentJobLogs.mockResolvedValue({
            job,
            events: [{ kind: 'agent_job_message', sequence: 1, at: '2026-01-01T00:04:01Z', message: 'Reading' }],
        })
        const wrapper = await mountTimeline()
        expect(wrapper.find('.log-list').exists()).toBe(false)
        await wrapper.get('.timeline__toggle').trigger('click')
        await flushPromises()
        expect(getAgentJobLogs).toHaveBeenCalledWith('job-1')
        expect(wrapper.get('.log-list').text()).toContain('Reading')

        const event = {
            id: 'job-1',
            ticket_id: 'DEV-1',
            sequence: 2,
            at: '2026-01-01T00:04:02Z',
            message: 'Done reading',
        }
        sseHandlers.get('agent_job_message')?.({ data: JSON.stringify(event) } as MessageEvent)
        sseHandlers.get('agent_job_message')?.({
            data: JSON.stringify({ ...event, ticket_id: 'DEV-2', id: 'job-9' }),
        } as MessageEvent)
        await flushPromises()
        expect(wrapper.get('.log-list').text()).toContain('Done reading')
    })
})
