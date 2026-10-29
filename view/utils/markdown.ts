import DOMPurify from 'dompurify'
import { micromark } from 'micromark'
import { gfm, gfmHtml } from 'micromark-extension-gfm'
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
}

/** Class and data attribute that mark a linked task reference. */
export const TASK_REFERENCE_CLASS = 'task-ref'
export const TASK_REFERENCE_ATTRIBUTE = 'data-task-ref'

// A run of ID characters ending in `-<digits>`, not glued to more ID characters
// on either side. The project prefix may contain dashes (`ABC-OPS-12`).
const TASK_REFERENCE_CANDIDATE =
    /(?<![\p{Alphabetic}\p{Number}_-])[\p{Alphabetic}\p{Number}][\p{Alphabetic}\p{Number}_-]*-[0-9]+(?![\p{Alphabetic}\p{Number}_-])/gu

export function renderMarkdownToHtml(source: string, options: RenderMarkdownOptions = {}): string {
    const markdown = source ?? ''

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

    return options.taskReferences ? linkTaskReferences(safeHtml, options.taskReferences) : safeHtml
}

/**
 * Wrap task IDs in already-sanitized HTML in links. Runs on text nodes only and
 * builds the links with DOM APIs, so it cannot reintroduce markup.
 */
function linkTaskReferences(html: string, refs: NonNullable<RenderMarkdownOptions['taskReferences']>): string {
    if (!/-[0-9]/.test(html)) return html
    const template = document.createElement('template')
    template.innerHTML = html

    const walker = document.createTreeWalker(template.content, NodeFilter.SHOW_TEXT)
    const textNodes: Text[] = []
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (node.parentElement?.closest('a, code, pre')) continue
        textNodes.push(node as Text)
    }

    let changed = false
    for (const node of textNodes) {
        const text = node.data
        let last = 0
        let fragment: DocumentFragment | null = null
        for (const match of text.matchAll(TASK_REFERENCE_CANDIDATE)) {
            const id = match[0]
            const parsed = parseTaskId(id)
            if (!parsed || !refs.isProject(parsed.project)) continue
            fragment ??= document.createDocumentFragment()
            const start = match.index ?? 0
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

    return changed ? template.innerHTML : html
}
