import DOMPurify from 'dompurify'
import { micromark } from 'micromark'
import { gfm, gfmHtml } from 'micromark-extension-gfm'

export interface RenderMarkdownOptions {
    /**
     * Resolve `attachment:<stored name>` links and images (inserted when a
     * screenshot is pasted into a description) to their URLs. Without it they
     * are dropped by the sanitizer, like any unknown scheme.
     */
    resolveAttachment?: (stored: string) => string
}

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

    return DOMPurify.sanitize(rawHtml, {
        USE_PROFILES: {
            html: true,
        },
    })
}
