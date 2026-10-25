import { describe, expect, it } from 'vitest'
import { labelStride, niceTicks, seriesColor, tooltipX } from '../components/insights/chartFrame'

describe('chartFrame', () => {
    it('picks clean ticks that cover the maximum', () => {
        expect(niceTicks(0)).toEqual([0, 1])
        expect(niceTicks(7)).toEqual([0, 2, 4, 6, 8])
        expect(niceTicks(130)).toEqual([0, 50, 100, 150])
        // Counts never get fractional steps; durations may.
        expect(niceTicks(2)).toEqual([0, 1, 2])
        expect(niceTicks(2, false)).toEqual([0, 0.5, 1, 1.5, 2])
    })

    it('thins x labels when they would collide', () => {
        expect(labelStride(['Jan', 'Feb'], 200)).toBe(1)
        expect(labelStride(['Week of Jan 12', 'Week of Jan 19'], 30)).toBe(4)
        expect(labelStride([], 0)).toBe(19)
    })

    it('flips the tooltip left when it would overflow', () => {
        expect(tooltipX(100, 80, 120, 600)).toBe(100)
        expect(tooltipX(520, 500, 120, 600)).toBe(380)
        expect(tooltipX(40, 20, 120, 100)).toBe(0)
    })

    it('uses the muted colour beyond the four categorical slots', () => {
        expect(seriesColor(0)).toBe('var(--viz-1)')
        expect(seriesColor(4)).toBe('var(--viz-muted)')
    })
})
