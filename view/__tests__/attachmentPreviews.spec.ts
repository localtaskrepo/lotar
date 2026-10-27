import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AttachmentTile from '../components/AttachmentTile.vue'
import BoardCardMeta from '../components/BoardCardMeta.vue'
import TaskHoverCardBody from '../components/TaskHoverCardBody.vue'
import TaskTable from '../components/TaskTable.vue'
import { attachmentKind, taskAttachments } from '../utils/taskAttachments'

vi.mock('../api/client', () => ({ api: { showConfig: vi.fn(async () => ({})) } }))

const HASH = '0123456789abcdef0123456789abcdef'

function task(references: Array<{ attachment?: string; link?: string }>, overrides: Record<string, unknown> = {}) {
    return {
        id: 'SHOP-3',
        title: 'Dashboard layout',
        status: 'Todo',
        priority: 'Medium',
        task_type: 'Feature',
        tags: [],
        sprints: [],
        due_date: '',
        modified: '2026-10-01T00:00:00Z',
        created: '2026-10-01T00:00:00Z',
        references,
        ...overrides,
    } as any
}

const mixed = task([
    { attachment: `notes.${HASH}.pdf` },
    { link: 'https://example.com' },
    { attachment: `screen.${HASH}.png` },
    { attachment: `data.${HASH}.csv` },
    { attachment: `photo.${HASH}.jpg` },
    { attachment: `screen.${HASH}.png` },
])

beforeEach(() => localStorage.clear())

describe('attachment helpers (DEV-204)', () => {
    it('classifies attachments by extension', () => {
        expect(attachmentKind(`a.${HASH}.PNG`)).toBe('image')
        expect(attachmentKind(`a.${HASH}.pdf`)).toBe('pdf')
        expect(attachmentKind(`a.${HASH}.csv`)).toBe('sheet')
        expect(attachmentKind(`a.${HASH}.mp4`)).toBe('video')
        expect(attachmentKind(`a.${HASH}.zip`)).toBe('archive')
        expect(attachmentKind('README')).toBe('file')
    })

    it('lists a task’s attachments once, in order, with display names and URLs', () => {
        const views = taskAttachments(mixed)
        expect(views.map((view) => [view.name, view.kind, view.label])).toEqual([
            ['notes.pdf', 'pdf', 'PDF'],
            ['screen.png', 'image', 'PNG'],
            ['data.csv', 'sheet', 'CSV'],
            ['photo.jpg', 'image', 'JPG'],
        ])
        expect(views[1]!.url).toBe(`/api/attachments/h/${HASH}/screen.png?project=SHOP`)
        expect(taskAttachments(task([]))).toEqual([])
    })
})

describe('AttachmentTile', () => {
    it('shows images and labels other files', async () => {
        const [pdf, image] = taskAttachments(mixed)
        const imageTile = mount(AttachmentTile, { props: { attachment: image!, size: 24 } })
        expect(imageTile.find('img').attributes('src')).toBe(image!.url)
        expect(imageTile.find('img').attributes('loading')).toBe('lazy')
        expect(imageTile.find('img').attributes('style')).toContain('24px')

        const pdfTile = mount(AttachmentTile, { props: { attachment: pdf! } })
        expect(pdfTile.find('img').exists()).toBe(false)
        expect(pdfTile.text()).toBe('PDF')
        expect(pdfTile.classes()).toContain('attachment-tile--pdf')

        // A broken image falls back to the file tile.
        await imageTile.find('img').trigger('error')
        expect(imageTile.find('img').exists()).toBe(false)
        expect(imageTile.text()).toBe('PNG')
    })
})

describe('card and hover previews', () => {
    const sprint = { label: () => '', stateClass: () => '', tooltip: () => '' }

    it('counts attachments on cards only while the field is shown', () => {
        const props = { task: mixed, hasHeader: true, dueInfo: { label: '', overdue: false }, modifiedInfo: '', sprint }
        const shown = mount(BoardCardMeta, { props: { ...props, fields: { attachments: true } } })
        expect(shown.find('.task-meta__attachments').text()).toBe('4')
        expect(shown.find('.task-meta__attachments').attributes('aria-label')).toBe('4 attachments')
        const hidden = mount(BoardCardMeta, { props: { ...props, fields: { attachments: false } } })
        expect(hidden.find('.task-meta__attachments').exists()).toBe(false)
    })

    it('previews the first image in the hover card and lists the other files', async () => {
        const wrapper = mount(TaskHoverCardBody, { props: { task: mixed, fields: {} } })
        await flushPromises()
        expect(wrapper.find('.task-hover-card__preview img').attributes('src')).toContain('screen.png')
        const files = wrapper.findAll('.task-hover-card__file').map((file) => file.text())
        expect(files).toEqual(['PDFnotes.pdf', 'CSVdata.csv', 'photo.jpg'])

        const off = mount(TaskHoverCardBody, { props: { task: mixed, fields: { attachments: false } } })
        await flushPromises()
        expect(off.find('.task-hover-card__preview').exists()).toBe(false)
        expect(off.find('.task-hover-card__files').exists()).toBe(false)
    })
})

describe('task table attachments column', () => {
    it('shows up to three tiles and a count of the rest', () => {
        const many = task([
            { attachment: `a.${HASH}.png` },
            { attachment: `b.${HASH}.pdf` },
            { attachment: `c.${HASH}.csv` },
            { attachment: `d.${HASH}.zip` },
            { attachment: `e.${HASH}.txt` },
        ])
        const wrapper = mount(TaskTable, { props: { tasks: [many, task([], { id: 'SHOP-4' })], projectKey: 'SHOP' } })
        const header = wrapper.findAll('th').find((th) => th.text().includes('Attachments'))
        expect(header).toBeTruthy()
        expect(header!.find('.header-button__sort').exists()).toBe(false)
        const cells = wrapper.findAll('.task-table__cell--attachments')
        expect(cells[0]!.findAll('.attachment-tile')).toHaveLength(3)
        expect(cells[0]!.find('.task-table__attachments-more').text()).toBe('+2')
        expect(cells[1]!.text()).toBe('—')
    })
})
