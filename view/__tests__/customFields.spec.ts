import { describe, expect, it } from 'vitest'
import { customFieldValues, withCustomField } from '../utils/customFields'
import { compareVersionText } from '../utils/versions'

describe('custom fields across tasks (DEV-394)', () => {
    it('sets one field and keeps the others', () => {
        expect(withCustomField({ product: 'web', affects: '0.13' }, 'fixed_in', '0.15.0')).toEqual({
            product: 'web',
            affects: '0.13',
            fixed_in: '0.15.0',
        })
        expect(withCustomField(undefined, 'fixed_in', ' 0.15.0 ')).toEqual({ fixed_in: '0.15.0' })
    })

    it('replaces a spelling that differs only in case', () => {
        expect(withCustomField({ Fixed_In: '0.14', product: 'web' }, 'fixed_in', '0.15')).toEqual({
            product: 'web',
            fixed_in: '0.15',
        })
    })

    it('clears the field on an empty value', () => {
        expect(withCustomField({ fixed_in: '0.15', product: 'web' }, 'FIXED_IN', '  ')).toEqual({ product: 'web' })
    })

    it('reports no change', () => {
        expect(withCustomField({ product: 'web' }, 'fixed_in', '')).toBeNull()
        expect(withCustomField({ fixed_in: '0.15' }, 'fixed_in', '0.15')).toBeNull()
        expect(withCustomField({ affects: ['0.13.2', '0.13.3'] }, 'affects', '0.13.2, 0.13.3')).toBeNull()
    })

    it('reads values, a list as its items', () => {
        expect(customFieldValues({ Affects: ['0.13.2', ' ', '0.13.3'] }, 'affects')).toEqual(['0.13.2', '0.13.3'])
        expect(customFieldValues({ product: 'web' }, 'product')).toEqual(['web'])
        expect(customFieldValues({ product: '' }, 'product')).toEqual([])
        expect(customFieldValues({ meta: { a: 1 } }, 'meta')).toEqual([])
        expect(customFieldValues(null, 'product')).toEqual([])
    })
})

describe('compareVersionText', () => {
    it('orders by semver precedence, non-versions last', () => {
        const sorted = [
            '1.0.0',
            'soon',
            '0.13',
            '1.0.0-rc.1',
            '0.9.1',
            '1.0.0-alpha',
            '1.0.0-rc.10',
            'v0.12.4',
            '1.0.0-1',
        ]
        expect(sorted.sort(compareVersionText)).toEqual([
            '0.9.1',
            'v0.12.4',
            '0.13',
            '1.0.0-1',
            '1.0.0-alpha',
            '1.0.0-rc.1',
            '1.0.0-rc.10',
            '1.0.0',
            'soon',
        ])
    })
})
