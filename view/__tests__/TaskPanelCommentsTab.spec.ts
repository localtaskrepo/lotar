import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { TaskComment } from '../api/types'
import TaskPanelCommentsTab from '../components/task-panel/TaskPanelCommentsTab.vue'

function mountTab(comments: TaskComment[]) {
    return mount(TaskPanelCommentsTab, {
        props: {
            mode: 'edit',
            task: { comments },
            newComment: '',
            submitting: false,
            editingCommentIndex: null,
            editingCommentText: '',
            editingCommentSubmitting: false,
            formatDate: (input: string) => input,
            setEditingTextarea: () => {},
        },
        global: { stubs: { MarkdownContent: { props: ['source'], template: '<p>{{ source }}</p>' } } },
    })
}

describe('TaskPanelCommentsTab (DEV-327)', () => {
    it('shows the author next to the date, and comments without one as before', () => {
        const wrapper = mountTab([
            { date: '2026-11-12', author: 'claude', text: 'new' },
            { date: '2026-01-01', text: 'old' },
        ])
        const metas = wrapper.findAll('.task-panel__comment-meta').map((meta) => meta.text())
        expect(wrapper.findAll('.task-panel__comment-edit')).toHaveLength(0)
        expect(metas).toEqual(['claude · 2026-11-12', '2026-01-01'])
    })

    it('shows a hand-off report under its text and offers a Hand-off type chip (DEV-490)', () => {
        const wrapper = mount(TaskPanelCommentsTab, {
            props: {
                mode: 'edit',
                task: {
                    comments: [
                        { date: '2026-11-12', author: 'claude', text: 'note' },
                        {
                            date: '2026-11-13',
                            author: 'claude',
                            text: 'done',
                            report: { checks: [{ command: 'npm test', result: 'passed' }] },
                        },
                    ],
                },
                newComment: '',
                submitting: false,
                editingCommentIndex: null,
                editingCommentText: '',
                editingCommentSubmitting: false,
                formatDate: (input: string) => input,
                setEditingTextarea: () => {},
            },
            global: { stubs: { MarkdownContent: { props: ['source'], template: '<p>{{ source }}</p>' } } },
        })
        expect(wrapper.findAll('[data-testid="comment-report"]')).toHaveLength(1)
        const chips = wrapper.findAll('.task-panel__activity-filter-chip').map((chip) => chip.text())
        expect(chips).toEqual(expect.arrayContaining(['Comment', 'Hand-off']))
    })
})

describe('TaskPanelCommentsTab (DEV-491)', () => {
    it('mutes only the comments automation wrote', () => {
        const wrapper = mountTab([
            { date: '2026-11-13', author: 'automation', text: 'Agent job failed' },
            { date: '2026-11-13', author: 'claude', text: 'Fixed it' },
            { date: '2026-11-13', text: 'old' },
            {
                date: '2026-11-14',
                author: 'automation',
                text: 'Which?',
                question: { options: [{ key: 'yes', label: 'Yes' }] },
            },
        ])
        const muted = wrapper
            .findAll('.task-panel__comment')
            .map((comment) => comment.classes('task-panel__comment--automation'))
        expect(muted).toEqual([true, false, false, false])
    })
})
