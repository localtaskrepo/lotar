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
    })
    return { form, ownership }
}

describe('task panel ownership', () => {
    it('sets the picked owner on the form and trims it', async () => {
        const { form, ownership } = setup()
        ownership.setOwner('assignee', ' @fix-light ')
        ownership.setOwner('reporter', 'priya')
        expect(form.assignee).toBe('@fix-light')
        expect(form.reporter).toBe('priya')
        ownership.setOwner('assignee', '')
        expect(form.assignee).toBe('')
    })

    it('saves a pick right away when editing a task', async () => {
        const updateField = vi.fn()
        const form = reactive<Record<string, any>>({ reporter: '', assignee: 'marco' })
        const ownership = useTaskPanelOwnership({
            form,
            mode: computed(() => 'edit' as const),
            ready: ref(true),
            suppressWatch: ref(false),
            defaults: ref({}),
            members: ref([]),
            mergeKnownTags: () => {},
            updateField,
        })
        ownership.setOwner('assignee', '@review')
        await nextTick()
        await nextTick()
        expect(updateField).toHaveBeenCalledExactlyOnceWith('assignee')
        ownership.setOwner('assignee', '@review')
        await nextTick()
        await nextTick()
        expect(updateField).toHaveBeenCalledTimes(1)
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

        expect(ownership.agentProfileInfos.value.map((profile) => [profile.name, profile.runner])).toEqual([
            ['fix', 'claude'],
            ['review', 'codex'],
        ])
        ownership.setOwner('assignee', '@review')
        expect(form.assignee).toBe('@review')
    })

    it('still lists people when agent profiles fail to load', async () => {
        vi.mocked(api.listAgentProfiles).mockRejectedValueOnce(new Error('offline'))
        const { ownership } = setup()
        await ownership.preloadPeople('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['marco', 'priya'])
        expect(ownership.agentProfileInfos.value).toEqual([])
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
        })
        await ownership.preloadPeople('A')
        expect(ownership.orderedKnownUsers.value).toEqual(['@fix', 'lead', 'sam'])
    })
})
