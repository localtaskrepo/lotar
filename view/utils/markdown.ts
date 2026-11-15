import DOMPurify from 'dompurify'
import { micromark } from 'micromark'
import { gfm, gfmHtml } from 'micromark-extension-gfm'
import { codeFileCandidate, parseFileLocation, textFileCandidates, type FileLocation } from './fileReferences'
import { parseTaskId } from './text'

export interface RenderMarkdownOptions {
    /**
     * Resolve `attachment:<stored name>` links and images (inserted when a
     * screenshot is pasted into a description) to their URLs. Without it they
     * are dropped by the sanitizer, like any unknown scheme.
     */
    resolveAttachment?: (stored: string) => string
    /**
     * Turn plain task IDs such as `DEV-12` into links. Only IDs whose project
     * prefix `isProject` accepts are linked, so `UTF-8` or `SHA-256` stay text.
     * Text inside links, inline code and code blocks is left alone.
     */
    taskReferences?: {
        isProject: (prefix: string) => boolean
        href: (id: string) => string
    }
    /**
     * Turn names of repository files into links: markdown links to relative
     * paths, inline code holding a path, and file names in prose. Only names
     * `resolve` maps to a repository path are linked, so a file that does not
     * exist stays text. Text inside code blocks is left alone. Images with a
     * relative source load the repository file through `src`.
     */
    fileReferences?: {
        /** Repository path of the file a name refers to; null or undefined when unknown. */
        resolve: (reference: string) => string | null | undefined
        href: (location: FileLocation) => string
        /** URL of a repository file's bytes, for images. */
        src: (path: string) => string
    }
}

/** Class and data attribute that mark a linked task reference. */
export const TASK_REFERENCE_CLASS = 'task-ref'
export const TASK_REFERENCE_ATTRIBUTE = 'data-task-ref'

/** Class and data attributes that mark a linked repository file. */
export const FILE_REFERENCE_CLASS = 'file-ref'
export const FILE_REFERENCE_ATTRIBUTE = 'data-file-ref'
export const FILE_LINE_ATTRIBUTE = 'data-file-line'
export const FILE_END_LINE_ATTRIBUTE = 'data-file-end-line'

// A run of ID characters ending in `-<digits>`, not glued to more ID characters
// on either side. The project prefix may contain dashes (`ABC-OPS-12`).
const TASK_REFERENCE_CANDIDATE =
    /(?<![\p{Alphabetic}\p{Number}_-])[\p{Alphabetic}\p{Number}][\p{Alphabetic}\p{Number}_-]*-[0-9]+(?![\p{Alphabetic}\p{Number}_-])/gu

export function renderMarkdownToHtml(source: string, options: RenderMarkdownOptions = {}): string {
    const markdown = source

    // micromark blanks unknown URL schemes, so resolve `attachment:` targets
    // in the source; the resolved same-origin URLs pass its checks.
    const resolve = options.resolveAttachment
    const withAttachments = resolve
        ? markdown.replace(/\]\(attachment:([^)\s]+)\)/g, (_match, stored: string) => `](${resolve(stored)})`)
        : markdown

    const rawHtml = micromark(withAttachments, {
        extensions: [gfm()],
        htmlExtensions: [gfmHtml()],
    })

    const safeHtml = DOMPurify.sanitize(rawHtml, {
        USE_PROFILES: {
            html: true,
        },
    })

    const { taskReferences, fileReferences } = options
    if (!taskReferences && !fileReferences) return safeHtml
    const template = document.createElement('template')
    template.innerHTML = safeHtml
    // Files first: a path such as `src/x-12.rs` must not become a task link.
    const files = fileReferences ? linkFileReferences(template.content, fileReferences) : false
    const tasks =
        taskReferences && /-[0-9]/.test(safeHtml) ? linkTaskReferences(template.content, taskReferences) : false
    return files || tasks ? template.innerHTML : safeHtml
}

