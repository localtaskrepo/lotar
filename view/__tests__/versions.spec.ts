import { describe, expect, it } from 'vitest'
import { compareVersionText, isVersion, versionFieldNames, versionValueError } from '../utils/versions'

describe('versions', () => {
    it('accepts what the server reads as a version', () => {
        for (const ok of [
            '0.13.3',
            'v0.13.3',
            'V1.0',
            '0.13',
            '2',
            '0.13.x',
            '1.0.0-rc.1',
            '1.0-rc1',
            '1.0.0-rc.1+build.5',
        ]) {
            expect(isVersion(ok), ok).toBe(true)
        }
        for (const bad of ['', 'v', 'abc', '1.2.3.4', '1..2', '1.2.3-', '1.2.3-rc..1', '1.a', '-1', '1.2.3-r$']) {
            expect(isVersion(bad), bad).toBe(false)
        }
    })

    it('checks every item of a list and allows an empty value', () => {
        expect(versionValueError('')).toBeNull()
        expect(versionValueError('0.13.2, v0.13.3')).toBeNull()
        expect(versionValueError('0.13.2, soon')).toContain('"soon" is not a version')
    })

    it('reads version field names from config properties', () => {
        expect(versionFieldNames({ affects: { type: 'version' }, product: { type: 'text' }, other: {} })).toEqual([
            'affects',
        ])
        expect(versionFieldNames(undefined)).toEqual([])
    })

    it('sorts in version order, a partial version just before its first release', () => {
        const values = [
            '0.13.10',
            'soon',
            '1.0.0',
            '0.13.2',
            '1.0.0-rc.1',
            '0.13',
            '1.0.0-alpha',
            '0.13.0',
            '1.0.0-rc.10',
            '0.9',
        ]
        expect([...values].sort(compareVersionText)).toEqual([
            '0.9',
            '0.13',
            '0.13.0',
            '0.13.2',
            '0.13.10',
            '1.0.0-alpha',
            '1.0.0-rc.1',
            '1.0.0-rc.10',
            '1.0.0',
            'soon',
        ])
    })
})
