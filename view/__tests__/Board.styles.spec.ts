import { describe, expect, it } from 'vitest'
import BoardSource from '../pages/Board.vue?raw'

function extractStyle(source: string): string {
    const start = source.indexOf('<style')
    if (start === -1) return ''
    const openEnd = source.indexOf('>', start)
    if (openEnd === -1) return ''
    const close = source.indexOf('</style>', openEnd)
    if (close === -1) return ''
    return source.slice(openEnd + 1, close)
}

describe('Board styles', () => {
    it('keeps the task id on a single line', () => {
        const style = extractStyle(BoardSource)
        expect(style).toMatch(/\.task\s+\.id\s*\{[^}]*white-space\s*:\s*nowrap\s*;?/s)
        expect(style).toMatch(/\.task\s+\.id\s*\{[^}]*word-break\s*:\s*keep-all\s*;?/s)
    })
})