// Hrefs that already go somewhere: a scheme, a fragment, a query, another
// host, or the API (resolved attachments).
const NON_FILE_HREF = /^(?:[a-z][a-z0-9+.-]*:|#|\?|\/\/|\/api\/)/i

function safeDecodeURI(uri: string) {
    try {
        return decodeURI(uri)
    } catch {
        return uri
    }
}

function fileLink(
    location: FileLocation,
    path: string,
    refs: NonNullable<RenderMarkdownOptions['fileReferences']>,
    link = document.createElement('a'),
) {
    link.classList.add(FILE_REFERENCE_CLASS)
    link.setAttribute(FILE_REFERENCE_ATTRIBUTE, path)
    if (location.line) link.setAttribute(FILE_LINE_ATTRIBUTE, String(location.line))
    if (location.endLine) link.setAttribute(FILE_END_LINE_ATTRIBUTE, String(location.endLine))
    link.href = refs.href({ ...location, path })
    return link
}

/**
 * Link file names in already-sanitized HTML. Builds the links with DOM APIs,
 * so it cannot reintroduce markup.
 */
function linkFileReferences(
    root: DocumentFragment,
    refs: NonNullable<RenderMarkdownOptions['fileReferences']>,
): boolean {
    let changed = false

    for (const image of root.querySelectorAll<HTMLImageElement>('img[src]')) {
        const src = image.getAttribute('src') ?? ''
        if (!src || NON_FILE_HREF.test(src)) continue
        const path = refs.resolve(safeDecodeURI(src))
        // A name that names no file keeps its source; while the lookup is
        // pending, no source, so the browser does not ask the app URL for it.
        if (path === null) continue
        if (path) image.src = refs.src(path)
        else image.removeAttribute('src')
        changed = true
    }

    for (const anchor of root.querySelectorAll<HTMLAnchorElement>('a[href]')) {
        const href = anchor.getAttribute('href') ?? ''
        if (!href || NON_FILE_HREF.test(href)) continue
        const location = parseFileLocation(safeDecodeURI(href))
        const path = refs.resolve(location.path)
        if (!path) continue
        fileLink(location, path, refs, anchor)
        changed = true
    }

    for (const code of root.querySelectorAll('code')) {
        if (code.closest('a, pre')) continue
        const location = codeFileCandidate(code.textContent)
        const path = location && refs.resolve(location.path)
        if (!location || !path) continue
        const link = fileLink(location, path, refs)
        code.replaceWith(link)
        link.append(code)
        changed = true
    }

    for (const node of textNodesOutsideLinksAndCode(root)) {
        const text = node.data
        let last = 0
        let fragment: DocumentFragment | null = null
        for (const candidate of textFileCandidates(text)) {
            const path = refs.resolve(candidate.location.path)
            if (!path) continue
            fragment ??= document.createDocumentFragment()
            if (candidate.index > last) fragment.append(text.slice(last, candidate.index))
            const link = fileLink(candidate.location, path, refs)
            link.textContent = candidate.text
            fragment.append(link)
            last = candidate.index + candidate.text.length
        }
        if (!fragment) continue
        if (last < text.length) fragment.append(text.slice(last))
        node.replaceWith(fragment)
        changed = true
    }

    return changed
}

function textNodesOutsideLinksAndCode(root: DocumentFragment): Text[] {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
    const nodes: Text[] = []
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (node.parentElement?.closest('a, code, pre')) continue
        nodes.push(node as Text)
    }
    return nodes
}

/**
 * Wrap task IDs in already-sanitized HTML in links. Runs on text nodes only and
 * builds the links with DOM APIs, so it cannot reintroduce markup.
 */
function linkTaskReferences(
    root: DocumentFragment,
    refs: NonNullable<RenderMarkdownOptions['taskReferences']>,
): boolean {
    let changed = false
    for (const node of textNodesOutsideLinksAndCode(root)) {
        const text = node.data
        let last = 0
        let fragment: DocumentFragment | null = null
        for (const match of text.matchAll(TASK_REFERENCE_CANDIDATE)) {
            const id = match[0]
            const parsed = parseTaskId(id)
            if (!parsed || !refs.isProject(parsed.project)) continue
            fragment ??= document.createDocumentFragment()
            const start = match.index
            if (start > last) fragment.append(text.slice(last, start))
            const link = document.createElement('a')
            link.className = TASK_REFERENCE_CLASS
            link.setAttribute(TASK_REFERENCE_ATTRIBUTE, id)
            link.href = refs.href(id)
            link.textContent = id
            fragment.append(link)
            last = start + id.length
        }
        if (!fragment) continue
        if (last < text.length) fragment.append(text.slice(last))
        node.replaceWith(fragment)
        changed = true
    }

    return changed
}
