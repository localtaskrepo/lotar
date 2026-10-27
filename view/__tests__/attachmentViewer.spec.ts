import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AttachmentViewer from '../components/AttachmentViewer.vue'
import { renderMarkdownToHtml } from '../utils/markdown'
import { attachmentPreviewUrl, formatFileSize, moveTo, reorderSubset, taskAttachments } from '../utils/taskAttachments'

const last = <T>(items: T[]): T => items[items.length - 1]!
import { dismissToast, showToast, toasts } from '../components/toast'

const HASH = '0123456789abcdef0123456789abcdef'

// DEV-216: preview sizing, reordering helpers, inline attachment images,
// actionable toasts and the in-app viewer.
describe('attachment helpers', () => {
    it('asks the server for a preview sized for the slot', () => {
        const url = `/api/attachments/h/${HASH}/a.png?project=SHOP`
        expect(attachmentPreviewUrl(url, 24)).toBe(`${url}&w=96`)
        expect(attachmentPreviewUrl(url, 320)).toBe(`${url}&w=480`)
        expect(attachmentPreviewUrl(url, 4000)).toBe(`${url}&w=1600`)
        expect(attachmentPreviewUrl(`/api/attachments/h/${HASH}/a.png`, 100)).toBe(
            `/api/attachments/h/${HASH}/a.png?w=240`,
        )
        expect(attachmentPreviewUrl('/api/attachments/get?path=a.png', 100)).toBe('/api/attachments/get?path=a.png')
    })

    it('reorders a group in place and moves single entries', () => {
        expect(reorderSubset(['img1', 'doc', 'img2', 'img3'], ['img3', 'img1', 'img2'])).toEqual([
            'img3',
            'doc',
            'img1',
            'img2',
        ])
        expect(moveTo(['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'a', 'b'])
        expect(moveTo(['a', 'b', 'c'], 'a', 9)).toEqual(['b', 'c', 'a'])
        expect(moveTo(['a', 'b', 'c'], 'b', -1)).toEqual(['b', 'a', 'c'])
    })

    it('formats file sizes', () => {
        expect(formatFileSize(512)).toBe('512 B')
        expect(formatFileSize(1536)).toBe('1.5 KB')
        expect(formatFileSize(840 * 1024)).toBe('840 KB')
        expect(formatFileSize(5 * 1024 * 1024)).toBe('5.0 MB')
    })
})

describe('attachment links in markdown', () => {
    const source = `See ![shot](attachment:shot.${HASH}.png) and [spec](attachment:spec.${HASH}.pdf).`

    it('resolves attachment links when the task is known', () => {
        const html = renderMarkdownToHtml(source, { resolveAttachment: (stored) => `/files/${stored}?project=SHOP` })
        expect(html).toContain(`<img src="/files/shot.${HASH}.png?project=SHOP" alt="shot">`)
        expect(html).toContain(`<a href="/files/spec.${HASH}.pdf?project=SHOP">spec</a>`)
    })

    it('drops them otherwise, like any unknown scheme', () => {
        expect(renderMarkdownToHtml(source)).not.toContain('attachment:')
    })
})

describe('toasts with an action', () => {
    afterEach(() => {
        toasts.value = []
    })

    it('carries the action and can be dismissed early', () => {
        const run = vi.fn()
        const id = showToast('Removed a.png', undefined, 6000, { label: 'Undo', run })
        expect(toasts.value[0]).toMatchObject({ message: 'Removed a.png', action: { label: 'Undo' } })
        dismissToast(id)
        expect(toasts.value).toEqual([])
    })
})

describe('AttachmentViewer', () => {
    const views = taskAttachments({
        id: 'SHOP-1',
        references: [
            { attachment: `shot.${HASH}.png` },
            { attachment: `scan.${HASH}.tiff` },
            { attachment: `notes.${HASH}.zip` },
        ],
    })

    let mounted: Array<ReturnType<typeof mount>> = []
    afterEach(() => {
        mounted.forEach((wrapper) => wrapper.unmount())
        mounted = []
        document.body.innerHTML = ''
    })

    function mountViewer(openIndex: number | null) {
        const wrapper = mount(AttachmentViewer, {
            props: {
                attachments: views,
                openIndex,
                info: { [views[0]!.path]: { attachment: views[0]!.path, size: 2048, width: 640, height: 480 } },
            },
            global: { stubs: { Teleport: true } },
            attachTo: document.body,
        })
        mounted.push(wrapper)
        return wrapper
    }

    it('stays closed without an index', () => {
        expect(mountViewer(null).find('.attachment-viewer').exists()).toBe(false)
    })

    it('shows the image in full with its details and steps through attachments', async () => {
        const wrapper = mountViewer(0)
        await flushPromises()
        expect(wrapper.find('.attachment-viewer__image').attributes('src')).toBe(views[0]!.url)
        expect(wrapper.find('.attachment-viewer__details').text()).toBe('640×480 · 2.0 KB')
        expect(wrapper.find('.attachment-viewer__count').text()).toBe('1 / 3')

        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
        expect(last(wrapper.emitted('update:openIndex')!)).toEqual([1])
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft' }))
        expect(last(wrapper.emitted('update:openIndex')!)).toEqual([2])
    })

    it('uses the converted preview for TIFF and offers a download for other files', async () => {
        const tiff = mountViewer(1)
        await flushPromises()
        expect(tiff.find('.attachment-viewer__image').attributes('src')).toBe(`${views[1]!.url}&w=1600`)
        tiff.unmount()
        mounted = mounted.filter((wrapper) => wrapper !== tiff)

        const zip = mountViewer(2)
        await flushPromises()
        expect(zip.find('.attachment-viewer__fallback').text()).toContain('No preview for ZIP files')
        expect(zip.find('.attachment-viewer__fallback a').attributes('href')).toBe(`${views[2]!.url}&download=1`)
    })

    it('closes on Escape without letting the key reach the task panel', async () => {
        const wrapper = mountViewer(0)
        await flushPromises()
        const panelListener = vi.fn()
        window.addEventListener('keydown', panelListener)
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
        window.removeEventListener('keydown', panelListener)
        expect(panelListener).not.toHaveBeenCalled()
        expect(wrapper.emitted('close')).toHaveLength(1)
        expect(last(wrapper.emitted('update:openIndex')!)).toEqual([null])
    })
})
