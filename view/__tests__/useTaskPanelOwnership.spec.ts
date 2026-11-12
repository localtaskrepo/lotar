import { describe, expect, it, vi } from 'vitest'
import { computed, nextTick, reactive, ref } from 'vue'
import { api } from '../api/client'
import { useTaskPanelOwnership } from '../composables/useTaskPanelOwnership'

vi.mock('../api/client', () => ({
    api: {
        whoami: vi.fn(async () => ''),
        listTasks: vi.fn(async () => ({ tasks: [{ id: 'A-1', reporter: 'priya', assignee: 'marco', tags: [] }] })),
        listAgentProfiles: vi.fn(async () => ({ profiles: [] })),
    },
}))

function setup() {
    const form = reactive<Record<string, any>>({ reporter: '', assignee: '' })
    const ownership = useTaskPanelOwnership({
        form,
        mode: computed(() => 'edit' as const),
        ready: ref(false),
        suppressWatch: ref(true),
        defaults: ref({}),
        members: ref([]),
        mergeKnownTags: () => {},
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
        vi.mocked(api.listTasks).mockResolvedValueOnce({
            tasks: [
                { id: 'A-1', reporter: '@priya', assignee: '@fix', tags: [] },
                { id: 'A-2', reporter: 'priya', assignee: 'marco', tags: [] },
            ],
        } as any)
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
})
