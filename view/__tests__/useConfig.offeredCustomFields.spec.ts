import { describe, expect, it, vi } from 'vitest'
import { useConfig } from '../composables/useConfig'
import { customFieldEntriesFromTask } from '../composables/useColumns'

const showConfigMock = vi.hoisted(() => vi.fn(async (_project?: string): Promise<Record<string, unknown>> => ({})))
vi.mock('../api/client', () => ({
    api: {
        showConfig: showConfigMock,
    },
}))

describe('offered custom fields (DEV-485)', () => {
    it('offers the fields the server lists, not only the declared ones', async () => {
        showConfigMock.mockResolvedValueOnce({
            custom_fields: ['*'],
            ui_custom_fields_from: 'both',
            custom_fields_offered: ['source', 'source_repo', 'finding'],
        })
        const config = useConfig()
        await config.refresh('INB')
        expect(config.customFields.value).toEqual(['*'])
        expect(config.offeredCustomFields.value).toEqual(['source', 'source_repo', 'finding'])
    })

    it('falls back to the declared names without the wildcard', async () => {
        showConfigMock.mockResolvedValueOnce({ custom_fields: ['*', 'estimate'] })
        const config = useConfig()
        await config.refresh('OLD')
        expect(config.offeredCustomFields.value).toEqual(['estimate'])
    })

    it('shows a merged field whatever case the task wrote it in', () => {
        const task = { custom_fields: { Source_Repo: 'lotar' } }
        expect(customFieldEntriesFromTask(task, ['source_repo'])).toEqual([
            { key: 'source_repo', label: 'source_repo', value: 'lotar' },
        ])
    })
})
