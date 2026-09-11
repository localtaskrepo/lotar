import { describe, expect, it } from 'vitest'
import { numericOf, parseTaskId, projectOf, projectPrefixOfTaskId } from '../utils/text'

describe('parseTaskId (final-dash canonical grammar)', () => {
    it('splits at the final dash so hyphenated prefixes stay intact', () => {
        expect(parseTaskId('TP-1')).toEqual({ project: 'TP', number: '1' })
        expect(parseTaskId('ABC-OPS-12')).toEqual({ project: 'ABC-OPS', number: '12' })
        expect(parseTaskId('ABC-OPS-12-3')).toEqual({ project: 'ABC-OPS-12', number: '3' })
        expect(parseTaskId('42-7')).toEqual({ project: '42', number: '7' })
    })

    it('keeps prefixes exact case, including lower-case, digits and Unicode', () => {
        expect(parseTaskId('dev-ops-4')).toEqual({ project: 'dev-ops', number: '4' })
        expect(parseTaskId('ÜBER-2')).toEqual({ project: 'ÜBER', number: '2' })
        expect(parseTaskId('My_Project-2')).toEqual({ project: 'My_Project', number: '2' })
        expect(parseTaskId('ABC-OPS-12')).not.toEqual(parseTaskId('abc-ops-12'))
    })

    it('collapses padded aliases to the canonical number', () => {
        expect(parseTaskId('TP-001')).toEqual({ project: 'TP', number: '1' })
        expect(parseTaskId('ABC-OPS-012')).toEqual({ project: 'ABC-OPS', number: '12' })
    })

    it('accepts the u64 boundary exactly and rejects overflow', () => {
        expect(parseTaskId('TP-18446744073709551615')).toEqual({
            project: 'TP',
            number: '18446744073709551615',
        })
        expect(parseTaskId('TP-18446744073709551616')).toBeNull()
        expect(parseTaskId('TP-99999999999999999999999')).toBeNull()
    })

    it('fails closed for malformed ids without injecting a prefix', () => {
        const malformed = [
            '', 'TP123', 'TP-', 'TP--', 'TP-1x', 'TP-x1', 'TP-1.5', 'TP- 1', 'TP-1 ', 'TP-１',
            'TP-+12', 'TP-12+', '../evil-1', '..-1', '.-1', 'a/b-1', 'a\\b-1', '/tmp/abs-1',
            '@sprints-1', '.hidden-1', '-flag-1', 'a b-1', 'a..b-1', ' TP-1', 'TP-1\n',
        ]
        for (const bad of malformed) {
            expect(parseTaskId(bad), `expected null for ${JSON.stringify(bad)}`).toBeNull()
            expect(projectPrefixOfTaskId(bad), `expected null prefix for ${JSON.stringify(bad)}`).toBeNull()
        }
    })

    it('accepts backend-alphanumeric prefix chars beyond \\p{L}: Other_Alphabetic marks and Unicode numbers', () => {
        // Rust char::is_alphanumeric = Alphabetic | Number; these points are
        // rejected by \\p{L}\\p{N} but valid prefixes on the backend.
        expect(parseTaskId('\u0363X-1')).toEqual({ project: '\u0363X', number: '1' })
        expect(parseTaskId('A\u0345B-7')).toEqual({ project: 'A\u0345B', number: '7' })
        expect(parseTaskId('A\u0363-B-3')).toEqual({ project: 'A\u0363-B', number: '3' })
        expect(parseTaskId('\u2160-2')).toEqual({ project: '\u2160', number: '2' })
        expect(parseTaskId('\u0663-1')).toEqual({ project: '\u0663', number: '1' })
        expect(parseTaskId('\u00B2-4')).toEqual({ project: '\u00B2', number: '4' })
    })

    it('still fails closed where the backend does: format chars, lone surrogates, byte overlong marks', () => {
        expect(parseTaskId('A\u00AD-1')).toBeNull()
        expect(parseTaskId('A\uD800-1')).toBeNull()
        expect(parseTaskId('\u0345'.repeat(33) + '-1')).toBeNull()
        expect(parseTaskId('\u0345'.repeat(32) + '-1')).toEqual({
            project: '\u0345'.repeat(32),
            number: '1',
        })
    })

    it('keeps the numeric suffix ASCII-only even for Number-class digits', () => {
        expect(parseTaskId('TP-\u0663')).toBeNull()
        expect(parseTaskId('TP-\uFF11')).toBeNull()
        expect(parseTaskId('TP-\u2160')).toBeNull()
        expect(parseTaskId('TP-\u00B2')).toBeNull()
    })

    it('rejects prefixes longer than 64 bytes', () => {
        expect(parseTaskId(`${'A'.repeat(65)}-1`)).toBeNull()
        expect(parseTaskId(`${'A'.repeat(64)}-1`)).toEqual({ project: 'A'.repeat(64), number: '1' })
        expect(parseTaskId(`${'Ü'.repeat(33)}-1`)).toBeNull()
    })
})

describe('projectOf/numericOf display helpers', () => {
    it('derives display segments from the canonical parse', () => {
        expect(projectOf('ABC-OPS-12')).toBe('ABC-OPS')
        expect(numericOf('ABC-OPS-12')).toBe('12')
        expect(projectOf('dev-ops-4')).toBe('dev-ops')
        expect(numericOf('dev-ops-4')).toBe('4')
        expect(numericOf('ABC-OPS-012')).toBe('12')
    })

    it('shows exact u64 numbers for big ids without JS precision loss', () => {
        expect(numericOf('OPS-18446744073709551615')).toBe('18446744073709551615')
        expect(projectOf('OPS-18446744073709551615')).toBe('OPS')
        expect(numericOf('OPS-18446744073709551616')).toBe('')
    })

    it('returns empty segments for null or malformed ids', () => {
        expect(projectOf(null)).toBe('')
        expect(numericOf(undefined)).toBe('')
        expect(projectOf('TP123')).toBe('')
        expect(numericOf('TP-1x')).toBe('')
    })

    it('exposes a null prefix helper for config and attachment call sites', () => {
        expect(projectPrefixOfTaskId('ABC-OPS-12')).toBe('ABC-OPS')
        expect(projectPrefixOfTaskId('dev-ops-4')).toBe('dev-ops')
        expect(projectPrefixOfTaskId('TP123')).toBeNull()
        expect(projectPrefixOfTaskId('')).toBeNull()
    })
})
