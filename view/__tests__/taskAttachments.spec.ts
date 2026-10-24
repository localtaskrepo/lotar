import { describe, expect, it } from 'vitest'
import {
    attachmentDisplayName,
    attachmentHoverTitle,
    attachmentUrl,
    extractDroppedUrls,
    extractUrlsFromText,
    isImageAttachment,
    linkDisplayName,
    normalizeUrlCandidate,
} from '../utils/taskAttachments'

const HASH = '0123456789abcdef0123456789abcdef'

describe('task attachment helpers (DEV-116)', () => {
    it('builds hash URLs scoped to the task project and path URLs otherwise', () => {
        expect(attachmentUrl(`notes-${HASH}.txt`, 'REF-1')).toBe(`/api/attachments/h/${HASH}/notes.txt?project=REF`)
        expect(attachmentUrl(`notes.txt.${HASH}`, 'REF-1')).toBe(
            `/api/attachments/h/${HASH}/notes.txt.${HASH}?project=REF`,
        )
        expect(attachmentUrl('dir/plain.txt', 'REF-1')).toBe('/api/attachments/get?path=dir%2Fplain.txt&project=REF')
        expect(attachmentUrl('plain.txt', null)).toBe('/api/attachments/get?path=plain.txt')
        expect(attachmentUrl('  ', 'REF-1')).toBe('/api/attachments/get?path=')
    })

    it('strips content hashes from display names', () => {
        expect(attachmentDisplayName(`a/b/report-${HASH}.pdf`)).toBe('report.pdf')
        expect(attachmentDisplayName(`${'-'}${HASH}.png`)).toBe('attachment.png')
        expect(attachmentDisplayName('plain.txt')).toBe('plain.txt')
        expect(attachmentDisplayName('')).toBe('attachment')
    })

    it('resolves hover paths against the configured attachments dir', () => {
        expect(attachmentHoverTitle('x.txt', null)).toBe('.tasks/@attachments/x.txt')
        expect(attachmentHoverTitle('/x.txt', 'custom/')).toBe('.tasks/custom/x.txt')
        expect(attachmentHoverTitle('x.txt', '/abs/store/')).toBe('/abs/store/x.txt')
    })

    it('detects images by display name and formats link labels', () => {
        expect(isImageAttachment(`shot-${HASH}.PNG`)).toBe(true)
        expect(isImageAttachment('doc.pdf')).toBe(false)
        expect(linkDisplayName('https://example.com/a?b=1#c')).toBe('example.com/a?b=1#c')
        expect(linkDisplayName('not a url')).toBe('not a url')
    })

    it('normalizes only absolute http(s) URLs and dedupes text matches', () => {
        expect(normalizeUrlCandidate('<https://example.com/x>')).toBe('https://example.com/x')
        expect(normalizeUrlCandidate('https://example.com/x).')).toBe('https://example.com/x')
        expect(normalizeUrlCandidate('ftp://example.com')).toBeNull()
        expect(extractUrlsFromText('see https://a.test/1, and https://a.test/1 or http://b.test.')).toEqual([
            'https://a.test/1',
            'http://b.test',
        ])
    })

    it('reads dropped URLs from uri-list and Firefox payloads', () => {
        const data: Record<string, string> = {
            'text/uri-list': '# comment\nhttps://a.test/1\r\nnot-a-url\n',
            'text/x-moz-url': 'https://b.test/2\nTitle',
        }
        const event = { dataTransfer: { getData: (type: string) => data[type] ?? '' } } as unknown as DragEvent
        expect(extractDroppedUrls(event)).toEqual(['https://a.test/1', 'https://b.test/2'])
    })
})
