import DOMPurify from 'dompurify'
import { micromark } from 'micromark'
import { gfm, gfmHtml } from 'micromark-extension-gfm'

export function renderMarkdownToHtml(source: string): string {
    const markdown = source ?? ''

    const rawHtml = micromark(markdown, {
        extensions: [gfm()],
        htmlExtensions: [gfmHtml()],
    })

    return DOMPurify.sanitize(rawHtml, {
        USE_PROFILES: {
            html: true,
        },
    })
}
