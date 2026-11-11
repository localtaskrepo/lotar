import { describe, expect, it } from 'vitest'
import { asList, itemAt, mustGet, required } from '../utils/lookup'

describe('lookup helpers', () => {
    it('returns present values, including falsy ones', () => {
        expect(mustGet(new Map([['a', 0]]), 'a')).toBe(0)
        expect(itemAt(['', 'b'], 0)).toBe('')
        expect(itemAt('xy', 1)).toBe('y')
        expect(required(false, 'flag')).toBe(false)
    })

    it('throws a message naming what is missing', () => {
        expect(() => mustGet(new Map<string, number>(), 'a', 'rank')).toThrow('No rank for a')
        expect(() => itemAt([1, 2], 2, 'column')).toThrow('No column at index 2 of 2')
        expect(() => itemAt([1], -1)).toThrow('No item at index -1 of 1')
        expect(() => required(null, 'canvas context')).toThrow('Missing canvas context')
        expect(() => required(undefined, 'query handle')).toThrow('Missing query handle')
    })

    it('lists a present value once and a missing one not at all', () => {
        expect(asList(0)).toEqual([0])
        expect(asList(null)).toEqual([])
        expect(asList(undefined)).toEqual([])
    })
})
