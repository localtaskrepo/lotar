import { describe, expect, it } from 'vitest'
import { formatProjectLabel } from '../utils/projectLabels'

describe('formatProjectLabel', () => {
    it('includes the prefix alongside the truncated name when project name is provided', () => {
        const label = formatProjectLabel({
            name: 'Quality and Compliance Platform With Extended Name',
            prefix: 'QC',
        })

        expect(label).toBe('Quality and Compliance Plat… (QC)')
    })

    it('falls back to the prefix only when project name is missing', () => {
        const label = formatProjectLabel({
            name: '',
            prefix: 'OPS',
        })

        expect(label).toBe('OPS')
    })
})
