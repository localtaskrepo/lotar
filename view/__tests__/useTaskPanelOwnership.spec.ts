import { describe, expect, it, vi } from 'vitest'
import { computed, nextTick, reactive, ref } from 'vue'
import { api } from '../api/client'
import { useTaskPanelOwnership } from '../composables/useTaskPanelOwnership'

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => ''),
        taskValues: vi.fn(async () => ({ people: ['marco', 'priya'], tags: [], custom_fields: {} })),
        listAgentProfiles: vi.fn(async () => ({ profiles: [] })),
    },
}))

function setup(mergeKnownTags: (tags: Array<string | null | undefined>) => void = () => {}) {
    const form = reactive<Record<string, any>>({ reporter: '', assignee: '' })
    const ownership = useTaskPanelOwnership({
        form,
        mode: computed(() => 'edit' as const),
        ready: ref(false),
        suppressWatch: ref(true),
        defaults: ref({}),
        members: ref([]),
        mergeKnownTags,
        updateField: () => {},
        onFieldBlur: () => {},
    })
    return { form, ownership }
}

describe('task panel ownership controls', () => {
    it('leaves the custom input once the people list loads and knows the name', async () => {
        const { form, ownership } = setup()
        form.reporter = 'priya'
        form.assignee = 'marco'
        await nextTick()
        expect(ownership.reporterMode.value).toBe('custom')
        expect(ownership.assigneeMode.value).toBe('custom')

        await ownership.preloadPeople('A')
        await nextTick()
        expect(ownership.reporterMode.value).toBe('select')
        expect(ownership.reporterSelection.value).toBe('priya')
        expect(ownership.assigneeMode.value).toBe('select')
        expect(ownership.assigneeSelection.value).toBe('marco')
    })

    it('keeps a custom input the user chose', async () => {
        const { form, ownership } = setup()
        await ownership.preloadPeople('A')
        form.reporter = 'priya'
        await nextTick()
        ownership.setReporterSelection('__custom')
        expect(ownership.reporterMode.value).toBe('custom')

        ownership.knownUsers.value = [...ownership.knownUsers.value, 'sam']
        await nextTick()
        expect(ownership.reporterMode.value).toBe('custom')
    })

    it('offers agent profiles with their @ and keeps an @profile assignee', async () => {
        vi.mocked(api.listAgentProfiles).mockResolvedValueOnce({
            profiles: [
                { name: 'fix', runner: 'claude', permissions: 'workspace' },
                { name: 'review', runner: 'codex', permissions: 'read-only' },
            ],
        })
        vi.mocked(api.taskValues).mockResolvedValueOnce({
            people: ['@fix', '@priya', 'marco', 'priya'],
            tags: [],
            custom_fields: {},
        })
        const { form, ownership } = setup()
        await ownership.preloadPeople('A')
        expect(api.listAgentProfiles).toHaveBeenCalledWith('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['@fix', '@review', 'marco', 'priya'])

        form.assignee = '@fix'
        await nextTick()
        expect(ownership.assigneeMode.value).toBe('select')
        expect(ownership.assigneeSelection.value).toBe('@fix')

        ownership.setAssigneeSelection('@review')
        expect(form.assignee).toBe('@review')
    })

    it('still lists people when agent profiles fail to load', async () => {
        vi.mocked(api.listAgentProfiles).mockRejectedValueOnce(new Error('offline'))
        const { ownership } = setup()
        await ownership.preloadPeople('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['marco', 'priya'])
    })

    it('offers the people, tags and field values of the project without loading its tasks', async () => {
        vi.mocked(api.taskValues).mockResolvedValueOnce({
            people: ['marco', 'priya'],
            tags: ['api', 'web'],
            custom_fields: { affects: ['0.15', '0.15.0'], product: ['web'] },
        })
        const mergeKnownTags = vi.fn()
        const { ownership } = setup(mergeKnownTags)
        await ownership.preloadPeople(' A ')
        expect(api.taskValues).toHaveBeenLastCalledWith('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['marco', 'priya'])
        expect(mergeKnownTags).toHaveBeenCalledWith(['api', 'web'])
        expect(ownership.knownFieldValues.value).toEqual({ affects: ['0.15', '0.15.0'], product: ['web'] })
    })

    it('still offers configured members, agent profiles and the current user when the values fail to load', async () => {
        vi.mocked(api.taskValues).mockRejectedValueOnce(new Error('offline'))
        vi.mocked(api.listAgentProfiles).mockResolvedValueOnce({
            profiles: [{ name: 'fix', runner: 'claude', permissions: 'workspace' }],
        })
        const form = reactive<Record<string, any>>({ reporter: '', assignee: '' })
        const ownership = useTaskPanelOwnership({
            form,
            mode: computed(() => 'edit' as const),
            ready: ref(false),
            suppressWatch: ref(true),
            defaults: ref({ reporter: 'lead' }),
            members: ref(['sam']),
            mergeKnownTags: () => {},
            updateField: () => {},
            onFieldBlur: () => {},
        })
        await ownership.preloadPeople('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['@fix', 'lead', 'sam'])
    })
})
