import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AttachmentTile from '../components/AttachmentTile.vue'
import BoardCardMeta from '../components/BoardCardMeta.vue'
import TaskHoverCardBody from '../components/TaskHoverCardBody.vue'
import TaskTable from '../components/TaskTable.vue'
import {
    attachmentKind,
    firstPreviewImage,
    imageFit,
    isAttachmentPreviewBroken,
    markAttachmentPreviewBroken,
    taskAttachments,
} from '../utils/taskAttachments'

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
        // A server preview sized for the tile, not the full image.
        expect(imageTile.find('img').attributes('src')).toBe(`${image!.url}&w=96`)
        expect(imageTile.find('img').attributes('loading')).toBe('lazy')
        expect(imageTile.find('img').attributes('style')).toContain('24px')

        const pdfTile = mount(AttachmentTile, { props: { attachment: pdf! } })
        expect(pdfTile.find('img').exists()).toBe(false)
        expect(pdfTile.text()).toBe('PDF')
        expect(pdfTile.classes()).toContain('attachment-tile--pdf')

        // A broken image falls back to the file tile and is remembered, so a
        // later tile for the same file does not try again.
        const [broken] = taskAttachments(task([{ attachment: `broken.${HASH}.png` }]))
        const brokenTile = mount(AttachmentTile, { props: { attachment: broken! } })
        await brokenTile.find('img').trigger('error')
        expect(brokenTile.find('img').exists()).toBe(false)
        expect(brokenTile.text()).toBe('PNG')
        expect(isAttachmentPreviewBroken(broken!.url)).toBe(true)
        expect(
            mount(AttachmentTile, { props: { attachment: broken! } })
                .find('img')
                .exists(),
        ).toBe(false)

        // As a cover it says why there is no picture.
        expect(mount(AttachmentTile, { props: { attachment: broken!, cover: true } }).text()).toBe('PNGNo preview')
    })

    it('fits each image to its shape once it loads', async () => {
        const [image] = taskAttachments(task([{ attachment: `wide.${HASH}.jpg` }]))
        const tile = mount(AttachmentTile, { props: { attachment: image!, cover: true } })
        const img = tile.find('img')
        expect(img.classes()).toContain('attachment-tile--fit-cover')
        const define = (key: string, value: number) =>
            Object.defineProperty(img.element, key, { value, configurable: true })
        define('naturalWidth', 2400)
        define('naturalHeight', 300)
        define('clientWidth', 300)
        define('clientHeight', 120)
        await img.trigger('load')
        expect(img.classes()).toContain('attachment-tile--fit-contain')
    })
})

describe('imageFit', () => {
    const box = { width: 300, height: 120 }
    it('never enlarges small images', () => {
        expect(imageFit({ width: 16, height: 16 }, box)).toBe('natural')
        expect(imageFit({ width: 300, height: 120 }, box)).toBe('natural')
    })
    it('shows vector art and transparent images whole', () => {
        expect(imageFit({ width: 2000, height: 1000 }, box, { vector: true })).toBe('contain')
        expect(imageFit({ width: 400, height: 400 }, box, { transparent: true })).toBe('contain')
    })
    it('crops tall screenshots from the top', () => {
        expect(imageFit({ width: 1200, height: 3200 }, box)).toBe('top')
    })
    it('covers photos but not shapes that would lose almost half', () => {
        expect(imageFit({ width: 3000, height: 1500 }, box)).toBe('cover')
        expect(imageFit({ width: 2400, height: 300 }, box)).toBe('contain')
        expect(imageFit({ width: 1000, height: 1000 }, box)).toBe('contain')
    })
    it('covers until the size is known', () => {
        expect(imageFit({ width: 0, height: 0 }, box)).toBe('cover')
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

    it('skips images that failed to load when choosing a preview', () => {
        const pair = task([{ attachment: `gone.${HASH}.png` }, { attachment: `kept.${HASH}.jpg` }])
        expect(firstPreviewImage(pair)!.name).toBe('gone.png')
        markAttachmentPreviewBroken(taskAttachments(pair)[0]!.url)
        expect(firstPreviewImage(pair)!.name).toBe('kept.jpg')
        markAttachmentPreviewBroken(taskAttachments(pair)[1]!.url)
        expect(firstPreviewImage(pair)).toBeNull()
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
