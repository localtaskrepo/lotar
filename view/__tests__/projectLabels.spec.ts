import { flushPromises } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { formatProjectLabel } from '../utils/projectLabels'

vi.mock('../api/client', () => ({
    api: {
        listProjects: vi.fn(async () => ({
            projects: [
                { prefix: 'PLAT', name: 'Platform Engineering' },
                { prefix: 'OPS', name: 'OPS' },
            ],
            total: 2,
            limit: 500,
            offset: 0,
        })),
    },
}))

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

    it('shows a project named after its prefix once', () => {
        expect(formatProjectLabel({ name: 'DEV', prefix: 'DEV' })).toBe('DEV')
    })
})

describe('useProjectNames (DEV-184)', () => {
    it('shows names for known prefixes and leaves the rest as they are', async () => {
        const { useProjectNames } = await import('../composables/useProjectNames')
        const { projectName, projectLabel } = useProjectNames()
        await flushPromises()
        expect(projectName('PLAT')).toBe('Platform Engineering')
        expect(projectLabel('PLAT')).toBe('Platform Engineering (PLAT)')
        expect(projectName('OPS')).toBe('OPS')
        expect(projectName('NEW')).toBe('NEW')
        expect(projectName('')).toBe('')
    })
})
