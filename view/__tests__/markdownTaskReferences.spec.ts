import { describe, expect, it } from 'vitest'
import { renderMarkdownToHtml } from '../utils/markdown'

const projects = new Set(['DEV', 'ABC-OPS', 'TP'])
const taskReferences = {
    isProject: (prefix: string) => projects.has(prefix),
    href: (id: string) => `/task/${id}`,
}

function render(source: string) {
    const host = document.createElement('div')
    host.innerHTML = renderMarkdownToHtml(source, { taskReferences })
    return host
}

function links(host: HTMLElement) {
    return [...host.querySelectorAll<HTMLAnchorElement>('a.task-ref')].map((link) => ({
        id: link.dataset.taskRef,
        text: link.textContent,
        href: link.getAttribute('href'),
    }))
}

describe('task references in markdown', () => {
    it('links IDs of known projects and keeps the surrounding text', () => {
        const host = render('See DEV-12 for more info, and (ABC-OPS-3), TP-001.')
        expect(links(host)).toEqual([
            { id: 'DEV-12', text: 'DEV-12', href: '/task/DEV-12' },
            { id: 'ABC-OPS-3', text: 'ABC-OPS-3', href: '/task/ABC-OPS-3' },
            { id: 'TP-001', text: 'TP-001', href: '/task/TP-001' },
        ])
        expect(host.textContent).toBe('See DEV-12 for more info, and (ABC-OPS-3), TP-001.')
    })

    it('leaves unknown prefixes and look-alikes as text', () => {
        const host = render('UTF-8, SHA-256, dev-12, XDEV-12, DEV-12a, DEV-12-beta and release-DEV-4')
        expect(links(host)).toEqual([])
    })

    it('leaves code, code blocks and existing links alone', () => {
        const host = render(
            [
                '`DEV-1` and [DEV-2](https://example.com/DEV-2) and https://example.com/DEV-3',
                '',
                '```',
                'DEV-4',
                '```',
            ].join('\n'),
        )
        expect(links(host)).toEqual([])
        expect(host.querySelector('a[href="https://example.com/DEV-2"]')?.textContent).toBe('DEV-2')
    })

    it('links inside emphasis, lists and headings', () => {
        const host = render('# Follow-up to DEV-5\n\n- **DEV-6** blocks _DEV-7_')
        expect(links(host).map((link) => link.id)).toEqual(['DEV-5', 'DEV-6', 'DEV-7'])
    })

    it('keeps raw HTML inert while linking the IDs around it', () => {
        const host = render('<img src=x onerror="alert(1)"> DEV-8 <script>DEV-9</script>')
        expect(host.querySelector('img, script')).toBeNull()
        expect(host.querySelector('[onerror]')).toBeNull()
        expect(links(host).map((link) => link.id)).toEqual(['DEV-8', 'DEV-9'])
    })

    it('renders without references when not asked', () => {
        const host = document.createElement('div')
        host.innerHTML = renderMarkdownToHtml('See DEV-12')
        expect(host.querySelector('a')).toBeNull()
    })
})
