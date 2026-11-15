import { describe, expect, it } from 'vitest'
import { codeFileCandidate, formatFileLocation, parseFileLocation, textFileCandidates } from '../utils/fileReferences'
import { splitHighlightedLines } from '../utils/highlight'
import { renderMarkdownToHtml } from '../utils/markdown'

// What the server would answer for this repository.
const repository = new Map([
    ['docs/help/serve.md', 'docs/help/serve.md'],
    ['api_types.rs', 'crates/lotar-core/src/api_types.rs'],
    ['view/api/types.ts', 'view/api/types.ts'],
    ['src/x-12.rs', 'src/x-12.rs'],
    ['img/flow.png', 'docs/img/flow.png'],
])
const asked: string[] = []
const fileReferences = {
    resolve: (reference: string) => {
        asked.push(reference)
        return repository.get(reference) ?? null
    },
    href: (location: { path: string; line?: number; endLine?: number }) => `?file=${formatFileLocation(location)}`,
    src: (path: string) => `/api/files/content?path=${path}`,
}

function render(source: string) {
    const host = document.createElement('div')
    host.innerHTML = renderMarkdownToHtml(source, {
        fileReferences,
        taskReferences: { isProject: (prefix) => prefix === 'X' || prefix === 'DEV', href: (id) => `/task/${id}` },
    })
    return host
}

function links(host: HTMLElement) {
    return [...host.querySelectorAll<HTMLAnchorElement>('a.file-ref')].map((link) => ({
        path: link.dataset.fileRef,
        line: link.dataset.fileLine,
        text: link.textContent,
        href: link.getAttribute('href'),
    }))
}

describe('file locations', () => {
    it('reads lines and ranges in the usual notations', () => {
        expect(parseFileLocation('src/a.rs')).toEqual({ path: 'src/a.rs' })
        expect(parseFileLocation('src/a.rs:12')).toEqual({ path: 'src/a.rs', line: 12 })
        expect(parseFileLocation('src/a.rs:12-20')).toEqual({ path: 'src/a.rs', line: 12, endLine: 20 })
        expect(parseFileLocation('src/a.rs:12:5')).toEqual({ path: 'src/a.rs', line: 12 })
        expect(parseFileLocation('src/a.rs#L12-L20')).toEqual({ path: 'src/a.rs', line: 12, endLine: 20 })
        expect(parseFileLocation('src/a.rs#3-4')).toEqual({ path: 'src/a.rs', line: 3, endLine: 4 })
        expect(formatFileLocation({ path: 'a.md', line: 3, endLine: 4 })).toBe('a.md:3-4')
    })

    it('takes inline code only when it is a path', () => {
        expect(codeFileCandidate('docs/a.md:3')).toEqual({ path: 'docs/a.md', line: 3 })
        expect(codeFileCandidate('.github/CODEOWNERS')).toEqual({ path: '.github/CODEOWNERS' })
        for (const text of ['npm run lint', 'lotar', '--tasks-dir', 'https://x.dev/a.md', 'crates/', 'a(b).rs']) {
            expect(codeFileCandidate(text), text).toBeNull()
        }
    })

    it('finds file names in prose without sentence punctuation', () => {
        expect(textFileCandidates('See docs/a.md, then b.rs:4. Or /abs/c.ts.').map((found) => found.text)).toEqual([
            'docs/a.md',
            'b.rs:4',
            '/abs/c.ts',
        ])
        expect(textFileCandidates('version v0.15.0 or 1.2.3').map((found) => found.text)).toEqual([])
    })
})

describe('file references in markdown', () => {
    it('links inline code, prose names and markdown links that name files', () => {
        const host = render(
            'Edit `docs/help/serve.md:12`, api_types.rs and [the types](view/api/types.ts#L3-L5). Not `missing.md`.',
        )
        expect(links(host)).toEqual([
            {
                path: 'docs/help/serve.md',
                line: '12',
                text: 'docs/help/serve.md:12',
                href: '?file=docs/help/serve.md:12',
            },
            {
                path: 'crates/lotar-core/src/api_types.rs',
                line: undefined,
                text: 'api_types.rs',
                href: '?file=crates/lotar-core/src/api_types.rs',
            },
            { path: 'view/api/types.ts', line: '3', text: 'the types', href: '?file=view/api/types.ts:3-5' },
        ])
        // The inline code stays code inside its link.
        expect(host.querySelector('a.file-ref > code')?.textContent).toBe('docs/help/serve.md:12')
        expect(host.textContent).toContain('Not missing.md.')
    })

    it('leaves code blocks, web links and attachments alone', () => {
        asked.length = 0
        const host = render(
            '```\ndocs/help/serve.md\n```\n\n[web](https://example.com/docs/help/serve.md) [shot](/api/attachments/get?path=a.png)',
        )
        expect(links(host)).toEqual([])
        expect(asked).toEqual([])
    })

    it('loads relative images from the repository', () => {
        const host = render('![flow](img/flow.png) ![gone](img/missing.png) ![web](https://example.com/a.png)')
        expect([...host.querySelectorAll('img')].map((image) => image.getAttribute('src'))).toEqual([
            '/api/files/content?path=docs/img/flow.png',
            'img/missing.png',
            'https://example.com/a.png',
        ])
    })

    it('gives a relative image no source while its lookup is pending', () => {
        const host = document.createElement('div')
        host.innerHTML = renderMarkdownToHtml('![flow](img/flow.png)', {
            fileReferences: { ...fileReferences, resolve: () => undefined },
        })
        expect(host.querySelector('img')?.hasAttribute('src')).toBe(false)
        expect(host.querySelector('img')?.getAttribute('alt')).toBe('flow')
    })

    it('links a path before a task ID inside it can claim it', () => {
        const host = render('See src/x-12.rs and X-12.')
        expect(links(host).map((link) => link.text)).toEqual(['src/x-12.rs'])
        expect([...host.querySelectorAll('a.task-ref')].map((link) => link.textContent)).toEqual(['X-12'])
    })
})

describe('highlighted lines', () => {
    it('closes tokens at each line end and reopens them on the next', () => {
        const html =
            'a <span class="hljs-comment">/* one\ntwo */</span> b\n<span class="x"><span class="y">c</span></span>'
        expect(splitHighlightedLines(html)).toEqual([
            'a <span class="hljs-comment">/* one</span>',
            '<span class="hljs-comment">two */</span> b',
            '<span class="x"><span class="y">c</span></span>',
        ])
    })
})
